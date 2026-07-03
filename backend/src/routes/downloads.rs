use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::env;
use std::path::{Path as FsPath, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    hash_verify,
    models::{
        AddDownloadRequest, ApiError, CachedFileInfo, Download, DownloadEvent, DownloadNetworkRoute,
        DownloadStatus, DuplicateDownload, DuplicateGroup, ExpectedHash, FileChildInfo,
        FileInfo, HashAlgorithm, PublicSettings, WsEvent,
    },
    providers,
    ws::AppState,
};

#[derive(Debug, Clone)]
struct QueueCandidate {
    id: String,
    url: String,
    dest_path: String,
    provider: String,
    created_at: u64,
    priority: i32,
}

async fn cancel_provider_sidecar_download(provider: &str, url: &str, dest_path: &str) {
    let _ = (provider, url, dest_path);
}

#[derive(Debug, Deserialize)]
pub struct PriorityRequest {
    pub priority: i32,
}

#[derive(Debug, Deserialize)]
pub struct SpeedLimitRequest {
    pub speed_limit_kib: u64,
}

fn normalize_identity_url(url: &str) -> String {
    url.split('#').next().unwrap_or(url).trim().to_string()
}

fn expected_hash_from_url_fragment(url: &str) -> Option<ExpectedHash> {
    let fragment = url.split('#').nth(1)?;
    for part in fragment.split('&') {
        let (key, value) = part.split_once('=')?;
        let algorithm = match key.to_ascii_lowercase().as_str() {
            "md5" => HashAlgorithm::Md5,
            "sha1" => HashAlgorithm::Sha1,
            "sha256" => HashAlgorithm::Sha256,
            "crc32" => HashAlgorithm::Crc32,
            _ => continue,
        };
        let normalized = hash_verify::normalize_hash(value);
        if !normalized.is_empty() {
            return Some(ExpectedHash {
                algorithm,
                value: normalized,
            });
        }
    }
    None
}

fn download_identity_key(
    provider_name: &str,
    url: &str,
    is_folder: bool,
    selected_children: &Option<Vec<String>>,
) -> String {
    let mut selected = selected_children.clone().unwrap_or_default();
    selected.sort();
    format!(
        "{}::{}::{}::{}",
        providers::provider_id_from_name(provider_name),
        normalize_identity_url(url),
        if is_folder { "folder" } else { "file" },
        selected.join("|")
    )
}

fn expand_home(path: &str) -> String {
    if path == "~" {
        return env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .unwrap_or_else(|_| path.to_string());
    }

    if let Some(rest) = path.strip_prefix("~/") {
        let home = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .unwrap_or_else(|_| "~".to_string());
        return format!("{home}/{rest}");
    }

    path.to_string()
}

fn suffix_filename(filename: &str, suffix: &str) -> String {
    let path = FsPath::new(filename);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(filename);
    let ext = path.extension().and_then(|value| value.to_str());
    match ext {
        Some(ext) if !ext.is_empty() => format!("{stem}{suffix}.{ext}"),
        _ => format!("{stem}{suffix}"),
    }
}

fn replace_filename_extension(filename: &str, extension: &str) -> String {
    let clean_extension = extension.trim().trim_start_matches('.');
    if clean_extension.is_empty() {
        return filename.to_string();
    }

    let path = FsPath::new(filename);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(filename);
    format!("{stem}.{clean_extension}")
}

fn fragment_value(url: &str, key: &str) -> Option<String> {
    let fragment = url.split('#').nth(1)?;
    for part in fragment.split('&') {
        let (name, value) = part.split_once('=')?;
        if name == key {
            return urlencoding::decode(value).ok().map(|value| value.into_owned());
        }
    }
    None
}

fn selected_fragment_value(selected_children: &Option<Vec<String>>, key: &str) -> Option<String> {
    selected_children.as_ref().and_then(|children| {
        children
            .iter()
            .find_map(|child| fragment_value(child, key))
    })
}

fn normalize_youtube_merge_format(value: &str) -> Option<String> {
    let normalized = value.trim().trim_start_matches('.').to_ascii_lowercase();
    match normalized.as_str() {
        "mp4" | "mkv" | "webm" => Some(normalized),
        _ => None,
    }
}

fn selected_youtube_child_matches(child_url: &str, selected: &[String]) -> bool {
    selected.iter().any(|selected_url| {
        selected_url == child_url
            || fragment_value(selected_url, "ytdlp_format")
                .zip(fragment_value(child_url, "ytdlp_format"))
                .map(|(left, right)| left == right)
                .unwrap_or(false)
    })
}

fn unique_destination(dest_dir: &str, filename: &str) -> (String, String) {
    let mut candidate_name = filename.to_string();
    let mut candidate_path = PathBuf::from(dest_dir);
    candidate_path.push(&candidate_name);

    let mut index = 2;
    while candidate_path.exists() {
        candidate_name = suffix_filename(filename, &format!("_{index}"));
        candidate_path = PathBuf::from(dest_dir);
        candidate_path.push(&candidate_name);
        index += 1;
    }

    (candidate_name, candidate_path.to_string_lossy().to_string())
}

fn duplicate_sha256(expected_hash: &Option<ExpectedHash>) -> Option<String> {
    expected_hash.as_ref().and_then(|hash| {
        if matches!(hash.algorithm, HashAlgorithm::Sha256) {
            Some(hash.value.clone())
        } else {
            None
        }
    })
}

fn duplicate_http_error(duplicate: DuplicateDownload) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::CONFLICT,
        Json(ApiError::duplicate(
            "Arquivo já baixado anteriormente",
            duplicate,
        )),
    )
}

fn record_download_event(state: &AppState, download_id: &str, kind: &str, message: &str) {
    if let Ok(db) = state.db.lock() {
        let _ = crate::db::insert_download_event(&db, download_id, kind, message);
    }
}

fn cached_file_info_to_file_info(cached: CachedFileInfo) -> FileInfo {
    FileInfo {
        filename: cached.name,
        size: cached.size,
        mime_type: cached.mime_type,
        is_folder: cached.is_folder,
        children: cached.children,
        thumbnail_url: cached.thumbnail_url,
        channel_name: cached.channel_name,
        channel_thumbnail_url: cached.channel_thumbnail_url,
    }
}

// Adiciona um novo download à fila e inicia o processo em background
// POST /downloads — body: { "url": "...", "dest_dir": "..." }
pub async fn add_download(
    State(state): State<AppState>,
    Json(req): Json<AddDownloadRequest>,
) -> Result<Json<Download>, (StatusCode, Json<ApiError>)> {
    add_download_internal(state, req).await.map(Json)
}

