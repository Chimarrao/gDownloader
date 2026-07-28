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

#[derive(Debug, Deserialize)]
pub struct MoveRequest {
    /// Nova pasta de destino do arquivo (o nome do arquivo é preservado).
    pub dest_dir: String,
}

fn normalize_identity_url(url: &str) -> String {
    url.split('#').next().unwrap_or(url).trim().to_string()
}

/// Aplica um nome escolhido pelo usuário preservando a extensão original quando ele
/// não digitou uma. Remove separadores de caminho e caracteres inválidos.
fn apply_custom_filename(original: &str, custom: &str) -> String {
    let cleaned: String = custom
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '\0' | '<' | '>' | '|' | '"' | '?' | '*'))
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return original.to_string();
    }
    let original_ext = std::path::Path::new(original)
        .extension()
        .and_then(|ext| ext.to_str());
    let custom_has_ext = std::path::Path::new(cleaned).extension().is_some();
    match (original_ext, custom_has_ext) {
        // Usuário digitou "Meu Filme" e o original era .mkv → "Meu Filme.mkv".
        (Some(ext), false) => format!("{cleaned}.{ext}"),
        _ => cleaned.to_string(),
    }
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
        duration_secs: cached.duration_secs,
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
                    info.duration_secs,
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

    // Renomear antes de baixar (A3): o usuário pode ter escolhido outro nome.
    if let Some(custom) = req.filename.as_deref() {
        if !file_info.is_folder {
            let renamed = apply_custom_filename(&file_info.filename, custom);
            if !renamed.is_empty() {
                file_info.filename = renamed;
            }
        }
    }

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
        duration_secs: file_info.duration_secs,
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
        error_kind: None,
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
    let mut list: Vec<Download> = map
        .values()
        .cloned()
        .map(|mut download| {
            if download.error_kind.is_none() {
                download.error_kind =
                    classify_error_kind(&download.status, download.error.as_deref());
            }
            download
        })
        .collect();
    list.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
    Json(list)
}

#[derive(Debug, Deserialize)]
pub struct KnownUrlsRequest {
    pub urls: Vec<String>,
}

/// Normaliza a URL para comparar dedup do capturador: tira espaços e o fragmento
/// (`#...`) — que alguns providers usam só para embutir hash, não muda o arquivo.
fn normalize_capture_url(url: &str) -> String {
    url.trim().split('#').next().unwrap_or("").trim().to_string()
}