pub async fn add_download_internal(
    state: AppState,
    req: AddDownloadRequest,
) -> Result<Download, (StatusCode, Json<ApiError>)> {
    // Detecta qual provider trata essa URL
    let provider = providers::detect_provider(&req.url).ok_or_else(|| {
        let error_msg = if req.url.contains("mega.nz/folder/") {
            "❌ URL de pasta do Mega inválida. Formato suportado:\n\
             • https://mega.nz/folder/HANDLE#KEY"
        } else if req.url.contains("mega.nz") {
            "❌ URL do Mega inválida. Formatos suportados:\n\
             • Novo: https://mega.nz/file/HANDLE#KEY\n\
             • Pasta: https://mega.nz/folder/HANDLE#KEY\n\
             • Antigo: https://mega.nz/#!HANDLE!KEY"
        } else if req.url.contains("mediafire.com") {
            "⚠️ URL do MediaFire não foi reconhecida.\n\
             Verifique se o link é válido e acessível.\n\
             O link pode estar expirado ou protegido."
        } else {
            "URL não reconhecida. Provedores suportados:\n\
             • Mega (mega.nz) — arquivos /file/ e pastas /folder/\n\
             • MediaFire (mediafire.com)\n\
             • Google Drive (drive.google.com)\n\
             • PixelDrain (pixeldrain.com)\n\
             • 1Fichier (1fichier.com)\n\
             • Drime (drime.cloud)\n\
             • Rapidgator (rapidgator.net)\n\
             • AkiraBox (akirabox.to)\n\
             • BRFiles (brfiles.com)\n\
             • MoonDL (moondl.com)\n\
             • Katfile (katfile.com / katfile.ws)\n\
             • Terabox (terabox.com)\n\
             • YouTube (youtube.com / youtu.be)\n\
             • OneDrive / SharePoint"
        };

        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(error_msg)),
        )
    })?;

    let selected_children = req
        .selected_children
        .clone()
        .filter(|children| !children.is_empty());

    // Busca informações do arquivo (nome, tamanho) antes de criar o item na fila.
    // Quando o capturador já leu o link, usa o cache salvo para evitar uma nova
    // rodada cara de metadados (especialmente no YouTube).
    let mut file_info = {
        let provider_id = providers::provider_id_from_name(provider.name());
        let cached = state
            .db
            .lock()
            .ok()
            .and_then(|db| crate::db::load_cached_file_info(&db, &req.url).ok())
            .flatten()
            .filter(|cached| cached.provider_id == provider_id);

        if let Some(cached) = cached {
            cached_file_info_to_file_info(cached)
        } else {
            let settings = state.db.lock().ok()
                .and_then(|db| crate::db::load_public_settings(&db).ok())
                .unwrap_or_default();
            let context = providers::DownloadContext {
                db_path: state.db_path.clone(),
                proxy_mode: settings.proxy_mode,
                proxy_host: settings.proxy_host,
                proxy_port: settings.proxy_port,
                proxy_username: settings.proxy_username,
                proxy_password: settings.proxy_password,
                youtube_use_cookies: settings.youtube_use_cookies,
                youtube_cookie_browser: settings.youtube_cookie_browser,
                youtube_cookies_file: settings.youtube_cookies_file,
                youtube_merge_format: settings.youtube_merge_format,
                youtube_download_subs: settings.youtube_download_subs,
                youtube_sub_langs: settings.youtube_sub_langs,
                youtube_embed_subs: settings.youtube_embed_subs,
                youtube_split_chapters: settings.youtube_split_chapters,
                request_headers: req.request_headers.clone().unwrap_or_default(),
                cached_channel_thumbnail_url: None,
            };
            let info = provider.get_file_info_with_context(&req.url, context).await.map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiError::new(format!("Falha ao obter informações do arquivo: {e}"))),
                )
            })?;
            if let Ok(db) = state.db.lock() {
                let _ = crate::db::save_cached_file_info(
                    &db,
                    &req.url,
                    provider_id,
                    &info.filename,
                    info.size,
                    info.mime_type.as_deref(),
                    info.is_folder,
                    &info.children,
                    info.thumbnail_url.as_deref(),
                    info.channel_name.as_deref(),
                    info.channel_thumbnail_url.as_deref(),
                );
            }
            info
        }
    };

    if file_info.is_folder {
        if let (Some(children), Some(selected)) = (file_info.children.as_mut(), selected_children.as_ref()) {
            let selected_set = selected.iter().cloned().collect::<std::collections::HashSet<_>>();
            children.retain(|child| {
                child.source_url
                    .as_ref()
                    .map(|source_url| selected_set.contains(source_url))
                    .unwrap_or(false)
            });

            if children.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ApiError::new("Nenhum arquivo selecionado da pasta pôde ser resolvido")),
                ));
            }

            file_info.size = children.iter().map(|child| child.size).sum();
        }
    } else if provider.name() == "YouTube" {
        if let Some(merge_format) = selected_fragment_value(&selected_children, "ytdlp_merge_format")
            .and_then(|value| normalize_youtube_merge_format(&value))
        {
            file_info.filename = replace_filename_extension(&file_info.filename, &merge_format);
        }

        if let (Some(children), Some(selected)) = (file_info.children.as_mut(), selected_children.as_ref()) {
            children.retain(|child| {
                child.source_url
                    .as_ref()
                    .map(|source_url| selected_youtube_child_matches(source_url, selected))
                    .unwrap_or(false)
            });
            if let Some(child) = children.first() {
                if child.size > 0 {
                    file_info.size = child.size;
                }
            }
        }
    }

    let dest_dir = expand_home(&req.dest_dir);
    tokio::fs::create_dir_all(&dest_dir).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(format!("Falha ao preparar a pasta de destino: {e}"))),
        )
    })?;

    let identity_key = download_identity_key(
        provider.name(),
        &req.url,
        file_info.is_folder,
        &selected_children,
    );
    let expected_hash = req
        .expected_hash
        .clone()
        .or_else(|| expected_hash_from_url_fragment(&req.url));
    let duplicate_action = req.duplicate_action.as_deref().unwrap_or("ask");
    let mut dest_path = format!(
        "{}/{}",
        dest_dir.trim_end_matches('/'),
        file_info.filename
    );

    {
        let map = state.downloads.lock().await;
        if let Some(existing) = map.values().find(|download| {
            download.dest_path == dest_path && download.identity_key == identity_key
        }) {
            return Ok(existing.clone());
        }
    }

    // Gera um ID único para este download (como crypto.randomUUID() no JS)
    let id = Uuid::new_v4().to_string();

    let mut duplicate = {
        let db_duplicate = state
            .db
            .lock()
            .ok()
            .and_then(|db| crate::db::find_completed_duplicate_by_identity(&db, &identity_key).ok())
            .flatten();
        if db_duplicate.is_some() {
            db_duplicate
        } else {
            duplicate_sha256(&expected_hash)
                .and_then(|hash| {
                    state
                        .db
                        .lock()
                        .ok()
                        .and_then(|db| crate::db::find_history_duplicate_by_sha256(&db, &hash).ok())
                        .flatten()
                })
        }
    };

    if let Some(existing) = duplicate.take() {
        state.broadcast(WsEvent::DuplicateDetected {
            id: id.clone(),
            existing_id: existing.id.clone(),
            existing_path: existing.path.clone(),
            filename: existing.filename.clone(),
        });

        match duplicate_action {
            "skip" => {
                if let Some(existing_download) = state.downloads.lock().await.get(&existing.id).cloned() {
                    return Ok(existing_download);
                }
                return Err(duplicate_http_error(existing));
            }
            "rename" => {
                let (renamed_filename, renamed_path) = unique_destination(&dest_dir, &file_info.filename);
                file_info.filename = renamed_filename;
                dest_path = renamed_path;
            }
            "always_download" => {}
            _ => return Err(duplicate_http_error(existing)),
        }
    }

    // Timestamp Unix atual em segundos
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let download = Download {
        id: id.clone(),
        url: req.url.clone(),
        provider: provider.name().to_string(),
        identity_key,
        filename: file_info.filename,
        size: file_info.size,
        dest_path: dest_path.clone(),
        status: DownloadStatus::Pending,
        bytes_downloaded: 0,
        speed_bps: 0,
        eta_secs: 0,
        is_folder: file_info.is_folder,
        children: file_info.children.map(|children| {
            children
                .into_iter()
                .map(|child| FileChildInfo {
                    bytes_downloaded: Some(0),
                    speed_bps: Some(0),
                    eta_secs: Some(0),
                    status: Some(DownloadStatus::Pending),
                    ..child
                })
                .collect()
        }),
        retry_count: 0,
        max_retries: req.max_retries.unwrap_or(0),
        speed_limit_kib: req.speed_limit_kib.unwrap_or(0),
        parallel_parts: req.parallel_parts.unwrap_or(1).max(1),
        selected_children: selected_children.clone(),
        expected_hash,
        retry_at: None,
        captcha_type: None,
        captcha_sitekey: None,
        captcha_page_url: None,
        captcha_token: None,
        error: None,
        priority: req.priority.unwrap_or(0),
        created_at: now,
        started_at: None,
        completed_at: None,
        last_progress_at: None,
        pinned: false,
        package_id: None,
        request_headers: req.request_headers.clone(),
        network_route: None,
        thumbnail_url: file_info.thumbnail_url,
        channel_name: file_info.channel_name,
        channel_thumbnail_url: file_info.channel_thumbnail_url,
        auto_tor_on_limit: req.auto_tor_on_limit.unwrap_or(false),
    };

    {
        let mut map = state.downloads.lock().await;
        map.insert(id.clone(), download.clone());
    }

    // Persiste no SQLite
    if let Ok(db) = state.db.lock() {
        let _ = crate::db::upsert(&db, &download);
        let _ = crate::db::insert_download_event(&db, &download.id, "created", "Adicionado à fila");
    }

    info!(
        target: "gdownloader_backend::downloads",
        "download adicionado id={} provider={} folder={} size={} dest={}",
        download.id,
        download.provider,
        download.is_folder,
        download.size,
        download.dest_path
    );
    schedule_pending_downloads(state.clone()).await;

    Ok(download)
}

// Lista todos os downloads (ativos, completos, com erro)
// GET /downloads
pub async fn list_downloads(State(state): State<AppState>) -> Json<Vec<Download>> {
    let map = state.downloads.lock().await;
    // Coleta os valores do HashMap em um Vec e ordena por prioridade e data de criação.
    let mut list: Vec<Download> = map.values().cloned().collect();
    list.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
    Json(list)
}

pub async fn list_duplicate_downloads(
    State(state): State<AppState>,
) -> Result<Json<Vec<DuplicateGroup>>, (StatusCode, Json<ApiError>)> {
    let db = state.db.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        )
    })?;
    let groups = crate::db::load_duplicate_groups(&db).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Falha ao buscar duplicatas: {error}"))),
        )
    })?;
    Ok(Json(groups))
}

pub async fn list_download_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<DownloadEvent>>, (StatusCode, Json<ApiError>)> {
    let db = state.db.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("Falha ao abrir o banco local")),
        )
    })?;
    let events = crate::db::list_download_events(&db, &id, 80).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Falha ao buscar histórico do download: {error}"))),
        )
    })?;
    Ok(Json(events))
}

// Cancela e remove um download da fila
// DELETE /downloads/:id
pub async fn cancel_download(
    State(state): State<AppState>,
    Path(id): Path<String>, // Path extrai o :id da URL
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let sidecar_download = {
        let map = state.downloads.lock().await;
        map.get(&id).map(|download| {
            (
                download.provider.clone(),
                download.url.clone(),
                download.dest_path.clone(),
            )
        })
    };

    let Some((provider, url, dest_path)) = sidecar_download else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("Download não encontrado")),
        ));
    };

    cancel_provider_sidecar_download(&provider, &url, &dest_path).await;

    if let Some(handle) = state.active_tasks.lock().await.remove(&id) {
        handle.abort();
    }

    let found = {
        let mut map = state.downloads.lock().await;
        if let Some(download) = map.get_mut(&id) {
            download.status = DownloadStatus::Cancelled;
            download.speed_bps = 0;
            download.eta_secs = 0;
            download.retry_at = None;
            download.error = Some("Cancelado pelo usuário".to_string());
            download.completed_at = Some(current_unix_secs());
            if let Some(children) = download.children.as_mut() {
                for child in children.iter_mut() {
                    child.speed_bps = Some(0);
                    child.eta_secs = Some(0);
                    child.status = Some(DownloadStatus::Cancelled);
                }
            }
            true
        } else {
            false
        }
    };

    if !found {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("Download não encontrado")),
        ));
    }

    persist_download_snapshot(&state, &id).await;
    record_download_event(&state, &id, "cancelled", "Cancelado pelo usuário");

    state.broadcast(WsEvent::Status {
        id: id.clone(),
        status: DownloadStatus::Cancelled,
    });
    schedule_pending_downloads(state).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let removed = {
        let mut map = state.downloads.lock().await;
        let Some(download) = map.get(&id) else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiError::new("Download não encontrado")),
            ));
        };

        if matches!(
            download.status,
            DownloadStatus::Pending
                | DownloadStatus::Downloading
                | DownloadStatus::Verifying
                | DownloadStatus::RateLimited
                | DownloadStatus::WaitingCaptcha
        ) {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError::new("Só é possível remover downloads encerrados da lista")),
            ));
        }

        map.remove(&id).is_some()
    };

    if removed {
        if let Ok(db) = state.db.lock() {
            let _ = crate::db::insert_download_event(&db, &id, "removed", "Removido da lista");
            let _ = crate::db::delete(&db, &id);
        }
        return Ok(StatusCode::NO_CONTENT);
    }

    Err((
        StatusCode::NOT_FOUND,
        Json(ApiError::new("Download não encontrado")),
    ))
}