/// Dado um conjunto de URLs capturadas, informa quais já estão NA FILA (qualquer
/// status) ou no HISTÓRICO de concluídos, para o capturador marcar "já baixado" e
/// impedir re-adição. Compara por URL normalizada (sem fragmento).
pub async fn check_known_urls(
    State(state): State<AppState>,
    Json(req): Json<KnownUrlsRequest>,
) -> Json<serde_json::Value> {
    // Mapa: url normalizada -> (location, status, filename)
    let mut result: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();

    // 1) Fila em memória (qualquer status) — tem prioridade sobre o histórico.
    {
        let map = state.downloads.lock().await;
        for download in map.values() {
            let norm = normalize_capture_url(&download.url);
            if norm.is_empty() {
                continue;
            }
            result.entry(norm).or_insert_with(|| {
                serde_json::json!({
                    "location": "queue",
                    "status": format!("{:?}", download.status),
                    "filename": download.filename,
                })
            });
        }
    }

    // 2) Histórico de concluídos — só para as URLs pedidas que ainda não casaram.
    let pending: Vec<String> = req
        .urls
        .iter()
        .map(|u| normalize_capture_url(u))
        .filter(|u| !u.is_empty() && !result.contains_key(u))
        .collect();
    if !pending.is_empty() {
        if let Ok(db) = state.db.lock() {
            if let Ok(found) = crate::db::find_history_titles_by_urls(&db, &pending) {
                for (url, title) in found {
                    result.insert(
                        normalize_capture_url(&url),
                        serde_json::json!({
                            "location": "history",
                            "status": "Complete",
                            "filename": title,
                        }),
                    );
                }
            }
        }
    }

    // Responde só para as URLs pedidas, preservando a URL original como chave.
    let mut known = serde_json::Map::new();
    for original in &req.urls {
        let norm = normalize_capture_url(original);
        if let Some(entry) = result.get(&norm) {
            known.insert(original.clone(), entry.clone());
        }
    }
    Json(serde_json::json!({ "known": known }))
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
    let removed_dest = {
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

        let dest_path = download.dest_path.clone();
        map.remove(&id).map(|_| dest_path)
    };

    if let Some(dest_path) = removed_dest {
        state.speed_limits.lock().await.remove(&id);
        // Remove da lista preserva o arquivo final, mas limpa temporários e o
        // estado de resume — senão sobram `.part`/`.parts`/`.merging` e linhas órfãs.
        cleanup_temp_artifacts(&dest_path).await;
        if let Ok(db) = state.db.lock() {
            let _ = crate::db::insert_download_event(&db, &id, "removed", "Removido da lista");
            let _ = crate::db::clear_direct_http_parts_for_dest(&db, &dest_path);
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
    // Também limpa os temporários (partes/merge) que o passo acima não cobre.
    cleanup_temp_artifacts(&download.dest_path).await;

    {
        let mut map = state.downloads.lock().await;
        map.remove(&id);
    }
    state.speed_limits.lock().await.remove(&id);

    if let Ok(db) = state.db.lock() {
        let _ = crate::db::insert_download_event(&db, &id, "removed_files", "Removido com arquivos físicos");
        let _ = crate::db::clear_direct_http_parts_for_dest(&db, &download.dest_path);
        let _ = crate::db::delete(&db, &id);
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn clear_finished_downloads(
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    // Decide o que fica e coleta os que saem (para limpar temporários + resume).
    let removed: Vec<(String, String)> = {
        let mut map = state.downloads.lock().await;
        let keep = |download: &Download| {
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
        };
        let removed: Vec<(String, String)> = map
            .values()
            .filter(|d| !keep(d))
            .map(|d| (d.id.clone(), d.dest_path.clone()))
            .collect();
        map.retain(|_, download| keep(download));
        removed
    };
    {
        // Descarta handles de limite vivo de downloads que saíram da lista.
        let map = state.downloads.lock().await;
        state
            .speed_limits
            .lock()
            .await
            .retain(|id, _| map.contains_key(id));
    }
    // Limpa temporários (partes/merge) dos que saíram — os concluídos não têm, mas
    // os interrompidos sem progresso resumível podem ter deixado lixo.
    for (_, dest_path) in &removed {
        cleanup_temp_artifacts(dest_path).await;
    }
    if let Ok(db) = state.db.lock() {
        for (_, dest_path) in &removed {
            let _ = crate::db::clear_direct_http_parts_for_dest(&db, dest_path);
        }
        let _ = crate::db::delete_finished(&db);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Pausa um único download (aborta a task, mata o sidecar, marca Paused). Fonte de
/// verdade compartilhada entre o pause individual e o "pausar todos". NÃO reescalona
/// a fila (quem chama decide). Retorna `false` se o id não existir.
async fn pause_one(state: &AppState, id: &str, reason: &str) -> bool {
    let sidecar = {
        let map = state.downloads.lock().await;
        map.get(id).map(|download| {
            (
                download.provider.clone(),
                download.url.clone(),
                download.dest_path.clone(),
            )
        })
    };
    let Some((provider, url, dest_path)) = sidecar else {
        return false;
    };

    cancel_provider_sidecar_download(&provider, &url, &dest_path).await;
    if let Some(handle) = state.active_tasks.lock().await.remove(id) {
        handle.abort();
    }
    {
        let mut map = state.downloads.lock().await;
        if let Some(download) = map.get_mut(id) {
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
        } else {
            return false;
        }
    }
    persist_download_snapshot(state, id).await;
    record_download_event(state, id, "paused", reason);
    state.broadcast(WsEvent::Status {
        id: id.to_string(),
        status: DownloadStatus::Paused,
    });
    true
}

pub async fn pause_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !pause_one(&state, &id, "Pausado pelo usuário").await {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("Download não encontrado")),
        ));
    }
    schedule_pending_downloads(state).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Pausa TODOS os downloads DE UMA VEZ (atômico), não "aos poucos":
/// 1) liga a flag global (o scheduler para de iniciar qualquer coisa);
/// 2) marca todos os ativos como Paused num único lock;
/// 3) aborta todas as tasks de uma vez;
/// 4) só então cancela sidecars e persiste/broadcast.
/// Preserva o progresso parcial de cada download.
pub async fn pause_all_downloads(State(state): State<AppState>) -> Json<serde_json::Value> {
    use std::sync::atomic::Ordering;
    // 1) Flag global — bloqueia o scheduler imediatamente.
    state.paused_all.store(true, Ordering::SeqCst);

    // Coleta ids ativos + dados do sidecar (num lock só).
    let (ids, sidecars): (Vec<String>, Vec<(String, String, String)>) = {
        let map = state.downloads.lock().await;
        let mut ids = Vec::new();
        let mut sidecars = Vec::new();
        for d in map.values() {
            if matches!(
                d.status,
                DownloadStatus::Downloading
                    | DownloadStatus::Pending
                    | DownloadStatus::Verifying
                    | DownloadStatus::RateLimited
                    | DownloadStatus::WaitingCaptcha
            ) {
                ids.push(d.id.clone());
                sidecars.push((d.provider.clone(), d.url.clone(), d.dest_path.clone()));
            }
        }
        (ids, sidecars)
    };

    // 2) Marca TODOS como Paused de uma vez (um único lock → a UI vê tudo pausar junto).
    {
        let mut map = state.downloads.lock().await;
        for id in &ids {
            if let Some(d) = map.get_mut(id) {
                d.status = DownloadStatus::Paused;
                d.speed_bps = 0;
                d.eta_secs = 0;
                d.retry_at = None;
                d.error = None;
                if let Some(children) = d.children.as_mut() {
                    for child in children.iter_mut() {
                        child.speed_bps = Some(0);
                        child.eta_secs = Some(0);
                        if child.status == Some(DownloadStatus::Downloading) {
                            child.status = Some(DownloadStatus::Paused);
                        }
                    }
                }
            }
        }
    }

    // 3) Aborta TODAS as tasks de uma vez (instantâneo).
    {
        let mut tasks = state.active_tasks.lock().await;
        for id in &ids {
            if let Some(handle) = tasks.remove(id) {
                handle.abort();
            }
        }
    }

    // 4) Cancela sidecars (helpers de navegador) — best effort — e persiste/broadcast.
    for (provider, url, dest_path) in &sidecars {
        cancel_provider_sidecar_download(provider, url, dest_path).await;
    }
    for id in &ids {
        persist_download_snapshot(&state, id).await;
        state.broadcast(WsEvent::Status {
            id: id.clone(),
            status: DownloadStatus::Paused,
        });
    }
    info!(target: "gdownloader_backend::downloads", "PAUSAR TODOS (atômico): {} downloads pausados", ids.len());
    Json(serde_json::json!({ "paused": ids.len() }))
}

/// Retoma TODOS os downloads pausados: desliga a flag global, marca os pausados como
/// Pending e deixa o scheduler iniciá-los respeitando o limite (não dispara todos).
pub async fn resume_all_downloads(State(state): State<AppState>) -> Json<serde_json::Value> {
    use std::sync::atomic::Ordering;
    // Desliga a pausa global ANTES de reescalonar.
    state.paused_all.store(false, Ordering::SeqCst);

    let ids: Vec<String> = {
        let mut map = state.downloads.lock().await;
        let mut ids = Vec::new();
        for d in map.values_mut() {
            if d.status == DownloadStatus::Paused {
                d.status = DownloadStatus::Pending;
                d.error = None;
                d.retry_at = None;
                ids.push(d.id.clone());
            }
        }
        ids
    };
    for id in &ids {
        persist_download_snapshot(&state, id).await;
        record_download_event(&state, id, "resumed", "Retomado (retomar todos)");
    }
    info!(target: "gdownloader_backend::downloads", "RETOMAR TODOS: {} downloads na fila", ids.len());
    // Um único schedule inicia até o limite; o resto fica Pending e entra depois.
    schedule_pending_downloads(state.clone()).await;
    Json(serde_json::json!({ "resumed": ids.len() }))
}

pub async fn resume_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    // Retomar preserva o progresso parcial (não zera nem apaga o arquivo).
    restart_download_internal(state, id, false, false).await
}

pub async fn retry_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    // Retentar preserva o progresso parcial (retoma) e não apaga o arquivo.
    restart_download_internal(state, id, false, false).await
}

pub async fn restart_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    // Reiniciar zera o progresso e apaga o arquivo parcial.
    restart_download_internal(state, id, true, true).await
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

    // Aplica ao vivo: rebalance considera o novo limite individual como um teto
    // sobre a cota da banda total compartilhada (o menor dos dois vale).
    rebalance_speed_limits(&state).await;

    persist_download_snapshot(&state, &id).await;
    record_download_event(&state, &id, "speed_limit", &format!("Limite individual alterado para {} KiB/s", req.speed_limit_kib));
    Ok(StatusCode::NO_CONTENT)
}

/// Move o arquivo (parcial ou concluído) de um download para outra pasta, como no
/// jDownloader (item 6). Funciona durante o download: pausa a task, move os arquivos,
/// atualiza o dest_path e retoma do ponto onde parou (via Range onde suportado).
pub async fn move_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<MoveRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let dest_dir = req.dest_dir.trim().to_string();
    if dest_dir.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(ApiError::new("Pasta de destino inválida"))));
    }

    let (provider, url, old_dest, is_folder, status, filename) = {
        let map = state.downloads.lock().await;
        let Some(d) = map.get(&id) else {
            return Err((StatusCode::NOT_FOUND, Json(ApiError::new("Download não encontrado"))));
        };
        (d.provider.clone(), d.url.clone(), d.dest_path.clone(), d.is_folder, d.status.clone(), d.filename.clone())
    };

    if is_folder {
        return Err((StatusCode::BAD_REQUEST, Json(ApiError::new("Mover downloads de pasta ainda não é suportado"))));
    }

    let file_name = std::path::Path::new(&old_dest)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or(filename);
    let new_dest = std::path::Path::new(&dest_dir).join(&file_name);
    let new_dest_str = new_dest.to_string_lossy().to_string();

    if new_dest_str == old_dest {
        return Ok(StatusCode::NO_CONTENT);
    }

    // Se estiver baixando, aborta a task para liberar o arquivo antes de mover.
    let was_active = matches!(status, DownloadStatus::Downloading | DownloadStatus::Verifying);
    if was_active {
        cancel_provider_sidecar_download(&provider, &url, &old_dest).await;
        if let Some(handle) = state.active_tasks.lock().await.remove(&id) {
            handle.abort();
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }

    if let Err(error) = tokio::fs::create_dir_all(&dest_dir).await {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(format!("Falha ao criar a pasta de destino: {error}")))));
    }
    if let Err(error) = move_download_files(&old_dest, &new_dest_str).await {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(format!("Falha ao mover o arquivo: {error}")))));
    }

    // Remapeia o resume do Direct HTTP e atualiza o caminho no modelo.
    if let Ok(db) = state.db.lock() {
        let _ = crate::db::remap_direct_http_dest(&db, &old_dest, &new_dest_str);
    }
    {
        let mut map = state.downloads.lock().await;
        if let Some(d) = map.get_mut(&id) {
            d.dest_path = new_dest_str.clone();
            // Atualiza o path do filho único (single-file com children).
            if let Some(children) = d.children.as_mut() {
                if children.len() == 1 {
                    children[0].path = Some(new_dest_str.clone());
                }
            }
            if was_active && matches!(d.status, DownloadStatus::Downloading | DownloadStatus::Verifying) {
                d.status = DownloadStatus::Pending;
                d.speed_bps = 0;
                d.eta_secs = 0;
            }
        }
    }

    persist_download_snapshot(&state, &id).await;
    record_download_event(&state, &id, "moved", &format!("Arquivo movido para {dest_dir}"));
    if was_active {
        schedule_pending_downloads(state.clone()).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Move o arquivo principal e seus arquivos-parte irmãos (`base.partN`) para o novo
/// caminho, usando rename e caindo para copiar+apagar entre volumes diferentes.
async fn move_download_files(old_dest: &str, new_dest: &str) -> std::io::Result<()> {
    let old = std::path::Path::new(old_dest);
    let new = std::path::Path::new(new_dest);
    let old_parent = old.parent().unwrap_or_else(|| std::path::Path::new("."));
    let new_parent = new.parent().unwrap_or_else(|| std::path::Path::new("."));
    let base = match old.file_name().and_then(|name| name.to_str()) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => return Ok(()),
    };
    let new_base = match new.file_name().and_then(|name| name.to_str()) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => return Ok(()),
    };

    let mut entries = match tokio::fs::read_dir(old_parent).await {
        Ok(entries) => entries,
        Err(_) => return Ok(()), // pasta antiga sumiu; nada a mover
    };
    while let Some(entry) = entries.next_entry().await? {
        // Só arquivos (o dir `.parts` do download paralelo é recriado ao retomar).
        if entry.file_type().await.map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy().to_string();
        let is_main = name == base;
        let is_part = name.starts_with(&base) && name[base.len()..].starts_with(".part");
        if !is_main && !is_part {
            continue;
        }
        let suffix = &name[base.len()..];
        let target = new_parent.join(format!("{new_base}{suffix}"));
        move_fs_file(&entry.path(), &target).await?;
    }
    Ok(())
}

async fn move_fs_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    if tokio::fs::rename(from, to).await.is_ok() {
        return Ok(());
    }
    // Volumes diferentes (EXDEV): copia e remove o original.
    tokio::fs::copy(from, to).await?;
    let _ = tokio::fs::remove_file(from).await;
    Ok(())
}

// --- Executa o download em background ---
// Wrapper com guarda anti-dupla-execução: se já existe uma execução viva para este
// `id`, retorna imediatamente (evita duas tasks concorrentes escrevendo o mesmo
// arquivo e "piscando" o progresso). Remove o marcador ao terminar, em qualquer saída.
async fn run_download(state: AppState, id: String, url: String, dest_path: String) {
    {
        let mut running = state.running_downloads.lock().await;
        if !running.insert(id.clone()) {
            warn!(
                target: "gdownloader_backend::downloads",
                "ignorando execução duplicada de download id={}", id
            );
            return;
        }
    }
    run_download_inner(state.clone(), id.clone(), url, dest_path).await;
    state.running_downloads.lock().await.remove(&id);
    // Saiu um ativo: redistribui a banda total entre os que continuam baixando.
    rebalance_speed_limits(&state).await;
}