pub async fn remove_download_with_files(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let (download, preserve_root) = {
        let map = state.downloads.lock().await;
        let Some(download) = map.get(&id) else {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiError::new("Download não encontrado")),
            ));
        };

        if matches!(download.status, DownloadStatus::Downloading | DownloadStatus::Verifying) {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError::new("Não é possível apagar arquivos físicos de um download ativo")),
            ));
        }

        let same_root_in_use = map
            .iter()
            .any(|(other_id, other)| other_id != &id && other.dest_path == download.dest_path);
        (download.clone(), same_root_in_use)
    };

    delete_download_artifacts_for_download(&download, preserve_root).await.map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new(format!("Falha ao apagar os arquivos físicos: {error}"))),
        )
    })?;

    {
        let mut map = state.downloads.lock().await;
        map.remove(&id);
    }

    if let Ok(db) = state.db.lock() {
        let _ = crate::db::insert_download_event(&db, &id, "removed_files", "Removido com arquivos físicos");
        let _ = crate::db::delete(&db, &id);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn clear_finished_downloads(
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    {
        let mut map = state.downloads.lock().await;
        map.retain(|_, download| {
            // Mantém estados ativos/em espera.
            let is_active = matches!(
                download.status,
                DownloadStatus::Pending
                    | DownloadStatus::Downloading
                    | DownloadStatus::Verifying
                    | DownloadStatus::Paused
                    | DownloadStatus::RateLimited
                    | DownloadStatus::WaitingCaptcha
            );
            // Mantém também downloads que falharam/foram interrompidos mas têm
            // progresso parcial recuperável (ex.: 1fichier na metade). "Limpar
            // concluídos" não deve apagar algo que o usuário ainda pode retomar.
            let has_resumable_progress = !matches!(download.status, DownloadStatus::Complete)
                && download.bytes_downloaded > 0
                && (download.size == 0 || download.bytes_downloaded < download.size);
            is_active || has_resumable_progress
        });
    }
    if let Ok(db) = state.db.lock() {
        let _ = crate::db::delete_finished(&db);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn pause_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let sidecar_download = {
        let map = state.downloads.lock().await;
        map.get(&id).map(|download| {
            (
                download.provider.clone(),
                download.url.clone(),
                download.dest_path.clone(),
            )
        })
    };

    let Some((provider, url, dest_path)) = sidecar_download else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("Download não encontrado")),
        ));
    };

    cancel_provider_sidecar_download(&provider, &url, &dest_path).await;

    if let Some(handle) = state.active_tasks.lock().await.remove(&id) {
        handle.abort();
    }

    let found = {
        let mut map = state.downloads.lock().await;
        if let Some(download) = map.get_mut(&id) {
            download.status = DownloadStatus::Paused;
            download.speed_bps = 0;
            download.eta_secs = 0;
            download.retry_at = None;
            download.error = None;
            if let Some(children) = download.children.as_mut() {
                for child in children.iter_mut() {
                    child.speed_bps = Some(0);
                    child.eta_secs = Some(0);
                    if child.status == Some(DownloadStatus::Downloading) {
                        child.status = Some(DownloadStatus::Paused);
                    }
                }
            }
            true
        } else {
            false
        }
    };

    if !found {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("Download não encontrado")),
        ));
    }

    persist_download_snapshot(&state, &id).await;
    record_download_event(&state, &id, "paused", "Pausado pelo usuário");
    state.broadcast(WsEvent::Status {
        id: id.clone(),
        status: DownloadStatus::Paused,
    });
    schedule_pending_downloads(state).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn resume_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    restart_download_internal(state, id, false).await
}

pub async fn retry_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    restart_download_internal(state, id, false).await
}

pub async fn restart_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    restart_download_internal(state, id, true).await
}

pub async fn force_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let candidate = {
        let mut map = state.downloads.lock().await;
        let download = map.get_mut(&id).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError::new("Download não encontrado")),
            )
        })?;

        if matches!(download.status, DownloadStatus::Downloading | DownloadStatus::Verifying) {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError::new("O download já está em andamento")),
            ));
        }

        download.status = DownloadStatus::Pending;
        download.retry_at = None;
        download.error = Some("Forçado pelo usuário".to_string());
        download.speed_bps = 0;
        download.eta_secs = 0;

        let url = if let Some(ref token) = download.captcha_token {
            format!("{}#captcha_token={token}", download.url)
        } else {
            download.url.clone()
        };

        QueueCandidate {
            id: download.id.clone(),
            url,
            dest_path: download.dest_path.clone(),
            provider: download.provider.clone(),
            created_at: download.created_at,
            priority: i32::MAX,
        }
    };

    persist_download_snapshot(&state, &id).await;
    record_download_event(&state, &id, "forced", "Forçado para iniciar agora");
    state.broadcast(WsEvent::Status {
        id: id.clone(),
        status: DownloadStatus::Pending,
    });

    let state_clone = state.clone();
    tokio::spawn(async move {
        run_download(state_clone, candidate.id, candidate.url, candidate.dest_path).await;
    });

    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_download_priority(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<PriorityRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let found = {
        let mut map = state.downloads.lock().await;
        if let Some(download) = map.get_mut(&id) {
            download.priority = req.priority;
            true
        } else {
            false
        }
    };

    if !found {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("Download não encontrado")),
        ));
    }

    persist_download_snapshot(&state, &id).await;
    record_download_event(&state, &id, "priority", &format!("Prioridade alterada para {}", req.priority));
    schedule_pending_downloads(state).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_download_speed_limit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SpeedLimitRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let found = {
        let mut map = state.downloads.lock().await;
        if let Some(download) = map.get_mut(&id) {
            download.speed_limit_kib = req.speed_limit_kib;
            true
        } else {
            false
        }
    };

    if !found {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("Download não encontrado")),
        ));
    }

    persist_download_snapshot(&state, &id).await;
    record_download_event(&state, &id, "speed_limit", &format!("Limite individual alterado para {} KiB/s", req.speed_limit_kib));
    Ok(StatusCode::NO_CONTENT)
}

// --- Executa o download em background ---
// Esta função roda em uma task separada do tokio
// É como um Worker ou uma Promise longa rodando em paralelo
async fn run_download(state: AppState, id: String, url: String, dest_path: String) {
    let (max_retries, speed_limit_bps, parallel_parts, selected_children) = {
        {
            let mut map = state.downloads.lock().await;
            if let Some(d) = map.get_mut(&id) {
                d.status = DownloadStatus::Downloading;
                d.error = None;
                d.retry_count = 0;
                d.retry_at = None;
                d.started_at = d.started_at.or(Some(current_unix_secs()));
                d.completed_at = None;
            }
        }
        let map = state.downloads.lock().await;
        let max_retries = map.get(&id).map(|d| d.max_retries).unwrap_or(0);
        let speed_limit_bps = map
            .get(&id)
            .and_then(|d| if d.speed_limit_kib > 0 { Some(d.speed_limit_kib * 1024) } else { None });
        let parallel_parts = map
            .get(&id)
            .map(|d| d.parallel_parts.max(1))
            .unwrap_or(1);
        let selected_children = map
            .get(&id)
            .and_then(|d| d.selected_children.clone());
        (max_retries, speed_limit_bps, parallel_parts, selected_children)
    };

    persist_download_snapshot(&state, &id).await;

    // Stall watchdog: if a running download produces no progress for this long,
    // the underlying process is considered hung. It is aborted and retried
    // automatically, independently of the normal error-retry budget.
    const STALL_TIMEOUT_SECS: u64 = 90;
    const STALL_MAX_RETRIES: u32 = 3;

    let mut attempt = 0u32;
    let mut stall_retries = 0u32;
    let mut tor_limit_retries: u32 = 0;
    loop {
        {
            let mut map = state.downloads.lock().await;
            let Some(d) = map.get_mut(&id) else {
                return;
            };

            if matches!(d.status, DownloadStatus::Paused | DownloadStatus::Cancelled) {
                reschedule_pending_downloads(state.clone());
                return;
            }

            d.status = DownloadStatus::Downloading;
            d.retry_at = None;
            d.retry_count = attempt;
            d.speed_bps = 0;
            d.eta_secs = 0;
            d.retry_at = None;
            d.error = None;
            d.started_at = d.started_at.or(Some(current_unix_secs()));
            d.completed_at = None;
        }
        persist_download_snapshot(&state, &id).await;

        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(64);

        let provider = match providers::detect_provider(&url) {
            Some(p) => p,
            None => {
                update_error(&state, &id, "Provider não encontrado para a URL").await;
                reschedule_pending_downloads(state.clone());
                return;
            }
        };
        let provider_name = provider.name().to_string();
        let effective_parallel_parts = if provider_name == "YouTube" {
            1
        } else {
            parallel_parts as usize
        };
        info!(
            target: "gdownloader_backend::downloads",
            "iniciando tentativa de download id={} provider={} attempt={} dest={}",
            id,
            provider_name,
            attempt,
            dest_path
        );

        // Check disk space before starting
        {
            let (file_size, dest_parent) = {
                let map = state.downloads.lock().await;
                let d = map.get(&id);
                let size = d.map(|d| d.size).unwrap_or(0);
                let parent = std::path::Path::new(&dest_path)
                    .parent()
                    .map(|p| p.to_path_buf());
                (size, parent)
            };

            if let Some(dest_path_obj) = dest_parent {
                use sysinfo::Disks;
                let disks = Disks::new_with_refreshed_list();
                let available = disks.iter()
                    .filter(|d| dest_path_obj.starts_with(d.mount_point()))
                    .map(|d| d.available_space())
                    .max()
                    .unwrap_or(u64::MAX);

                if file_size > 0 && file_size > available {
                    let error_msg = format!(
                        "Espaço em disco insuficiente. Necessário: {}MB, Disponível: {}MB",
                        file_size / 1_048_576,
                        available / 1_048_576
                    );
                    {
                        let mut map = state.downloads.lock().await;
                        if let Some(dl) = map.get_mut(&id) {
                            dl.status = DownloadStatus::DiskFull;
                            dl.error = Some(error_msg.clone());
                        }
                    }
                    persist_download_snapshot(&state, &id).await;
                    state.broadcast(WsEvent::StatusChanged {
                        id: id.clone(),
                        status: DownloadStatus::DiskFull,
                        error: Some(error_msg),
                        retry_at: None,
                        captcha_type: None,
                        captcha_sitekey: None,
                        captcha_page_url: None,
                    });
                    reschedule_pending_downloads(state.clone());
                    return;
                }
            }
        }

        let url_clone = url.clone();
        let dest_clone = dest_path.clone();
        let selected_children_clone = selected_children.clone();
        let request_headers = {
            let map = state.downloads.lock().await;
            map.get(&id)
                .and_then(|download| download.request_headers.clone())
                .unwrap_or_default()
        };
        let download_context = {
            let settings = state.db.lock().ok().and_then(|db| crate::db::load_public_settings(&db).ok()).unwrap_or_default();
            let route = ensure_download_network_route(&state, &id, &settings).await;
            if let Some(route_for_test) = route.clone().filter(|route| route.mode == "tor" && route.exit_ip.is_none()) {
                let state_for_test = state.clone();
                let id_for_test = id.clone();
                tokio::spawn(async move {
                    refresh_download_tor_exit(state_for_test, id_for_test, route_for_test).await;
                });
            }
            providers::DownloadContext {
                db_path: state.db_path.clone(),
                proxy_mode: route.as_ref().map(|route| route.mode.clone()).unwrap_or(settings.proxy_mode),
                proxy_host: route.as_ref().map(|route| route.proxy_host.clone()).unwrap_or(settings.proxy_host),
                proxy_port: route.as_ref().map(|route| route.proxy_port).unwrap_or(settings.proxy_port),
                proxy_username: route.as_ref().and_then(|route| route.proxy_username.clone()).or(settings.proxy_username),
                proxy_password: route.as_ref().and_then(|route| route.proxy_password.clone()).or(settings.proxy_password),
                youtube_use_cookies: settings.youtube_use_cookies,
                youtube_cookie_browser: settings.youtube_cookie_browser,
                youtube_cookies_file: settings.youtube_cookies_file,
                youtube_merge_format: settings.youtube_merge_format,
                youtube_download_subs: settings.youtube_download_subs,
                youtube_sub_langs: settings.youtube_sub_langs,
                youtube_embed_subs: settings.youtube_embed_subs,
                youtube_split_chapters: settings.youtube_split_chapters,
                request_headers,
                cached_channel_thumbnail_url: None,
            }
        };
        let download_task = tokio::spawn(async move {
            provider
                .download_with_context(
                    &url_clone,
                    &dest_clone,
                    speed_limit_bps,
                    effective_parallel_parts,
                    selected_children_clone,
                    progress_tx,
                    download_context,
                )
                .await
        });
        state
            .active_tasks
            .lock()
            .await
            .insert(id.clone(), download_task.abort_handle());

        let mut last_bytes = 0u64;
        let mut max_bytes_seen = 0u64;
        let mut last_time = std::time::Instant::now();
        let mut current_speed = 0u64;
        let mut last_db_write = std::time::Instant::now();
        let mut last_progress_broadcast = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_millis(500))
            .unwrap_or_else(std::time::Instant::now);
        let mut stalled = false;

        loop {
            let update = match tokio::time::timeout(
                std::time::Duration::from_secs(STALL_TIMEOUT_SECS),
                progress_rx.recv(),
            )
            .await
            {
                Ok(Some(update)) => update,
                // Channel closed: the download task finished (success or failure).
                Ok(None) => break,
                Err(_elapsed) => {
                    // No progress update arrived within the stall window. Only
                    // treat it as a hang if the download is still meant to be
                    // running (ignore paused/captcha/waiting states).
                    let status = {
                        let map = state.downloads.lock().await;
                        map.get(&id).map(|d| d.status.clone())
                    };
                    if matches!(status, Some(DownloadStatus::Downloading)) {
                        warn!(
                            target: "gdownloader_backend::downloads",
                            "download travado sem progresso por {}s id={} provider={}",
                            STALL_TIMEOUT_SECS,
                            id,
                            provider_name
                        );
                        stalled = true;
                        download_task.abort();
                        break;
                    }
                    continue;
                }
            };
            let should_persist_snapshot = last_db_write.elapsed().as_secs() >= 5;
            max_bytes_seen = max_bytes_seen.max(update.bytes_downloaded);
            let reported_bytes = max_bytes_seen;
            let elapsed = last_time.elapsed().as_secs_f64();
            if elapsed >= 0.5 {
                let delta = reported_bytes.saturating_sub(last_bytes);
                if delta > 0 {
                    current_speed = (delta as f64 / elapsed) as u64;
                }
                last_bytes = reported_bytes;
                last_time = std::time::Instant::now();
            }

            let speed = if reported_bytes == 0 {
                0
            } else {
                update.child_speed_bps.unwrap_or(current_speed)
            };

            let eta = update.child_eta_secs.unwrap_or_else(|| {
                if speed > 0 && update.total_bytes > 0 {
                    update.total_bytes.saturating_sub(reported_bytes) / speed
                } else {
                    0
                }
            });

            {
                let mut map = state.downloads.lock().await;
                if let Some(d) = map.get_mut(&id) {
                    d.bytes_downloaded = d.bytes_downloaded.max(reported_bytes);
                    d.speed_bps = speed;
                    d.eta_secs = eta;
                    d.last_progress_at = Some(current_unix_secs());
                    // Update total size if it wasn't set yet (can happen with some providers)
                    if update.total_bytes > 0 && d.size == 0 {
                        d.size = update.total_bytes;
                    }
                    if let Some(children) = d.children.as_mut() {
                        if let Some(child_filename) = update.child_filename.as_deref() {
                            let child_path = update.child_path.as_deref();
                            for child in children.iter_mut() {
                                let matches = child_path
                                    .map(|path| {
                                        child.path.as_deref() == Some(path)
                                            || child.source_url.as_deref() == Some(path)
                                            || child
                                                .source_url
                                                .as_deref()
                                                .map(|source_url| selected_youtube_child_matches(source_url, &[path.to_string()]))
                                                .unwrap_or(false)
                                    })
                                    .unwrap_or_else(|| child.filename == child_filename);

                                if matches {
                                    child.bytes_downloaded = match (child.bytes_downloaded, update.child_bytes_downloaded) {
                                        (Some(previous), Some(next)) => Some(previous.max(next)),
                                        (None, Some(next)) => Some(next),
                                        (current, None) => current,
                                    };
                                    child.speed_bps = update.child_speed_bps;
                                    child.eta_secs = update.child_eta_secs;
                                    child.status = Some(DownloadStatus::Downloading);
                                } else if child.status == Some(DownloadStatus::Downloading) {
                                    child.speed_bps = Some(0);
                                    child.eta_secs = Some(0);
                                }

                                if let (Some(bytes_downloaded), size) = (child.bytes_downloaded, child.size) {
                                    if size > 0 && bytes_downloaded >= size {
                                        child.bytes_downloaded = Some(size);
                                        child.speed_bps = Some(0);
                                        child.eta_secs = Some(0);
                                        child.status = Some(DownloadStatus::Complete);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if should_persist_snapshot {
                persist_download_snapshot(&state, &id).await;
                last_db_write = std::time::Instant::now();
            }

            if last_progress_broadcast.elapsed() >= std::time::Duration::from_millis(500)
                || update.total_bytes > 0 && reported_bytes >= update.total_bytes
            {
                last_progress_broadcast = std::time::Instant::now();
                state.broadcast(WsEvent::Progress {
                    id: id.clone(),
                    bytes: reported_bytes,
                    total: update.total_bytes,
                    speed,
                    eta,
                    status: DownloadStatus::Downloading,
                    child_path: update.child_path.clone(),
                    child_filename: update.child_filename.clone(),
                    child_bytes: update.child_bytes_downloaded,
                    child_total: update.child_total_bytes,
                    child_speed: update.child_speed_bps,
                    child_eta: update.child_eta_secs,
                });
            }
        }

        match download_task.await {
            Ok(Ok(bytes)) => {
                let verification = {
                    let mut map = state.downloads.lock().await;
                    if let Some(d) = map.get_mut(&id) {
                        d.expected_hash.clone().filter(|_| !d.is_folder)
                    } else {
                        None
                    }
                };

                if let Some(expected_hash) = verification {
                    match verify_completed_download(&state, &id, &dest_path, expected_hash).await {
                        Ok(true) => {}
                        Ok(false) => {
                            state.active_tasks.lock().await.remove(&id);
                            reschedule_pending_downloads(state.clone());
                            return;
                        }
                        Err(error) => {
                            state.active_tasks.lock().await.remove(&id);
                            update_error(&state, &id, &format!("Falha ao verificar hash: {error}")).await;
                            reschedule_pending_downloads(state.clone());
                            return;
                        }
                    }
                }

                {
                    let mut map = state.downloads.lock().await;
                    if let Some(d) = map.get_mut(&id) {
                        if bytes > 0 {
                            d.size = bytes;
                        }
                        d.status = DownloadStatus::Complete;
                        d.bytes_downloaded = d.size;
                        d.speed_bps = 0;
                        d.eta_secs = 0;
                        d.retry_at = None;
                        d.error = None;
                        d.completed_at = Some(current_unix_secs());
                        if let Some(children) = d.children.as_mut() {
                            let single_child = children.len() == 1;
                            for child in children.iter_mut() {
                                if bytes > 0 && single_child {
                                    child.size = bytes;
                                }
                                child.bytes_downloaded = Some(child.size);
                                child.speed_bps = Some(0);
                                child.eta_secs = Some(0);
                                child.status = Some(DownloadStatus::Complete);
                            }
                        }
                    }
                }
                state.active_tasks.lock().await.remove(&id);
                persist_download_snapshot(&state, &id).await;
                record_download_event(&state, &id, "completed", "Download concluído");
                info!(
                    target: "gdownloader_backend::downloads",
                    "download concluído id={} provider={} dest={}",
                    id,
                    provider_name,
                    dest_path
                );
                state.broadcast(WsEvent::Complete {
                    id: id.clone(),
                    path: dest_path,
                });

                // Trigger post-download action
                {
                    let settings = state.db.lock().ok()
                        .and_then(|db| crate::db::load_public_settings(&db).ok())
                        .unwrap_or_default();
                    let trigger = settings.post_download_action_trigger.clone();
                    let action = settings.post_download_action.clone();

                    if action != "none" {
                        let should_fire = if trigger == "per_item" {
                            true
                        } else {
                            // queue_empty: fire only when no more active/pending downloads
                            let map = state.downloads.lock().await;
                            !map.values().any(|d| matches!(d.status,
                                DownloadStatus::Downloading | DownloadStatus::Pending | DownloadStatus::RateLimited))
                        };

                        if should_fire {
                            let cmd = settings.post_download_command.clone();
                            let webhook = settings.post_download_webhook_url.clone();
                            let dl_id = id.clone();
                            tokio::spawn(async move {
                                execute_post_download_action(&action, &cmd, &webhook, &dl_id).await;
                            });
                        }
                    }
                }

                reschedule_pending_downloads(state.clone());
                return;
            }
            Ok(Err(e)) => {
                state.active_tasks.lock().await.remove(&id);
                let err_str = e.to_string();

                // Captcha required — set WaitingCaptcha and halt
                if let Some((captcha_type, sitekey, page_url)) = parse_captcha_error(&err_str) {
                    warn!(
                        target: "gdownloader_backend::downloads",
                        "download aguardando captcha id={} provider={} type={} page={}",
                        id,
                        provider_name,
                        captcha_type,
                        page_url
                    );
                    {
                        let mut map = state.downloads.lock().await;
                        if let Some(d) = map.get_mut(&id) {
                            d.status = DownloadStatus::WaitingCaptcha;
                            d.captcha_type = Some(captcha_type.clone());
                            d.captcha_sitekey = Some(sitekey.clone());
                            d.captcha_page_url = Some(page_url.clone());
                            d.speed_bps = 0;
                            d.eta_secs = 0;
                            d.completed_at = None;
                        }
                    }
                    state.broadcast(WsEvent::StatusChanged {
                        id: id.clone(),
                        status: DownloadStatus::WaitingCaptcha,
                        error: None,
                        retry_at: None,
                        captcha_type: Some(captcha_type),
                        captcha_sitekey: Some(sitekey),
                        captcha_page_url: Some(page_url),
                    });
                    persist_download_snapshot(&state, &id).await;
                    reschedule_pending_downloads(state.clone());
                    return;
                }

                let retry_policy = classify_retry_policy(&provider_name, &err_str, attempt);
                let is_rate_limit = err_str.starts_with("RATE_LIMIT:");
                let is_premium_required = err_str.starts_with("PREMIUM_REQUIRED:");
                let should_retry = !is_premium_required && (is_rate_limit || attempt < max_retries);
                if should_retry {
                    let settings = state.db.lock().ok()
                        .and_then(|db| crate::db::load_public_settings(&db).ok())
                        .unwrap_or_default();
                    // Flag por-download: "usar Tor ao atingir o limite". Quando
                    // ligada, rotaciona o circuito Tor e retenta — sem exigir o
                    // ajuste global de reconexão.
                    let download_auto_tor = {
                        let map = state.downloads.lock().await;
                        map.get(&id).map(|d| d.auto_tor_on_limit).unwrap_or(false)
                    };
                    let isolated_tor_port = *state.isolated_tor_port.lock().await;
                    let isolated_tor_retry = is_rate_limit
                        && should_assign_isolated_route(download_auto_tor, isolated_tor_port);
                    if isolated_tor_retry {
                        tor_limit_retries = tor_limit_retries.saturating_add(1);
                        if tor_limit_retry_exhausted(tor_limit_retries) {
                            state.active_tasks.lock().await.remove(&id);
                            update_error(
                                &state,
                                &id,
                                "Limite de tentativas via Tor atingido — o arquivo pode estar indisponível",
                            )
                            .await;
                            reschedule_pending_downloads(state.clone());
                            return;
                        }
                        let _ = rotate_download_tor_route(&state, &id, &settings).await;
                    }
                    let retry_delay_secs = if isolated_tor_retry { 3 } else { retry_policy.retry_delay_secs };
                    let retry_at = current_unix_secs().saturating_add(retry_delay_secs);
                    let wait_status = if is_rate_limit && !isolated_tor_retry { DownloadStatus::RateLimited } else { DownloadStatus::Pending };
                    {
                        let mut map = state.downloads.lock().await;
                        if let Some(d) = map.get_mut(&id) {
                            d.status = wait_status.clone();
                            d.speed_bps = 0;
                            d.eta_secs = retry_delay_secs;
                            d.retry_at = Some(retry_at);
                            d.error = Some(retry_policy.wait_message.clone());
                            d.completed_at = None;
                            if let Some(children) = d.children.as_mut() {
                                for child in children.iter_mut() {
                                    child.speed_bps = Some(0);
                                    child.eta_secs = Some(0);
                                    if child.status == Some(DownloadStatus::Downloading) {
                                        child.status = Some(DownloadStatus::Pending);
                                    }
                                }
                            }
                        }
                    }
                    persist_download_snapshot(&state, &id).await;
                    warn!(
                        target: "gdownloader_backend::downloads",
                        "download reagendado id={} provider={} delay={}s status={} reason={}",
                        id,
                        provider_name,
                        retry_delay_secs,
                        if is_rate_limit { "rate_limited" } else { "pending" },
                        retry_policy.wait_message
                    );
                    state.broadcast(WsEvent::StatusChanged {
                        id: id.clone(),
                        status: wait_status,
                        error: Some(retry_policy.wait_message.clone()),
                        retry_at: Some(retry_at),
                        captcha_type: None,
                        captcha_sitekey: None,
                        captcha_page_url: None,
                    });
                    reschedule_pending_downloads(state.clone());

                    // Try reconnect for rate-limited downloads before waiting
                    if is_rate_limit && !isolated_tor_retry {
                        let reconnect_cfg = (settings.use_reconnect_on_rate_limit, crate::reconnect::ReconnectConfig {
                            method: settings.reconnect_method,
                            command: settings.reconnect_command,
                            router_ip: settings.router_ip,
                        });
                        if reconnect_cfg.0 {
                            match crate::reconnect::attempt_reconnect(&reconnect_cfg.1).await {
                                Ok(true) => {
                                    // Reconnect succeeded — skip the wait and retry immediately
                                    if !is_rate_limit {
                                        attempt = attempt.saturating_add(1);
                                    }
                                    continue;
                                }
                                Ok(false) => {}
                                Err(e) => {
                                    warn!(target: "gdownloader_backend::downloads",
                                        "reconnect error for id={}: {}", id, e);
                                }
                            }
                        }
                    }

                    schedule_retry_wakeup(state.clone(), retry_at);

                    for _ in 0..retry_delay_secs {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        let status = {
                            let map = state.downloads.lock().await;
                            map.get(&id).map(|download| download.status.clone())
                        };
                        match status {
                            None | Some(DownloadStatus::Paused | DownloadStatus::Cancelled) => {
                                reschedule_pending_downloads(state.clone());
                                return;
                            }
                            // WaitingCaptcha breaks the sleep loop: captcha was submitted, re-attempt
                            Some(DownloadStatus::Pending) => break,
                            _ => {}
                        }
                    }
                    if !is_rate_limit {
                        attempt = attempt.saturating_add(1);
                    }
                    continue;
                }
                warn!(
                    target: "gdownloader_backend::downloads",
                    "download falhou definitivamente id={} provider={} error={}",
                    id,
                    provider_name,
                    retry_policy.final_message
                );
                update_error(&state, &id, &retry_policy.final_message).await;
                reschedule_pending_downloads(state.clone());
                return;
            }
            Err(_) => {
                state.active_tasks.lock().await.remove(&id);
                let status = {
                    let map = state.downloads.lock().await;
                    map.get(&id).map(|download| download.status.clone())
                };
                if matches!(status, Some(DownloadStatus::Paused | DownloadStatus::Cancelled)) {
                    reschedule_pending_downloads(state.clone());
                    return;
                }
                // Hung download detected by the stall watchdog: retry automatically
                // up to STALL_MAX_RETRIES, regardless of the normal retry budget.
                if stalled && stall_retries < STALL_MAX_RETRIES {
                    stall_retries = stall_retries.saturating_add(1);
                    let retry_message = format!(
                        "Download travou — tentando novamente ({}/{})",
                        stall_retries, STALL_MAX_RETRIES
                    );
                    warn!(
                        target: "gdownloader_backend::downloads",
                        "download travado reagendado id={} provider={} stall_retry={}",
                        id,
                        provider_name,
                        stall_retries
                    );
                    let retry_at = current_unix_secs().saturating_add(3);
                    {
                        let mut map = state.downloads.lock().await;
                        if let Some(d) = map.get_mut(&id) {
                            d.status = DownloadStatus::Pending;
                            d.speed_bps = 0;
                            d.eta_secs = 0;
                            d.retry_at = Some(retry_at);
                            d.error = Some(retry_message.clone());
                            d.completed_at = None;
                            if let Some(children) = d.children.as_mut() {
                                for child in children.iter_mut() {
                                    child.speed_bps = Some(0);
                                    child.eta_secs = Some(0);
                                    if child.status == Some(DownloadStatus::Downloading) {
                                        child.status = Some(DownloadStatus::Pending);
                                    }
                                }
                            }
                        }
                    }
                    persist_download_snapshot(&state, &id).await;
                    state.broadcast(WsEvent::StatusChanged {
                        id: id.clone(),
                        status: DownloadStatus::Pending,
                        error: Some(retry_message),
                        retry_at: Some(retry_at),
                        captcha_type: None,
                        captcha_sitekey: None,
                        captcha_page_url: None,
                    });
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    continue;
                }
                if attempt < max_retries {
                    warn!(
                        target: "gdownloader_backend::downloads",
                        "task abortada, tentando novamente id={} provider={} next_attempt={}",
                        id,
                        provider_name,
                        attempt + 1
                    );
                    tokio::time::sleep(std::time::Duration::from_secs((attempt + 1) as u64)).await;
                    attempt = attempt.saturating_add(1);
                    continue;
                }
                warn!(
                    target: "gdownloader_backend::downloads",
                    "task abortada definitivamente id={} provider={}",
                    id,
                    provider_name
                );
                update_error(
                    &state,
                    &id,
                    if stalled {
                        "Download travou repetidamente sem progresso"
                    } else {
                        "Download interrompido"
                    },
                )
                .await;
                reschedule_pending_downloads(state.clone());
                return;
            }
        }
    }
}

/// Decide se um download deve receber uma rota Tor isolada. Puro e testável:
/// só depende da flag por-download e de o daemon Tor estar rodando (porta runtime).
fn should_assign_isolated_route(auto_tor_on_limit: bool, isolated_tor_port: Option<u16>) -> bool {
    auto_tor_on_limit && isolated_tor_port.is_some()
}

/// Teto de tentativas no modo Tor isolado antes de desistir e marcar erro.
const TOR_LIMIT_MAX_RETRIES: u32 = 50;

/// Puro e testável: indica se o modo Tor esgotou o teto de tentativas.
fn tor_limit_retry_exhausted(tor_limit_retries: u32) -> bool {
    tor_limit_retries >= TOR_LIMIT_MAX_RETRIES
}

async fn ensure_download_network_route(
    state: &AppState,
    id: &str,
    settings: &PublicSettings,
) -> Option<DownloadNetworkRoute> {
    let isolated_port = *state.isolated_tor_port.lock().await;
    // Modo global "tudo via Tor" continua funcionando como antes.
    let global_tor = settings.proxy_mode == "tor" && settings.start_tor;

    let mut changed = false;
    let route = {
        let mut map = state.downloads.lock().await;
        let Some(download) = map.get_mut(id) else {
            return None;
        };
        // Rota isolada por-download: flag ligada + daemon Tor rodando, mesmo que
        // o proxy global esteja desligado.
        let want_isolated = should_assign_isolated_route(download.auto_tor_on_limit, isolated_port);
        if !global_tor && !want_isolated {
            return None;
        }
        let needs_new = download
            .network_route
            .as_ref()
            .map(|route| route.mode != "tor" || route.proxy_username.is_none() || route.proxy_password.is_none())
            .unwrap_or(true);
        if needs_new {
            download.network_route = Some(new_tor_route(id, settings, 0, isolated_port));
            changed = true;
        }
        download.network_route.clone()
    };

    if changed {
        persist_download_snapshot(state, id).await;
        record_download_event(state, id, "network", "Circuito Tor isolado atribuído ao download");
    }

    route
}

async fn rotate_download_tor_route(
    state: &AppState,
    id: &str,
    settings: &PublicSettings,
) -> Option<DownloadNetworkRoute> {
    let isolated_port = *state.isolated_tor_port.lock().await;
    let global_tor = settings.proxy_mode == "tor" && settings.start_tor;
    let want_isolated = {
        let map = state.downloads.lock().await;
        map.get(id)
            .map(|d| should_assign_isolated_route(d.auto_tor_on_limit, isolated_port))
            .unwrap_or(false)
    };
    if !global_tor && !want_isolated {
        return None;
    }

    let route = {
        let mut map = state.downloads.lock().await;
        let Some(download) = map.get_mut(id) else {
            return None;
        };
        let next_generation = download
            .network_route
            .as_ref()
            .map(|route| route.circuit_changes.saturating_add(1))
            .unwrap_or(1);
        download.network_route = Some(new_tor_route(id, settings, next_generation, isolated_port));
        download.network_route.clone()
    };

    persist_download_snapshot(state, id).await;
    record_download_event(
        state,
        id,
        "network",
        "Rate-limit detectado; circuito Tor isolado trocado para este download",
    );
    route
}

async fn refresh_download_tor_exit(state: AppState, id: String, route: DownloadNetworkRoute) {
    let result = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let client = <crate::providers::direct_http::DirectHttpProvider as crate::providers::ProviderDefaults>::http_client_with_proxy(
            &route.mode,
            &route.proxy_host,
            route.proxy_port,
            route.proxy_username.as_deref(),
            route.proxy_password.as_deref(),
        )?;
        let json = client
            .get("https://check.torproject.org/api/ip")
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        Ok::<(String, bool), anyhow::Error>((
            json["IP"]
                .as_str()
                .or_else(|| json["origin"].as_str())
                .unwrap_or("unknown")
                .to_string(),
            json["IsTor"].as_bool().unwrap_or(false),
        ))
    })
    .await;

    let Ok(Ok((ip, is_tor))) = result else {
        return;
    };
    if !is_tor {
        return;
    }

    {
        let mut map = state.downloads.lock().await;
        if let Some(download) = map.get_mut(&id) {
            if let Some(current) = download.network_route.as_mut() {
                if current.proxy_username == route.proxy_username {
                    current.exit_ip = Some(ip.clone());
                    current.last_checked_at = Some(current_unix_secs());
                }
            }
        }
    }
    persist_download_snapshot(&state, &id).await;
    record_download_event(&state, &id, "network", &format!("Saída Tor validada: {ip}"));
}

fn new_tor_route(id: &str, settings: &PublicSettings, circuit_changes: u32, isolated_port: Option<u16>) -> DownloadNetworkRoute {
    let nonce = Uuid::new_v4().simple().to_string();
    let short_id = id.chars().take(8).collect::<String>();
    let port = isolated_port
        .filter(|value| *value > 0)
        .or_else(|| if settings.proxy_port == 0 { None } else { Some(settings.proxy_port) })
        .unwrap_or(9150);
    DownloadNetworkRoute {
        mode: "tor".to_string(),
        isolated: true,
        proxy_host: if settings.proxy_host.trim().is_empty() {
            "127.0.0.1".to_string()
        } else {
            settings.proxy_host.clone()
        },
        proxy_port: port,
        proxy_username: Some(format!("gdl-{short_id}-{circuit_changes}")),
        proxy_password: Some(nonce),
        exit_ip: None,
        exit_country: None,
        exit_country_code: None,
        circuit_changes,
        last_checked_at: Some(current_unix_secs()),
    }
}

fn reschedule_pending_downloads(state: AppState) {
    tokio::spawn(async move {
        schedule_pending_downloads(state).await;
    });
}

async fn execute_post_download_action(action: &str, command: &str, webhook_url: &str, download_id: &str) {
    match action {
        "shutdown" => {
            #[cfg(target_os = "macos")]
            let _ = tokio::process::Command::new("osascript")
                .args(["-e", "tell app \"System Events\" to shut down"])
                .status().await;
            #[cfg(target_os = "windows")]
            let _ = tokio::process::Command::new("shutdown")
                .args(["/s", "/t", "30"])
                .status().await;
            #[cfg(target_os = "linux")]
            let _ = tokio::process::Command::new("shutdown")
                .args(["-h", "now"])
                .status().await;
        }
        "sleep" => {
            #[cfg(target_os = "macos")]
            let _ = tokio::process::Command::new("pmset")
                .args(["sleepnow"])
                .status().await;
            #[cfg(target_os = "windows")]
            let _ = tokio::process::Command::new("rundll32.exe")
                .args(["powrprof.dll,SetSuspendState", "0,1,0"])
                .status().await;
            #[cfg(target_os = "linux")]
            let _ = tokio::process::Command::new("systemctl")
                .args(["suspend"])
                .status().await;
        }
        "custom_command" if !command.is_empty() => {
            let parts: Vec<&str> = command.splitn(2, ' ').collect();
            if !parts.is_empty() {
                let mut cmd = tokio::process::Command::new(parts[0]);
                if parts.len() > 1 {
                    cmd.args(parts[1].split_whitespace());
                }
                let _ = cmd.status().await;
            }
        }
        "webhook" if !webhook_url.is_empty() => {
            if let Ok(client) = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build() {
                let _ = client.post(webhook_url)
                    .json(&serde_json::json!({ "event": "download_complete", "downloadId": download_id }))
                    .send().await;
            }
        }
        _ => {}
    }
}

async fn verify_completed_download(
    state: &AppState,
    id: &str,
    dest_path: &str,
    expected_hash: ExpectedHash,
) -> anyhow::Result<bool> {
    {
        let mut map = state.downloads.lock().await;
        if let Some(download) = map.get_mut(id) {
            download.status = DownloadStatus::Verifying;
            download.speed_bps = 0;
            download.eta_secs = 0;
            download.error = None;
            download.completed_at = None;
        }
    }
    persist_download_snapshot(state, id).await;
    state.broadcast(WsEvent::StatusChanged {
        id: id.to_string(),
        status: DownloadStatus::Verifying,
        error: None,
        retry_at: None,
        captcha_type: None,
        captcha_sitekey: None,
        captcha_page_url: None,
    });

    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(32);
    let verify_task = hash_verify::verify_file(dest_path.to_string(), expected_hash.clone(), progress_tx);
    tokio::pin!(verify_task);

    loop {
        tokio::select! {
            progress = progress_rx.recv() => {
                let Some(progress) = progress else {
                    continue;
                };
                {
                    let mut map = state.downloads.lock().await;
                    if let Some(download) = map.get_mut(id) {
                        download.bytes_downloaded = progress.bytes_done;
                        download.size = progress.bytes_total.max(download.size);
                        download.last_progress_at = Some(current_unix_secs());
                    }
                }
                state.broadcast(WsEvent::Verifying {
                    id: id.to_string(),
                    bytes_done: progress.bytes_done,
                    bytes_total: progress.bytes_total,
                    algorithm: expected_hash.algorithm.clone(),
                });
            }
            result = &mut verify_task => {
                let result = result?;
                if result.matched {
                    return Ok(true);
                }

                let message = format!(
                    "Hash inválido ({}): esperado {}, obtido {}",
                    hash_algorithm_label(&expected_hash.algorithm),
                    result.expected,
                    result.actual
                );
                {
                    let mut map = state.downloads.lock().await;
                    if let Some(download) = map.get_mut(id) {
                        download.status = DownloadStatus::Corrupted;
                        download.speed_bps = 0;
                        download.eta_secs = 0;
                        download.error = Some(message.clone());
                        download.completed_at = Some(current_unix_secs());
                    }
                }
                persist_download_snapshot(state, id).await;
                record_download_event(state, id, "corrupted", "Hash inválido na verificação");
                state.broadcast(WsEvent::StatusChanged {
                    id: id.to_string(),
                    status: DownloadStatus::Corrupted,
                    error: Some(message),
                    retry_at: None,
                    captcha_type: None,
                    captcha_sitekey: None,
                    captcha_page_url: None,
                });
                return Ok(false);
            }
        }
    }
}

fn hash_algorithm_label(algorithm: &HashAlgorithm) -> &'static str {
    match algorithm {
        HashAlgorithm::Md5 => "MD5",
        HashAlgorithm::Sha1 => "SHA1",
        HashAlgorithm::Sha256 => "SHA256",
        HashAlgorithm::Crc32 => "CRC32",
    }
}

async fn persist_download_snapshot(state: &AppState, id: &str) {
    let snapshot = {
        let map = state.downloads.lock().await;
        map.get(id).cloned()
    };

    if let Some(download) = snapshot {
        if let Ok(db) = state.db.lock() {
            if let Err(error) = crate::db::upsert(&db, &download) {
                warn!(
                    target: "gdownloader_backend::downloads",
                    "falha ao persistir download id={} status={:?}: {}",
                    id, download.status, error
                );
            }
        } else {
            warn!(
                target: "gdownloader_backend::downloads",
                "falha ao obter lock do DB para persistir download id={}",
                id
            );
        }
    }
}

pub async fn schedule_pending_downloads(state: AppState) {
    let limit = *state.max_concurrent_downloads.lock().await;
    let active_task_ids = {
        let tasks = state.active_tasks.lock().await;
        tasks.keys().cloned().collect::<std::collections::HashSet<_>>()
    };
    let to_start = {
        let mut map = state.downloads.lock().await;
        let active_count = map.values().map(active_file_units).sum::<usize>();
        debug!(
            target: "gdownloader_backend::downloads",
            "scheduler tick active_count={} limit={} queue_size={}",
            active_count,
            limit,
            map.len()
        );
        if active_count >= limit {
            schedule_next_retry_wakeup(state.clone(), &map);
            return;
        }

        let slots = limit.saturating_sub(active_count);
        if slots == 0 {
            schedule_next_retry_wakeup(state.clone(), &map);
            return;
        }

        let now = current_unix_secs();
        let mut active_by_provider = std::collections::HashMap::<String, usize>::new();
        for download in map.values() {
            let units = active_file_units(download);
            if units == 0 {
                continue;
            }
            *active_by_provider.entry(download.provider.clone()).or_insert(0) += units;
        }

        let pending = map
            .values()
            .filter(|download| {
                !active_task_ids.contains(&download.id)
                    &&
                matches!(download.status, DownloadStatus::Pending | DownloadStatus::RateLimited)
                    && download.retry_at.map(|retry_at| retry_at <= now).unwrap_or(true)
            })
            .map(|download| {
                let url = if let Some(ref token) = download.captcha_token {
                    format!("{}#captcha_token={}", download.url, token)
                } else {
                    download.url.clone()
                };

                QueueCandidate {
                    id: download.id.clone(),
                    url,
                    dest_path: download.dest_path.clone(),
                    provider: download.provider.clone(),
                    created_at: download.created_at,
                    priority: download.priority,
                }
            })
            .collect::<Vec<_>>();

        let selected = select_downloads_to_start(pending, &active_by_provider, slots);
        if selected.is_empty() {
            schedule_next_retry_wakeup(state.clone(), &map);
        } else {
            debug!(
                target: "gdownloader_backend::downloads",
                "scheduler selecionou downloads {:?}",
                selected.iter().map(|item| item.id.as_str()).collect::<Vec<_>>()
            );
        }

        for candidate in &selected {
            if let Some(download) = map.get_mut(&candidate.id) {
                download.status = DownloadStatus::Downloading;
                download.error = None;
                download.speed_bps = 0;
                download.eta_secs = 0;
                download.retry_at = None;
                download.started_at = download.started_at.or(Some(now));
            }
        }

        selected
    };

    for candidate in to_start {
        let state_clone = state.clone();
        tokio::spawn(async move {
            run_download(state_clone, candidate.id, candidate.url, candidate.dest_path).await;
        });
    }
}

async fn restart_download_internal(
    state: AppState,
    id: String,
    delete_existing: bool,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let dest_path = {
        let mut map = state.downloads.lock().await;
        let download = map.get_mut(&id).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError::new("Download não encontrado")),
            )
        })?;

        if matches!(download.status, DownloadStatus::Downloading | DownloadStatus::Verifying) {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError::new("O download já está em andamento")),
            ));
        }

        download.status = DownloadStatus::Pending;
        download.bytes_downloaded = 0;
        download.speed_bps = 0;
        download.eta_secs = 0;
        download.retry_at = None;
        download.error = None;
        download.started_at = None;
        download.completed_at = None;
        download.last_progress_at = None;
        if let Some(children) = download.children.as_mut() {
            for child in children.iter_mut() {
                child.bytes_downloaded = Some(0);
                child.speed_bps = Some(0);
                child.eta_secs = Some(0);
                child.status = Some(DownloadStatus::Pending);
            }
        }
        download.dest_path.clone()
    };

    if delete_existing {
        let path = FsPath::new(&dest_path);
        if path.exists() {
            if path.is_dir() {
                let _ = tokio::fs::remove_dir_all(path).await;
            } else {
                let _ = tokio::fs::remove_file(path).await;
            }
        }
    }

    state.broadcast(WsEvent::Status {
        id: id.clone(),
        status: DownloadStatus::Pending,
    });
    persist_download_snapshot(&state, &id).await;
    record_download_event(
        &state,
        &id,
        if delete_existing { "restarted" } else { "resumed" },
        if delete_existing { "Reiniciado do zero" } else { "Retomado/reagendado pelo usuário" },
    );

    schedule_pending_downloads(state).await;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_download_artifacts_for_download(
    download: &Download,
    preserve_root: bool,
) -> Result<(), std::io::Error> {
    let root = FsPath::new(&download.dest_path);
    if !root.exists() {
        return Ok(());
    }

    if download.is_folder {
        if let Some(children) = download.children.as_ref() {
            for child in children {
                let path = child_artifact_path(root, child);
                if !path.exists() {
                    continue;
                }
                if path.is_dir() {
                    tokio::fs::remove_dir_all(&path).await?;
                } else {
                    tokio::fs::remove_file(&path).await?;
                }
                remove_empty_parents_until(path.parent(), root).await?;
            }
            if !preserve_root && is_dir_empty(root).await? {
                tokio::fs::remove_dir(root).await?;
            }
            return Ok(());
        }
    }

    if root.is_dir() {
        if preserve_root {
            return Ok(());
        }
        tokio::fs::remove_dir_all(root).await
    } else if preserve_root {
        Ok(())
    } else {
        tokio::fs::remove_file(root).await
    }
}