// Esta função roda em uma task separada do tokio
// É como um Worker ou uma Promise longa rodando em paralelo
async fn run_download_inner(state: AppState, id: String, url: String, dest_path: String) {
    let (max_retries, speed_limit_kib, parallel_parts, selected_children) = {
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
        let speed_limit_kib = map.get(&id).map(|d| d.speed_limit_kib).unwrap_or(0);
        let parallel_parts = map
            .get(&id)
            .map(|d| d.parallel_parts.max(1))
            .unwrap_or(1);
        let selected_children = map
            .get(&id)
            .and_then(|d| d.selected_children.clone());
        (max_retries, speed_limit_kib, parallel_parts, selected_children)
    };

    // Handle de limite de velocidade vivo: a UI altera este valor atômico via PATCH
    // e o loop de streaming do provider lê a cada iteração (efeito em tempo real).
    let speed_limit = providers::speed_limit_from_kib(speed_limit_kib);
    state
        .speed_limits
        .lock()
        .await
        .insert(id.clone(), speed_limit.clone());
    // Entrou um novo ativo: redistribui a banda total compartilhada entre todos.
    rebalance_speed_limits(&state).await;

    persist_download_snapshot(&state, &id).await;

    // Stall watchdog: if a running download produces no progress for this long,
    // the underlying process is considered hung. It is aborted and retried
    // automatically, independently of the normal error-retry budget.
    const STALL_TIMEOUT_SECS: u64 = 90;
    const STALL_MAX_RETRIES: u32 = 3;

    // Quedas de conexão (troca de Wi-Fi, cabo, roteador reiniciando) NÃO são culpa do
    // arquivo: têm um orçamento próprio e generoso de tentativas, com backoff curto,
    // e não consomem o `max_retries` do usuário. Assim o download não "fica travado"
    // ao trocar de rede — ele espera a rede voltar e retoma do ponto onde parou.
    const NETWORK_MAX_RETRIES: u32 = 60;

    let mut attempt = 0u32;
    let mut stall_retries = 0u32;
    let mut network_retries = 0u32;
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
        // YouTube usa o yt-dlp (1 fluxo). Mega: força conexão única para garantir a
        // descriptografia AES-CTR sequencial e 100% correta — no download em partes,
        // o seek do cifrador por parte (em offsets não alinhados a 16 bytes) é uma
        // fonte de arquivo corrompido; conexão única elimina esse risco.
        let effective_parallel_parts = if provider_name == "YouTube" || provider_name == "Mega" {
            1
        } else {
            parallel_parts as usize
        };
        info!(
            target: "gdownloader_backend::downloads",
            "iniciando tentativa id={} provider={} attempt={}/{} parts={} limite_kib={} dest={}",
            id,
            provider_name,
            attempt,
            max_retries,
            effective_parallel_parts,
            speed_limit.load(std::sync::atomic::Ordering::Relaxed) / 1024,
            dest_path
        );

        // Informa o usuário (no log do download) o que está acontecendo no Mega:
        // o arquivo chega criptografado e é descriptografado (AES-CTR) durante o
        // próprio download, em conexão única.
        if provider_name == "Mega" && attempt == 0 {
            record_download_event(
                &state,
                &id,
                "info",
                "Mega: baixando e descriptografando (AES-CTR) em conexão única para garantir a integridade do arquivo.",
            );
        }

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

                // Aviso, não bloqueio: o usuário pediu para informar quando o arquivo
                // pode não caber, mas deixar baixar mesmo assim (item 11). Se o disco
                // realmente encher, a escrita falha naturalmente e o erro aparece.
                if file_size > 0 && file_size > available {
                    let warn_msg = format!(
                        "Pode não caber no disco: necessário ~{}MB, livre ~{}MB. O download vai continuar mesmo assim.",
                        file_size / 1_048_576,
                        available / 1_048_576
                    );
                    warn!(
                        target: "gdownloader_backend::downloads",
                        "espaço em disco baixo id={} necessario={}MB livre={}MB (baixando mesmo assim)",
                        id,
                        file_size / 1_048_576,
                        available / 1_048_576
                    );
                    record_download_event(&state, &id, "disk_warning", &warn_msg);
                }
            }
        }

        let url_clone = url.clone();
        let dest_clone = dest_path.clone();
        let speed_limit_clone = speed_limit.clone();
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
                    speed_limit_clone,
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
        // Vira true quando os bytes chegam a 100% mas a task ainda não voltou (fase de
        // junção de partes / finalização). Evita o download ficar "Baixando" a 100%
        // e mostra "Finalizando" enquanto o merge acontece. YouTube tem fases próprias
        // (vídeo/áudio/merge do yt-dlp) e é tratado à parte, então fica fora disso.
        let mut finalizing = false;
        // Estado do progresso da JUNÇÃO de partes (evento child_path "merge"): usado
        // para calcular velocidade e tempo restante da junção, que pode demorar.
        let mut merge_last_bytes = 0u64;
        let mut merge_last_time = std::time::Instant::now();
        let mut merge_speed = 0u64;
        let is_youtube = provider_name == "YouTube";

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

            // Progresso da JUNÇÃO de partes: mostra velocidade + tempo restante da
            // junção (status Verifying/"Juntando partes"), sem mexer no resto.
            if update.child_path.as_deref() == Some("merge") {
                let merged = update.child_bytes_downloaded.unwrap_or(0);
                let total = update.child_total_bytes.unwrap_or(0).max(merged);
                let elapsed = merge_last_time.elapsed().as_secs_f64();
                if elapsed >= 0.4 {
                    let delta = merged.saturating_sub(merge_last_bytes);
                    merge_speed = (delta as f64 / elapsed) as u64;
                    merge_last_bytes = merged;
                    merge_last_time = std::time::Instant::now();
                }
                let merge_eta = if merge_speed > 0 && total > merged {
                    (total - merged) / merge_speed
                } else {
                    0
                };
                {
                    let mut map = state.downloads.lock().await;
                    if let Some(d) = map.get_mut(&id) {
                        d.status = DownloadStatus::Verifying;
                        d.speed_bps = merge_speed;
                        d.eta_secs = merge_eta;
                        d.last_progress_at = Some(current_unix_secs());
                    }
                }
                state.broadcast(WsEvent::Progress {
                    id: id.clone(),
                    bytes: total,
                    total,
                    speed: merge_speed,
                    eta: merge_eta,
                    status: DownloadStatus::Verifying,
                    child_path: Some("merge".to_string()),
                    child_filename: None,
                    child_bytes: Some(merged),
                    child_total: Some(total),
                    child_speed: Some(merge_speed),
                    child_eta: Some(merge_eta),
                });
                continue;
            }

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

            // Chegou a 100% dos bytes mas a task ainda não voltou → está finalizando
            // (juntando partes / gravando o arquivo final). A partir daqui não mostra
            // mais velocidade/ETA/partes e o status vira "Verificando" (Finalizando).
            let at_100 = !is_youtube && update.total_bytes > 0 && reported_bytes >= update.total_bytes;
            let just_started_finalizing = at_100 && !finalizing;
            if at_100 {
                finalizing = true;
            }
            let display_status = if finalizing {
                DownloadStatus::Verifying
            } else {
                DownloadStatus::Downloading
            };
            let display_speed = if finalizing { 0 } else { speed };
            let display_eta = if finalizing { 0 } else { eta };

            {
                let mut map = state.downloads.lock().await;
                if let Some(d) = map.get_mut(&id) {
                    d.bytes_downloaded = d.bytes_downloaded.max(reported_bytes);
                    d.speed_bps = display_speed;
                    d.eta_secs = display_eta;
                    if finalizing {
                        d.status = DownloadStatus::Verifying;
                    }
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

            if should_persist_snapshot || just_started_finalizing {
                persist_download_snapshot(&state, &id).await;
                last_db_write = std::time::Instant::now();
            }

            // Ao entrar na finalização, avisa o usuário de forma explícita e muda o
            // status (barra deixa de ser "Baixando 100%" e vira "Juntando partes…").
            if just_started_finalizing {
                record_download_event(&state, &id, "merging", "Juntando as partes e gravando o arquivo final…");
                state.broadcast(WsEvent::StatusChanged {
                    id: id.clone(),
                    status: DownloadStatus::Verifying,
                    error: None,
                    retry_at: None,
                    captcha_type: None,
                    captcha_sitekey: None,
                    captcha_page_url: None,
                });
            }

            if last_progress_broadcast.elapsed() >= std::time::Duration::from_millis(500)
                || update.total_bytes > 0 && reported_bytes >= update.total_bytes
            {
                last_progress_broadcast = std::time::Instant::now();
                state.broadcast(WsEvent::Progress {
                    id: id.clone(),
                    bytes: reported_bytes,
                    total: update.total_bytes,
                    speed: display_speed,
                    eta: display_eta,
                    status: display_status,
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

                // Verificação leve de integridade (tamanho, HTML de erro, assinatura do
                // formato). Só para arquivo único — pega o caso "página de erro salva
                // como .mkv" e formatos claramente corrompidos.
                let is_folder = {
                    let map = state.downloads.lock().await;
                    map.get(&id).map(|d| d.is_folder).unwrap_or(false)
                };
                if !is_folder {
                    let integrity = crate::integrity::check_file(&dest_path, bytes).await;
                    if let Some(reason) = integrity.reason() {
                        warn!(
                            target: "gdownloader_backend::downloads",
                            "integridade suspeita id={} provider={} bytes={} motivo={}",
                            id, provider_name, bytes, reason
                        );
                        {
                            let mut map = state.downloads.lock().await;
                            if let Some(d) = map.get_mut(&id) {
                                d.status = DownloadStatus::Corrupted;
                                d.bytes_downloaded = bytes;
                                d.speed_bps = 0;
                                d.eta_secs = 0;
                                d.error = Some(format!("Arquivo possivelmente corrompido: {reason}"));
                                d.error_kind = Some("integrity".to_string());
                                d.completed_at = Some(current_unix_secs());
                            }
                        }
                        state.active_tasks.lock().await.remove(&id);
                        persist_download_snapshot(&state, &id).await;
                        record_download_event(&state, &id, "corrupted", &format!("Integridade suspeita: {reason}"));
                        state.broadcast(WsEvent::StatusChanged {
                            id: id.clone(),
                            status: DownloadStatus::Corrupted,
                            error: Some(format!("Arquivo possivelmente corrompido: {reason}")),
                            retry_at: None,
                            captcha_type: None,
                            captcha_sitekey: None,
                            captcha_page_url: None,
                        });
                        reschedule_pending_downloads(state.clone());
                        return;
                    }
                    info!(
                        target: "gdownloader_backend::downloads",
                        "integridade ok id={} bytes={}", id, bytes
                    );
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
                            d.error_kind = Some("captcha".to_string());
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

                // Queda de conexão (troca de rede, roteador reiniciando, cabo solto):
                // retenta com orçamento próprio e backoff curto, SEM consumir o
                // `max_retries` do usuário e SEM marcar erro definitivo. O arquivo
                // parcial é preservado e a próxima tentativa retoma via Range quando o
                // provider suporta resume. Isso conserta o "foi a zero e travou".
                if is_connection_error(&err_str)
                    && !err_str.starts_with("RATE_LIMIT:")
                    && !err_str.starts_with("PREMIUM_REQUIRED:")
                {
                    let status = {
                        let map = state.downloads.lock().await;
                        map.get(&id).map(|d| d.status.clone())
                    };
                    if matches!(status, Some(DownloadStatus::Paused | DownloadStatus::Cancelled)) {
                        reschedule_pending_downloads(state.clone());
                        return;
                    }
                    if network_retries < NETWORK_MAX_RETRIES {
                        network_retries = network_retries.saturating_add(1);
                        // Backoff curto e limitado (2s→15s): a rede pode voltar a qualquer momento.
                        let delay = (network_retries as u64).saturating_add(1).min(15);
                        let retry_at = current_unix_secs().saturating_add(delay);
                        {
                            let mut map = state.downloads.lock().await;
                            if let Some(d) = map.get_mut(&id) {
                                d.status = DownloadStatus::Pending;
                                d.speed_bps = 0;
                                d.eta_secs = delay;
                                d.retry_at = Some(retry_at);
                                d.error = Some(
                                    "Conexão perdida — aguardando a rede voltar para retomar do ponto onde parou…".to_string(),
                                );
                                d.error_kind = Some("network".to_string());
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
                            "queda de conexão id={} provider={} tentativa_rede={} bytes={} delay={}s err={}",
                            id, provider_name, network_retries, max_bytes_seen, delay, err_str
                        );
                        record_download_event(
                            &state,
                            &id,
                            "network_retry",
                            &format!("Queda de conexão (tentativa {network_retries}) em {max_bytes_seen} bytes — retomando em {delay}s"),
                        );
                        state.broadcast(WsEvent::StatusChanged {
                            id: id.clone(),
                            status: DownloadStatus::Pending,
                            error: Some("Conexão perdida — retomando quando a rede voltar…".to_string()),
                            retry_at: Some(retry_at),
                            captcha_type: None,
                            captcha_sitekey: None,
                            captcha_page_url: None,
                        });
                        reschedule_pending_downloads(state.clone());
                        for _ in 0..delay {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            let status = {
                                let map = state.downloads.lock().await;
                                map.get(&id).map(|download| download.status.clone())
                            };
                            if matches!(status, None | Some(DownloadStatus::Paused | DownloadStatus::Cancelled)) {
                                reschedule_pending_downloads(state.clone());
                                return;
                            }
                        }
                        // Não incrementa `attempt`: queda de rede não gasta o orçamento do arquivo.
                        continue;
                    }
                    // Esgotou o orçamento de rede: segue para o tratamento normal abaixo.
                }

                let retry_policy = classify_retry_policy(&provider_name, &err_str, attempt);
                let is_rate_limit = err_str.starts_with("RATE_LIMIT:");
                let is_premium_required = err_str.starts_with("PREMIUM_REQUIRED:");
                // Erros permanentes (arquivo removido, premium obrigatório, link morto)
                // NÃO entram no loop de retry — falham na hora com mensagem clara.
                let is_permanent = is_permanent_error(&err_str);
                // Flag por-download: "usar Tor ao atingir o limite". Só entra em
                // cena DEPOIS de esgotar as tentativas normais (attempt >= max_retries):
                // primeiro tenta o máximo pela rede normal, só então troca pro Tor.
                let (download_auto_tor, max_retries) = {
                    let map = state.downloads.lock().await;
                    match map.get(&id) {
                        // Relê `max_retries` do registro a cada decisão para que mudar
                        // o limite de tentativas nas configurações valha AO VIVO,
                        // inclusive para downloads que já estavam rodando.
                        Some(d) => (d.auto_tor_on_limit, d.max_retries),
                        None => (false, max_retries),
                    }
                };
                let isolated_tor_port = *state.isolated_tor_port.lock().await;
                let normal_retries_exhausted = attempt >= max_retries;
                let isolated_tor_retry = should_assign_isolated_route(download_auto_tor, isolated_tor_port)
                    && normal_retries_exhausted
                    && !is_permanent;
                // Retenta enquanto: for rate-limit, ainda houver orçamento normal, ou
                // o download estiver marcado p/ Tor (segue tentando até o Tor entrar).
                // Nunca retenta erros permanentes (removed/premium/unsupported).
                let should_retry = !is_permanent
                    && !is_premium_required
                    && (is_rate_limit || attempt < max_retries || download_auto_tor);
                if should_retry {
                    let settings = state.db.lock().ok()
                        .and_then(|db| crate::db::load_public_settings(&db).ok())
                        .unwrap_or_default();
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
                    let wait_kind = classify_error_kind(&wait_status, Some(&retry_policy.wait_message));
                    {
                        let mut map = state.downloads.lock().await;
                        if let Some(d) = map.get_mut(&id) {
                            d.status = wait_status.clone();
                            d.speed_bps = 0;
                            d.eta_secs = retry_delay_secs;
                            d.retry_at = Some(retry_at);
                            d.error = Some(retry_policy.wait_message.clone());
                            d.error_kind = wait_kind;
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
                        "download reagendado id={} provider={} attempt={}/{} delay={}s status={} reason={}",
                        id,
                        provider_name,
                        attempt,
                        max_retries,
                        retry_delay_secs,
                        if is_rate_limit { "rate_limited" } else { "pending" },
                        retry_policy.wait_message
                    );
                    record_download_event(
                        &state,
                        &id,
                        if is_rate_limit { "rate_limited" } else { "retry" },
                        &format!(
                            "Reagendado (tentativa {attempt}/{max_retries}) em {delay}s: {}",
                            retry_policy.wait_message,
                            delay = retry_delay_secs
                        ),
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
                                    // Reconnect succeeded — skip the wait and retry immediately.
                                    // Downloads marcados p/ Tor contam o rate-limit para
                                    // avançar rumo à troca pro Tor após as tentativas normais.
                                    if !is_rate_limit || download_auto_tor {
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
                    // Conta a tentativa normal (não durante a fase Tor). Rate-limits
                    // só contam quando o Tor está ligado p/ o download, para esgotar
                    // as tentativas normais antes de trocar pro Tor.
                    if !isolated_tor_retry && (!is_rate_limit || download_auto_tor) {
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
                            d.error_kind = Some("temporary".to_string());
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

    let country_code = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        async {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(6))
                .build()
                .ok()?;
            let json = client
                .get(format!("https://ipapi.co/{ip}/json/"))
                .send()
                .await
                .ok()?
                .json::<serde_json::Value>()
                .await
                .ok()?;
            json.get("country_code")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        },
    )
    .await
    .ok()
    .flatten();

    {
        let mut map = state.downloads.lock().await;
        if let Some(download) = map.get_mut(&id) {
            if let Some(current) = download.network_route.as_mut() {
                if current.proxy_username == route.proxy_username {
                    current.exit_ip = Some(ip.clone());
                    if let Some(cc) = country_code.clone() {
                        current.exit_country_code = Some(cc);
                    }
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
    // Entra na fila de processamento pesada (hash lê o arquivo inteiro). Se não há
    // vaga agora, avisa o usuário ("Na fila de processamento…") e aguarda — assim
    // várias verificações não saturam CPU/disco ao mesmo tempo.
    let _permit = match providers::finalization_semaphore().try_acquire() {
        Ok(permit) => permit,
        Err(_) => {
            record_download_event(state, id, "queued_processing", "Na fila de processamento…");
            state.broadcast(WsEvent::StatusChanged {
                id: id.to_string(),
                status: DownloadStatus::Verifying,
                error: None,
                retry_at: None,
                captcha_type: None,
                captcha_sitekey: None,
                captcha_page_url: None,
            });
            providers::finalization_semaphore()
                .acquire()
                .await
                .map_err(|e| anyhow::anyhow!("semáforo de finalização fechado: {e}"))?
        }
    };
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

/// Dados de um download em execução, usados para decidir quem devolver à fila
/// quando o limite de concorrência é reduzido ao vivo.
struct ActiveSlot {
    id: String,
    priority: i32,
    /// Momento em que começou (ou de criação, como fallback) — para desempate.
    since: u64,
    provider: String,
    url: String,
    dest_path: String,
}

/// Aplica o limite de concorrência AO VIVO quando o usuário o REDUZ: se há mais
/// downloads em execução do que o novo limite permite, devolve os excedentes para a
/// fila (status Pending). Escolhe **menor prioridade primeiro e, em empate, os mais
/// recentes** — preservando os mais antigos/quase prontos. Os devolvidos voltam
/// sozinhos quando abrir vaga (o scheduler os reinicia respeitando o limite).
pub async fn enforce_active_limit(state: &AppState) {
    let limit = *state.max_concurrent_downloads.lock().await;

    let mut active: Vec<ActiveSlot> = {
        let map = state.downloads.lock().await;
        map.values()
            .filter(|d| d.status == DownloadStatus::Downloading)
            .map(|d| ActiveSlot {
                id: d.id.clone(),
                priority: d.priority,
                since: d.started_at.unwrap_or(d.created_at),
                provider: d.provider.clone(),
                url: d.url.clone(),
                dest_path: d.dest_path.clone(),
            })
            .collect()
    };
    if active.len() <= limit {
        return;
    }
    // Ordena por quem MANTER primeiro: maior prioridade, depois mais antigo.
    // Os últimos da lista (menor prioridade / mais recentes) são os devolvidos.
    active.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.since.cmp(&b.since)));
    let excess = active.split_off(limit);

    // Devolve todos para a fila num único lock (a UI vê a mudança de uma vez).
    {
        let mut map = state.downloads.lock().await;
        for slot in &excess {
            if let Some(d) = map.get_mut(&slot.id) {
                if d.status == DownloadStatus::Downloading {
                    d.status = DownloadStatus::Pending;
                    d.speed_bps = 0;
                    d.eta_secs = 0;
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
        }
    }
    // Aborta as tasks de uma vez.
    {
        let mut tasks = state.active_tasks.lock().await;
        for slot in &excess {
            if let Some(handle) = tasks.remove(&slot.id) {
                handle.abort();
            }
        }
    }
    for slot in &excess {
        cancel_provider_sidecar_download(&slot.provider, &slot.url, &slot.dest_path).await;
        persist_download_snapshot(state, &slot.id).await;
        record_download_event(state, &slot.id, "queued", "Devolvido à fila (limite reduzido)");
        state.broadcast(WsEvent::Status {
            id: slot.id.clone(),
            status: DownloadStatus::Pending,
        });
    }
    info!(
        target: "gdownloader_backend::downloads",
        "limite ao vivo: {} download(s) devolvido(s) à fila (novo limite={})",
        excess.len(),
        limit
    );
}

/// Redistribui o limite TOTAL de banda entre os downloads ativos (banda
/// COMPARTILHADA): cada ativo recebe `total / nº de ativos`, respeitando um limite
/// individual menor se o usuário tiver definido um. Com total = 0 (ilimitado), vale
/// apenas o limite individual de cada um. Deve ser chamada sempre que o conjunto de
/// ativos muda (início/fim) ou a configuração muda.
pub async fn rebalance_speed_limits(state: &AppState) {
    use std::sync::atomic::Ordering;
    let global = state.global_speed_limit_bps.load(Ordering::Relaxed);

    let active: Vec<(String, u64)> = {
        let map = state.downloads.lock().await;
        map.values()
            .filter(|d| d.status == DownloadStatus::Downloading)
            .map(|d| (d.id.clone(), d.speed_limit_kib.saturating_mul(1024)))
            .collect()
    };
    let count = (active.len() as u64).max(1);
    // Cota por download; nunca menor que 64KB/s para não travar segundos a fio.
    let share = if global == 0 { 0 } else { (global / count).max(65_536) };

    let limits = state.speed_limits.lock().await;
    for (id, individual) in &active {
        let effective = match (global, *individual) {
            (0, 0) => 0,               // ambos ilimitados
            (0, ind) => ind,           // só o limite individual
            (_, 0) => share,           // só a cota global compartilhada
            (_, ind) => share.min(ind), // o menor entre a cota global e o individual
        };
        if let Some(atomic) = limits.get(id) {
            atomic.store(effective, Ordering::Relaxed);
        }
    }
}

pub async fn schedule_pending_downloads(state: AppState) {
    // Pausa global ligada → não inicia nada.
    if state.paused_all.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let limit = *state.max_concurrent_downloads.lock().await;
    let active_task_ids = {
        let tasks = state.active_tasks.lock().await;
        let running = state.running_downloads.lock().await;
        tasks
            .keys()
            .chain(running.iter())
            .cloned()
            .collect::<std::collections::HashSet<_>>()
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
    reset_progress: bool,
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
        download.speed_bps = 0;
        download.eta_secs = 0;
        download.retry_at = None;
        download.error = None;
        // Retomar/retentar renova o orçamento de tentativas do usuário.
        download.retry_count = 0;
        download.completed_at = None;
        if reset_progress {
            // "Reiniciar do zero": descarta o progresso parcial.
            download.bytes_downloaded = 0;
            download.started_at = None;
            download.last_progress_at = None;
            if let Some(children) = download.children.as_mut() {
                for child in children.iter_mut() {
                    child.bytes_downloaded = Some(0);
                    child.speed_bps = Some(0);
                    child.eta_secs = Some(0);
                    child.status = Some(DownloadStatus::Pending);
                }
            }
        } else {
            // "Retentar": PRESERVA os bytes já baixados (retoma do ponto onde parou).
            if let Some(children) = download.children.as_mut() {
                for child in children.iter_mut() {
                    child.speed_bps = Some(0);
                    child.eta_secs = Some(0);
                    if child.status == Some(DownloadStatus::Downloading) {
                        child.status = Some(DownloadStatus::Pending);
                    }
                }
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

/// Remove os arquivos TEMPORÁRIOS de um download (merge em andamento, diretório de
/// partes do download paralelo e arquivos-parte irmãos `{base}.partN`), preservando
/// sempre o arquivo final. Usado ao excluir/limpar downloads. Best-effort: erros
/// (arquivo já ausente) são ignorados.
async fn cleanup_temp_artifacts(dest_path: &str) {
    let _ = tokio::fs::remove_file(format!("{dest_path}.merging")).await;
    let _ = tokio::fs::remove_dir_all(format!("{dest_path}.parts")).await;

    let path = FsPath::new(dest_path);
    if let (Some(parent), Some(base)) = (
        path.parent(),
        path.file_name().and_then(|name| name.to_str()),
    ) {
        if let Ok(mut entries) = tokio::fs::read_dir(parent).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let file_name = entry.file_name();
                if let Some(name) = file_name.to_str() {
                    // Só irmãos `{base}.partN` — nunca o arquivo final `{base}`.
                    if name.len() > base.len()
                        && name.starts_with(base)
                        && name[base.len()..].starts_with(".part")
                    {
                        let _ = tokio::fs::remove_file(entry.path()).await;
                    }
                }
            }
        }
    }
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
    let pretty = prettify_download_error(message);
    let kind = classify_error_kind(&DownloadStatus::Error, Some(&pretty));
    {
        let mut map = state.downloads.lock().await;
        if let Some(d) = map.get_mut(id) {
            d.status = DownloadStatus::Error;
            d.error = Some(pretty.clone());
            d.error_kind = kind.clone();
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
    record_download_event(state, id, kind.as_deref().unwrap_or("error"), &pretty);
    state.broadcast(WsEvent::Error {
        id: id.to_string(),
        message: pretty,
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
        // Downloads que terminaram em falha não vão retomar: seus temporários
        // (.part/.parts/.merging) são órfãos e podem ser limpos com segurança após
        // um crash, reclamando espaço em disco. Os resumíveis (Pending/RateLimited/
        // WaitingCaptcha) preservam os temporários para continuar de onde pararam.
        let cleanup_temps = matches!(
            download.status,
            DownloadStatus::Error | DownloadStatus::Corrupted | DownloadStatus::Cancelled
        );
        let dest_path = download.dest_path.clone();
        {
            let mut map = state.downloads.lock().await;
            // Recalcula error_kind a partir do status/mensagem persistidos.
            let mut download = download;
            if download.error_kind.is_none() {
                download.error_kind =
                    classify_error_kind(&download.status, download.error.as_deref());
            }
            map.insert(download_id.clone(), download);
        }
        if cleanup_temps {
            cleanup_temp_artifacts(&dest_path).await;
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
    if let Some(removed) = parse_prefixed_error(message, "REMOVED:") {
        return removed;
    }
    if let Some(unsupported) = parse_prefixed_error(message, "UNSUPPORTED:") {
        return unsupported;
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

/// Extrai mensagem legível de prefixos `KIND:provider:detail` ou `KIND:detail`.
fn parse_prefixed_error(message: &str, prefix: &str) -> Option<String> {
    let payload = message.strip_prefix(prefix)?;
    let mut parts = payload.splitn(2, ':');
    let first = parts.next()?.trim();
    match parts.next() {
        Some(detail) if !detail.trim().is_empty() => Some(detail.trim().to_string()),
        _ => Some(first.to_string()),
    }
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
    // Fairness entre hosts: prioridade formal primeiro; em empate, hosts com
    // MENOS downloads ativos saem na frente (evita um host monopolizar a fila);
    // por último, ordem de chegada (FIFO).
    pending.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| {
                let left_used = active_by_provider.get(&left.provider).copied().unwrap_or(0);
                let right_used = active_by_provider.get(&right.provider).copied().unwrap_or(0);
                left_used.cmp(&right_used)
            })
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
        } else {
            // Mesmo sem hard-cap, conta o ativo para fairness nas próximas escolhas.
            *active_by_provider.entry(provider).or_insert(0) += 1;
        }

        selected.push(candidate);
        if selected.len() >= slots {
            break;
        }
    }

    selected
}

/// Classifica o erro/estado para a UI e políticas de retry.
/// Valores estáveis (snake_case) consumidos pelo frontend.
fn classify_error_kind(status: &DownloadStatus, message: Option<&str>) -> Option<String> {
    match status {
        DownloadStatus::RateLimited => return Some("rate_limit".to_string()),
        DownloadStatus::WaitingCaptcha => return Some("captcha".to_string()),
        DownloadStatus::DiskFull => return Some("disk_full".to_string()),
        DownloadStatus::Corrupted => return Some("integrity".to_string()),
        _ => {}
    }

    let message = message?;
    if message.starts_with("RATE_LIMIT:") {
        return Some("rate_limit".to_string());
    }
    if message.starts_with("CAPTCHA_REQUIRED:") {
        return Some("captcha".to_string());
    }
    if message.starts_with("PREMIUM_REQUIRED:") {
        return Some("premium".to_string());
    }
    if message.starts_with("REMOVED:") {
        return Some("removed".to_string());
    }
    if message.starts_with("UNSUPPORTED:") {
        return Some("permanent".to_string());
    }
    if is_connection_error(message) {
        return Some("network".to_string());
    }

    let lower = message.to_lowercase();
    if lower.contains("corromp")
        || lower.contains("integridade")
        || lower.contains("hash")
        || lower.contains("assinatura")
    {
        return Some("integrity".to_string());
    }
    if lower.contains("não localizado")
        || lower.contains("nao localizado")
        || lower.contains("not found")
        || lower.contains("file was deleted")
        || lower.contains("file has been removed")
        || lower.contains("arquivo removido")
    {
        return Some("removed".to_string());
    }
    if lower.contains("premium") {
        return Some("premium".to_string());
    }
    if lower.contains("disco") && (lower.contains("cheio") || lower.contains("espaço") || lower.contains("espaco")) {
        return Some("disk_full".to_string());
    }
    if is_permanent_error(message) {
        return Some("permanent".to_string());
    }
    if matches!(status, DownloadStatus::Error | DownloadStatus::Pending) {
        return Some("temporary".to_string());
    }
    None
}

/// Erros que NÃO devem consumir o loop de retry (falha definitiva na hora).
fn is_permanent_error(message: &str) -> bool {
    if message.starts_with("PREMIUM_REQUIRED:")
        || message.starts_with("REMOVED:")
        || message.starts_with("UNSUPPORTED:")
    {
        return true;
    }
    let lower = message.to_lowercase();
    lower.contains("arquivo não localizado")
        || lower.contains("arquivo nao localizado")
        || lower.contains("file not found")
        || lower.contains("file was deleted")
        || lower.contains("file has been removed")
        || lower.contains("arquivo removido")
        || lower.contains("link não suportado")
        || lower.contains("link nao suportado")
}

/// Quantos slots de concorrência um download ocupa. Cada download ATIVO ocupa
/// exatamente UM slot — inclusive pastas, que baixam seus arquivos sequencialmente
/// dentro de uma única task. Antes as pastas contavam como vários "file units", o
/// que furava a conta do limite (o limite deixava de valer ao adicionar pastas).
fn active_file_units(download: &Download) -> usize {
    if matches!(download.status, DownloadStatus::Downloading | DownloadStatus::Verifying) {
        1
    } else {
        0
    }
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

/// Detecta erros de transporte/rede (queda de conexão) — distintos de erros do host
/// (403/429/rate-limit) ou do arquivo (404). Usado para retentar indefinidamente com
/// orçamento próprio ao trocar de rede, sem gastar o `max_retries` do usuário.
fn is_connection_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    const NEEDLES: &[&str] = &[
        "error sending request",
        "connection reset",
        "connection closed",
        "connection aborted",
        "connection refused",
        "connection error",
        "connection closed before message completed",
        "timed out",
        "timeout",
        "operation timed out",
        "dns error",
        "failed to lookup address",
        "name or service not known",
        "temporary failure in name resolution",
        "network is unreachable",
        "no route to host",
        "host unreachable",
        "broken pipe",
        "tcp connect error",
        "request or response body error",
        "body error",
        "incomplete message",
        "unexpected end of file",
        "early eof",
        "connection closed before",
        "download http incompleto",
        "parte http incompleta",
        "servidor http ignorou range",
        // Códigos de erro de socket comuns (macOS/Linux)
        "os error 51", // ENETUNREACH
        "os error 54", // ECONNRESET (macOS)
        "os error 60", // ETIMEDOUT (macOS)
        "os error 64", // EHOSTDOWN
        "os error 65", // EHOSTUNREACH (macOS)
        "os error 104", // ECONNRESET (Linux)
        "os error 110", // ETIMEDOUT (Linux)
        "os error 111", // ECONNREFUSED (Linux)
        "os error 113", // EHOSTUNREACH (Linux)
    ];
    NEEDLES.iter().any(|needle| lower.contains(needle))
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

    #[tokio::test]
    async fn cleanup_temp_artifacts_removes_temps_but_keeps_final_file() {
        let dir = std::env::temp_dir().join(format!("gdl_cleanup_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let dest = dir.join("video.mp4");
        let dest_str = dest.to_str().unwrap().to_string();

        // Arquivo final + temporários que devem sumir.
        tokio::fs::write(&dest, b"final").await.unwrap();
        tokio::fs::write(dir.join("video.mp4.part0"), b"a").await.unwrap();
        tokio::fs::write(dir.join("video.mp4.part1"), b"b").await.unwrap();
        tokio::fs::write(dir.join("video.mp4.merging"), b"m").await.unwrap();
        tokio::fs::create_dir_all(dir.join("video.mp4.parts")).await.unwrap();
        tokio::fs::write(dir.join("video.mp4.parts/part-000"), b"p").await.unwrap();
        // Arquivo vizinho não relacionado — NÃO pode ser tocado.
        tokio::fs::write(dir.join("outro.mp4"), b"x").await.unwrap();

        cleanup_temp_artifacts(&dest_str).await;

        assert!(dest.exists(), "arquivo final deve ser preservado");
        assert!(dir.join("outro.mp4").exists(), "arquivo vizinho deve ser preservado");
        assert!(!dir.join("video.mp4.part0").exists());
        assert!(!dir.join("video.mp4.part1").exists());
        assert!(!dir.join("video.mp4.merging").exists());
        assert!(!dir.join("video.mp4.parts").exists());

        let _ = tokio::fs::remove_dir_all(&dir).await;
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

    #[test]
    fn permanent_errors_are_classified() {
        assert!(is_permanent_error("REMOVED:Rapidgator:Arquivo não localizado no Rapidgator"));
        assert!(is_permanent_error("PREMIUM_REQUIRED:Rapidgator:precisa premium"));
        assert!(is_permanent_error("UNSUPPORTED:Foo:Link não suportado"));
        assert!(!is_permanent_error("RATE_LIMIT:60:aguarde"));
        assert!(!is_permanent_error("error sending request"));

        assert_eq!(
            classify_error_kind(&DownloadStatus::Error, Some("REMOVED:X:gone")).as_deref(),
            Some("removed")
        );
        assert_eq!(
            classify_error_kind(&DownloadStatus::RateLimited, Some("wait")).as_deref(),
            Some("rate_limit")
        );
        assert_eq!(
            classify_error_kind(&DownloadStatus::Corrupted, Some("bad")).as_deref(),
            Some("integrity")
        );
        assert_eq!(
            classify_error_kind(
                &DownloadStatus::Pending,
                Some("error sending request: connection reset")
            )
            .as_deref(),
            Some("network")
        );
    }

    #[test]
    fn scheduler_prefers_less_busy_hosts_on_priority_tie() {
        let mut active = std::collections::HashMap::new();
        active.insert("Mega".to_string(), 2);
        active.insert("MediaFire".to_string(), 0);

        let selected = select_downloads_to_start(
            vec![
                candidate("mega-item", "Mega", 10, 1),
                candidate("mf-item", "MediaFire", 10, 2),
            ],
            &active,
            1,
        );

        let ids = selected.into_iter().map(|item| item.id).collect::<Vec<_>>();
        assert_eq!(ids, vec!["mf-item".to_string()]);
    }

    #[test]
    fn scheduler_respects_host_parallel_cap() {
        let mut active = std::collections::HashMap::new();
        active.insert("1Fichier".to_string(), 1); // cap free = 1

        let selected = select_downloads_to_start(
            vec![
                candidate("a", "1Fichier", 10, 1),
                candidate("b", "MediaFire", 5, 2),
            ],
            &active,
            2,
        );

        let ids = selected.into_iter().map(|item| item.id).collect::<Vec<_>>();
        assert_eq!(ids, vec!["b".to_string()]);
    }

    #[test]
    fn custom_filename_preserves_extension_and_sanitizes() {
        // Sem extensão digitada → mantém a original.
        assert_eq!(apply_custom_filename("filme.mkv", "Meu Filme"), "Meu Filme.mkv");
        // Com extensão digitada → usa a do usuário.
        assert_eq!(apply_custom_filename("filme.mkv", "Meu Filme.mp4"), "Meu Filme.mp4");
        // Remove separadores/caracteres inválidos.
        assert_eq!(apply_custom_filename("a.zip", "pas/ta\\x?:y"), "pastaxy.zip");
        // Vazio após limpeza → mantém o original.
        assert_eq!(apply_custom_filename("orig.bin", "   "), "orig.bin");
        // Original sem extensão.
        assert_eq!(apply_custom_filename("semext", "novo"), "novo");
    }

    #[test]
    fn detects_connection_errors_for_dedicated_retry_budget() {
        // Quedas de rede reais (mensagens de reqwest/hyper/io) devem ser retentadas.
        assert!(is_connection_error(
            "error reading a body from connection: Connection reset by peer (os error 54)"
        ));
        assert!(is_connection_error("error sending request for url (https://x/y)"));
        assert!(is_connection_error("operation timed out"));
        assert!(is_connection_error("dns error: failed to lookup address information"));
        assert!(is_connection_error("connection closed before message completed"));
        assert!(is_connection_error("Download HTTP incompleto: 1024/4096 bytes"));
        assert!(is_connection_error("network is unreachable (os error 51)"));
    }

    #[test]
    fn does_not_flag_host_or_file_errors_as_connection_drops() {
        // Erros do host/arquivo NÃO devem entrar no orçamento de rede (têm seu próprio fluxo).
        assert!(!is_connection_error("403 Forbidden"));
        assert!(!is_connection_error("429 Too Many Requests"));
        assert!(!is_connection_error("RATE_LIMIT:3600:aguarde"));
        assert!(!is_connection_error("Arquivo não encontrado (404)"));
        assert!(!is_connection_error("CAPTCHA_REQUIRED:recaptcha2:sitekey:url"));
    }
}