fn child_artifact_path(root: &FsPath, child: &FileChildInfo) -> PathBuf {
    let relative = child.path.as_deref().filter(|path| !path.is_empty()).unwrap_or(&child.filename);
    let mut path = root.to_path_buf();
    for component in FsPath::new(relative).components() {
        if let std::path::Component::Normal(part) = component {
            path.push(part);
        }
    }
    path
}

async fn remove_empty_parents_until(
    mut current: Option<&FsPath>,
    stop: &FsPath,
) -> Result<(), std::io::Error> {
    while let Some(path) = current {
        if path == stop {
            break;
        }
        if !path.exists() || !path.is_dir() || !is_dir_empty(path).await? {
            break;
        }
        tokio::fs::remove_dir(path).await?;
        current = path.parent();
    }
    Ok(())
}

async fn is_dir_empty(path: &FsPath) -> Result<bool, std::io::Error> {
    let mut entries = tokio::fs::read_dir(path).await?;
    Ok(entries.next_entry().await?.is_none())
}

// Helper: atualiza status para Error e emite evento WebSocket
async fn update_error(state: &AppState, id: &str, message: &str) {
    {
        let mut map = state.downloads.lock().await;
        if let Some(d) = map.get_mut(id) {
            d.status = DownloadStatus::Error;
            d.error = Some(message.to_string());
            d.speed_bps = 0;
            d.eta_secs = 0;
            d.retry_at = None;
            d.completed_at = Some(current_unix_secs());
            if let Some(children) = d.children.as_mut() {
                for child in children.iter_mut() {
                    if child.status == Some(DownloadStatus::Downloading) {
                        child.speed_bps = Some(0);
                        child.eta_secs = Some(0);
                        child.status = Some(DownloadStatus::Error);
                    }
                }
            }
        }
    }
    persist_download_snapshot(state, id).await;
    record_download_event(state, id, "error", message);
    state.broadcast(WsEvent::Error {
        id: id.to_string(),
        message: message.to_string(),
    });
}

/// Carrega downloads interrompidos do SQLite e tenta retomá-los.
pub async fn recover_downloads_from_db(state: AppState) {
    let downloads = {
        let Ok(db) = state.db.lock() else { return };
        match crate::db::load_all_downloads(&db) {
            Ok(list) => list,
            Err(error) => {
                warn!(
                    target: "gdownloader_backend::downloads",
                    "falha ao carregar downloads do SQLite na inicialização: {}",
                    error
                );
                Vec::new()
            }
        }
    };

    let complete_count = downloads
        .iter()
        .filter(|d| matches!(d.status, DownloadStatus::Complete))
        .count();
    info!(
        target: "gdownloader_backend::downloads",
        "carregados {} downloads do SQLite ({} concluídos)",
        downloads.len(),
        complete_count
    );

    for mut download in downloads {
        download.speed_bps = 0;
        download.eta_secs = 0;

        if matches!(download.status, DownloadStatus::Downloading | DownloadStatus::Verifying) {
            download.status = DownloadStatus::Pending;
            download.error = None;
            download.retry_at = None;
            if let Some(children) = download.children.as_mut() {
                for child in children.iter_mut() {
                    child.speed_bps = Some(0);
                    child.eta_secs = Some(0);
                    if child.status == Some(DownloadStatus::Downloading) {
                        child.status = Some(DownloadStatus::Pending);
                    }
                }
            }
        } else if let Some(children) = download.children.as_mut() {
            for child in children.iter_mut() {
                child.speed_bps = Some(0);
                child.eta_secs = Some(0);
            }
        }

        if matches!(download.status, DownloadStatus::Pending | DownloadStatus::RateLimited)
            && download
                .retry_at
                .map(|retry_at| retry_at <= current_unix_secs())
                .unwrap_or(false)
        {
            download.status = DownloadStatus::Pending;
            download.retry_at = None;
            download.eta_secs = 0;
        } else if matches!(download.status, DownloadStatus::Pending | DownloadStatus::RateLimited) {
            if let Some(retry_at) = download.retry_at {
                download.eta_secs = retry_at.saturating_sub(current_unix_secs());
            }
        }

        let download_id = download.id.clone();
        {
            let mut map = state.downloads.lock().await;
            map.insert(download_id.clone(), download);
        }
        persist_download_snapshot(&state, &download_id).await;
    }

    info!(target: "gdownloader_backend::downloads", "downloads recuperados do SQLite");
    schedule_pending_downloads(state).await;
}

fn prettify_download_error(message: &str) -> String {
    if let Some(premium_message) = parse_premium_required_error(message) {
        return premium_message;
    }

    let lower = message.to_lowercase();

    if lower.contains("416") || lower.contains("range not satisfiable") {
        return "O servidor rejeitou a retomada do arquivo parcial. Use Reiniciar para baixar do zero.".to_string();
    }

    if lower.contains("400 bad request") {
        return "O servidor rejeitou a requisição atual. Se houver arquivo parcial, use Reiniciar para baixar do zero.".to_string();
    }

    if lower.contains("403") || lower.contains("forbidden") {
        return "O servidor bloqueou o download neste momento. Tente novamente em alguns minutos.".to_string();
    }

    if lower.contains("429") || lower.contains("too many requests") {
        return "O provedor aplicou limite temporário de requisições. Aguarde um pouco e tente novamente.".to_string();
    }

    message.to_string()
}

struct RetryPolicy {
    retry_delay_secs: u64,
    wait_message: String,
    final_message: String,
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn schedule_retry_wakeup(state: AppState, retry_at: u64) {
    let delay = retry_at.saturating_sub(current_unix_secs());
    tokio::spawn(async move {
        if delay > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
        }
        schedule_pending_downloads(state).await;
    });
}

fn schedule_next_retry_wakeup(
    state: AppState,
    downloads: &std::collections::HashMap<String, Download>,
) {
    let now = current_unix_secs();
    let next_retry_at = downloads
        .values()
        .filter(|download| matches!(download.status, DownloadStatus::Pending | DownloadStatus::RateLimited))
        .filter_map(|download| download.retry_at)
        .filter(|retry_at| *retry_at > now)
        .min();

    if let Some(next_retry_at) = next_retry_at {
        schedule_retry_wakeup(state, next_retry_at);
    }
}

fn provider_parallel_limit(provider: &str) -> Option<usize> {
    providers::capabilities_for_provider_name(provider).max_parallel_downloads_free
}

fn parse_premium_required_error(message: &str) -> Option<String> {
    let payload = message.strip_prefix("PREMIUM_REQUIRED:")?;
    let mut parts = payload.splitn(2, ':');
    let provider = parts.next()?.trim();
    let detail = parts
        .next()
        .unwrap_or("este arquivo exige conta premium")
        .trim();
    Some(format!("{provider}: {detail}"))
}

fn select_downloads_to_start(
    mut pending: Vec<QueueCandidate>,
    active_by_provider: &std::collections::HashMap<String, usize>,
    slots: usize,
) -> Vec<QueueCandidate> {
    pending.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.created_at.cmp(&right.created_at))
    });

    let mut active_by_provider = active_by_provider.clone();
    let mut selected = Vec::new();

    for candidate in pending {
        let provider = candidate.provider.clone();
        if let Some(limit_for_provider) = provider_parallel_limit(&provider) {
            let used = active_by_provider.get(&provider).copied().unwrap_or(0);
            if used >= limit_for_provider {
                continue;
            }
            active_by_provider.insert(provider, used + 1);
        }

        selected.push(candidate);
        if selected.len() >= slots {
            break;
        }
    }

    selected
}

fn active_file_units(download: &Download) -> usize {
    if !matches!(download.status, DownloadStatus::Downloading | DownloadStatus::Verifying) {
        return 0;
    }

    if download.is_folder {
        let child_active = download
            .children
            .as_ref()
            .map(|children| {
                children
                    .iter()
                    .filter(|child| child.status == Some(DownloadStatus::Downloading))
                    .count()
            })
            .unwrap_or(0);
        return child_active.max(1);
    }

    1
}

/// Retorna Some((type, sitekey, pageurl)) se o erro é um captcha.
fn parse_captcha_error(message: &str) -> Option<(String, String, String)> {
    // Format: CAPTCHA_REQUIRED:{type}:{sitekey}:{pageurl}
    if message.starts_with("CAPTCHA_REQUIRED:") {
        let parts: Vec<&str> = message.splitn(4, ':').collect();
        let captcha_type = parts.get(1).copied().unwrap_or("recaptcha2");
        let sitekey = parts.get(2).copied().unwrap_or("");
        let pageurl = parts.get(3).copied().unwrap_or("");
        return Some((captcha_type.to_string(), sitekey.to_string(), pageurl.to_string()));
    }
    None
}

fn classify_retry_policy(provider: &str, message: &str, attempt: u32) -> RetryPolicy {
    // Format: RATE_LIMIT:{secs}:{human_message}
    if message.starts_with("RATE_LIMIT:") {
        let parts: Vec<&str> = message.splitn(3, ':').collect();
        let secs = parts.get(1).and_then(|s| s.parse::<u64>().ok()).unwrap_or(3600);
        let human = parts.get(2).copied().unwrap_or(message);
        return RetryPolicy {
            retry_delay_secs: secs,
            wait_message: format!("{}. Retry automático agendado.", human),
            final_message: human.to_string(),
        };
    }

    let lower = message.to_lowercase();
    let fallback_delay = (attempt + 1) as u64;
    let pretty = prettify_download_error(message);
    let parsed_wait = providers::extract_wait_seconds_from_text(message);
    let provider_cooldown = providers::capabilities_for_provider_name(provider).free_cooldown_secs;

    if lower.contains("509")
        || lower.contains("bandwidth limit exceeded")
        || lower.contains("userstorage.mega.co.nz")
        || lower.contains("userstorage.mega.nz")
    {
        return RetryPolicy {
            retry_delay_secs: 5 * 60 * (attempt as u64 + 1),
            wait_message: "Limite temporário do Mega detectado. Vamos tentar novamente automaticamente e retomar do ponto onde parou.".to_string(),
            final_message: "O Mega aplicou um limite temporário de tráfego. Tente novamente mais tarde ou use uma conta para continuar.".to_string(),
        };
    }

    if let Some(wait_secs) = parsed_wait {
        let wait_message = if wait_secs >= 3600 {
            format!("O host informou uma espera de {}h {:02}m. Vamos tentar novamente automaticamente.", wait_secs / 3600, (wait_secs % 3600) / 60)
        } else if wait_secs >= 60 {
            format!("O host informou uma espera de {}m {:02}s. Vamos tentar novamente automaticamente.", wait_secs / 60, wait_secs % 60)
        } else {
            format!("O host informou uma espera de {}s. Vamos tentar novamente automaticamente.", wait_secs)
        };
        return RetryPolicy {
            retry_delay_secs: wait_secs,
            wait_message,
            final_message: pretty,
        };
    }

    if (lower.contains("slot gratuito")
        || lower.contains("free slot")
        || lower.contains("limite")
        || lower.contains("download simult")
        || lower.contains("outro download")
        || lower.contains("traffic")
        || lower.contains("quota"))
        && provider_cooldown.is_some()
    {
        return RetryPolicy {
            retry_delay_secs: provider_cooldown.unwrap_or(3600),
            wait_message: format!(
                "{} Vamos revalidar automaticamente quando o cooldown provável do host expirar.",
                pretty
            ),
            final_message: pretty,
        };
    }

    if lower.contains("429") || lower.contains("too many requests") {
        return RetryPolicy {
            retry_delay_secs: 60 * (attempt as u64 + 1),
            wait_message: "O provedor limitou temporariamente as requisições. Vamos tentar novamente em breve.".to_string(),
            final_message: pretty,
        };
    }

    if lower.contains("403") || lower.contains("forbidden") {
        return RetryPolicy {
            retry_delay_secs: 30 * (attempt as u64 + 1),
            wait_message: "O servidor bloqueou temporariamente este download. Vamos tentar novamente em breve.".to_string(),
            final_message: pretty,
        };
    }

    RetryPolicy {
        retry_delay_secs: fallback_delay,
        wait_message: format!(
            "Falha temporária na tentativa {}. Vamos tentar novamente automaticamente.",
            attempt + 1
        ),
        final_message: pretty,
    }
}

pub async fn toggle_pin_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let mut map = state.downloads.lock().await;
    if let Some(dl) = map.get_mut(&id) {
        dl.pinned = !dl.pinned;
        let pinned = dl.pinned;
        drop(map);
        if let Ok(db) = state.db.lock() {
            let _ = db.execute(
                "UPDATE downloads SET pinned = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![pinned as i64, current_unix_secs() as i64, id],
            );
        }
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, Json(ApiError::new("Download not found"))))
    }
}

#[derive(serde::Deserialize)]
pub struct AutoTorRequest {
    pub enabled: bool,
}

/// Liga/desliga o "usar Tor ao atingir o limite" para um download específico.
/// A conexão do Tor em si é orquestrada pelo processo principal/UI; aqui só
/// persistimos a intenção para que o ciclo de retry passe a rotacionar o
/// circuito até concluir.
pub async fn set_auto_tor(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AutoTorRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    {
        let mut map = state.downloads.lock().await;
        let Some(dl) = map.get_mut(&id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiError::new("Download não encontrado"))));
        };
        dl.auto_tor_on_limit = req.enabled;
    }
    persist_download_snapshot(&state, &id).await;
    record_download_event(
        &state,
        &id,
        "network",
        if req.enabled {
            "Tor ao atingir limite: ativado para este download"
        } else {
            "Tor ao atingir limite: desativado para este download"
        },
    );
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, provider: &str, created_at: u64, priority: i32) -> QueueCandidate {
        QueueCandidate {
            id: id.to_string(),
            url: format!("https://example.com/{id}"),
            dest_path: format!("/tmp/{id}"),
            provider: provider.to_string(),
            created_at,
            priority,
        }
    }

    #[test]
    fn isolated_route_requires_flag_and_running_tor() {
        // flag on + tor running => assign
        assert!(should_assign_isolated_route(true, Some(9150)));
        // flag on but tor not running => no
        assert!(!should_assign_isolated_route(true, None));
        // flag off => no, even with tor running
        assert!(!should_assign_isolated_route(false, Some(9150)));
        // flag off + tor off => no
        assert!(!should_assign_isolated_route(false, None));
    }

    #[test]
    fn tor_limit_retry_cap_is_reached_after_max() {
        assert!(!tor_limit_retry_exhausted(0));
        assert!(!tor_limit_retry_exhausted(TOR_LIMIT_MAX_RETRIES - 1));
        assert!(tor_limit_retry_exhausted(TOR_LIMIT_MAX_RETRIES));
        assert!(tor_limit_retry_exhausted(TOR_LIMIT_MAX_RETRIES + 1));
    }

    #[test]
    fn scheduler_respects_priority_before_created_at() {
        let selected = select_downloads_to_start(
            vec![
                candidate("low", "Mega", 10, 0),
                candidate("high", "Mega", 20, 5),
                candidate("mid", "Mega", 5, 2),
            ],
            &std::collections::HashMap::new(),
            2,
        );

        let ids = selected.into_iter().map(|item| item.id).collect::<Vec<_>>();
        assert_eq!(ids, vec!["high".to_string(), "mid".to_string()]);
    }

    #[test]
    fn scheduler_respects_provider_parallel_limit() {
        let active = std::collections::HashMap::from([(String::from("BRFiles"), 1usize)]);
        let selected = select_downloads_to_start(
            vec![
                candidate("a", "BRFiles", 10, 0),
                candidate("b", "BRFiles", 11, 0),
                candidate("c", "Mega", 12, 0),
            ],
            &active,
            3,
        );

        let ids = selected.into_iter().map(|item| item.id).collect::<Vec<_>>();
        assert_eq!(ids, vec!["c".to_string()]);
    }

    #[test]
    fn scheduler_fills_remaining_slots_with_other_providers() {
        let active = std::collections::HashMap::from([(String::from("BRFiles"), 1usize)]);
        let selected = select_downloads_to_start(
            vec![
                candidate("a", "BRFiles", 10, 0),
                candidate("b", "Mega", 11, 0),
                candidate("c", "Mega", 12, 0),
            ],
            &active,
            2,
        );

        let ids = selected.into_iter().map(|item| item.id).collect::<Vec<_>>();
        assert_eq!(ids, vec!["b".to_string(), "c".to_string()]);
    }

    #[test]
    fn scheduler_respects_global_slot_limit() {
        let selected = select_downloads_to_start(
            vec![
                candidate("a", "Mega", 10, 5),
                candidate("b", "Mega", 11, 4),
                candidate("c", "Mega", 12, 3),
            ],
            &std::collections::HashMap::new(),
            1,
        );

        let ids = selected.into_iter().map(|item| item.id).collect::<Vec<_>>();
        assert_eq!(ids, vec!["a".to_string()]);
    }

    #[test]
    fn retry_policy_uses_provider_cooldown_for_free_slot_errors() {
        let policy = classify_retry_policy(
            "1Fichier",
            "1Fichier está sem slot gratuito disponível no momento. O host exige aguardar ou entrar com conta.",
            0,
        );

        assert_eq!(policy.retry_delay_secs, 300);
    }

    #[test]
    fn retry_policy_prefers_host_reported_wait_time() {
        let policy = classify_retry_policy(
            "BRFiles",
            "Seu IP já possui outro download ativo. Aguarde 2 horas 15 minutos e 9 segundos.",
            0,
        );

        assert_eq!(policy.retry_delay_secs, 8109);
    }
}
