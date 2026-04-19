use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::env;
use std::path::Path as FsPath;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::{
    models::{AddDownloadRequest, ApiError, Download, DownloadStatus, FileChildInfo, WsEvent},
    providers,
    ws::AppState,
};

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

// Adiciona um novo download à fila e inicia o processo em background
// POST /downloads — body: { "url": "...", "dest_dir": "..." }
pub async fn add_download(
    State(state): State<AppState>,
    Json(req): Json<AddDownloadRequest>,
) -> Result<Json<Download>, (StatusCode, Json<ApiError>)> {
    // Detecta qual provider trata essa URL
    let provider = providers::detect_provider(&req.url).ok_or_else(|| {
        let error_msg = if req.url.contains("mega.nz/folder/") {
            "❌ Links de PASTA do Mega (/folder/) não são suportados.\n\
             Use um link de ARQUIVO (/file/) em vez disso.\n\
             Para obter: abra a pasta no Mega > clique em um arquivo > compartilhe aquele arquivo"
        } else if req.url.contains("mega.nz") {
            "❌ URL do Mega inválida. Formatos suportados:\n\
             • Novo: https://mega.nz/file/HANDLE#KEY\n\
             • Antigo: https://mega.nz/#!HANDLE!KEY"
        } else if req.url.contains("mediafire.com") {
            "⚠️ URL do MediaFire não foi reconhecida.\n\
             Verifique se o link é válido e acessível.\n\
             O link pode estar expirado ou protegido."
        } else {
            "URL não reconhecida. Provedores suportados:\n\
             • Mega (mega.nz) — arquivos diretos /file/ (não pastas /folder/)\n\
             • MediaFire (mediafire.com)\n\
             • Google Drive (drive.google.com)\n\
             • PixelDrain (pixeldrain.com)"
        };

        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(error_msg)),
        )
    })?;

    // Busca informações do arquivo (nome, tamanho) antes de criar o item na fila
    let mut file_info = provider.get_file_info(&req.url).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(format!("Falha ao obter informações do arquivo: {e}"))),
        )
    })?;

    let selected_children = req
        .selected_children
        .clone()
        .filter(|children| !children.is_empty());

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
    }

    let dest_dir = expand_home(&req.dest_dir);
    tokio::fs::create_dir_all(&dest_dir).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(format!("Falha ao preparar a pasta de destino: {e}"))),
        )
    })?;

    // Monta o caminho completo do arquivo de destino
    let dest_path = format!(
        "{}/{}",
        dest_dir.trim_end_matches('/'),
        file_info.filename
    );

    {
        let map = state.downloads.lock().await;
        if let Some(existing) = map.values().find(|download| {
            download.dest_path == dest_path
                && download.size == file_info.size
                && download.is_folder == file_info.is_folder
                && download.selected_children == selected_children
        }) {
            return Ok(Json(existing.clone()));
        }
    }

    // Gera um ID único para este download (como crypto.randomUUID() no JS)
    let id = Uuid::new_v4().to_string();

    // Timestamp Unix atual em segundos
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let download = Download {
        id: id.clone(),
        url: req.url.clone(),
        provider: provider.name().to_string(),
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
        retry_at: None,
        captcha_type: None,
        captcha_sitekey: None,
        captcha_page_url: None,
        captcha_token: None,
        error: None,
        created_at: now,
    };

    {
        let mut map = state.downloads.lock().await;
        map.insert(id.clone(), download.clone());
    }

    // Persiste no SQLite
    if let Ok(db) = state.db.lock() {
        let _ = crate::db::upsert(&db, &download);
    }

    schedule_pending_downloads(state.clone()).await;

    Ok(Json(download))
}

// Lista todos os downloads (ativos, completos, com erro)
// GET /downloads
pub async fn list_downloads(State(state): State<AppState>) -> Json<Vec<Download>> {
    let map = state.downloads.lock().await;
    // Coleta os valores do HashMap em um Vec e ordena por data de criação (mais recentes primeiro)
    let mut list: Vec<Download> = map.values().cloned().collect();
    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Json(list)
}

// Cancela e remove um download da fila
// DELETE /downloads/:id
pub async fn cancel_download(
    State(state): State<AppState>,
    Path(id): Path<String>, // Path extrai o :id da URL
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
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

    if let Ok(db) = state.db.lock() {
        let _ = crate::db::update_status(&db, &id, "cancelled", Some("Cancelado pelo usuário"));
    }

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

        if matches!(download.status, DownloadStatus::Pending | DownloadStatus::Downloading | DownloadStatus::Paused) {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError::new("Só é possível remover downloads encerrados da lista")),
            ));
        }

        map.remove(&id).is_some()
    };

    if removed {
        if let Ok(db) = state.db.lock() {
            let _ = crate::db::delete(&db, &id);
        }
        return Ok(StatusCode::NO_CONTENT);
    }

    Err((
        StatusCode::NOT_FOUND,
        Json(ApiError::new("Download não encontrado")),
    ))
}

pub async fn clear_finished_downloads(
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    {
        let mut map = state.downloads.lock().await;
        map.retain(|_, download| {
            matches!(
                download.status,
                DownloadStatus::Pending | DownloadStatus::Downloading | DownloadStatus::Paused
            )
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

    if let Ok(db) = state.db.lock() {
        let _ = crate::db::update_status(&db, &id, "paused", None);
    }
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

    for attempt in 0..=max_retries {
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
            d.retry_count = attempt;
            d.speed_bps = 0;
            d.eta_secs = 0;
            d.retry_at = None;
            d.error = None;
        }

        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(64);

        let provider = match providers::detect_provider(&url) {
            Some(p) => p,
            None => {
                update_error(&state, &id, "Provider não encontrado para a URL").await;
                reschedule_pending_downloads(state.clone());
                return;
            }
        };

        let url_clone = url.clone();
        let dest_clone = dest_path.clone();
        let selected_children_clone = selected_children.clone();
        let download_task = tokio::spawn(async move {
            provider
                .download(
                    &url_clone,
                    &dest_clone,
                    speed_limit_bps,
                    parallel_parts as usize,
                    selected_children_clone,
                    progress_tx,
                )
                .await
        });
        state
            .active_tasks
            .lock()
            .await
            .insert(id.clone(), download_task.abort_handle());

        let mut last_bytes = 0u64;
        let mut last_time = std::time::Instant::now();
        let mut current_speed = 0u64;
        let mut last_db_write = std::time::Instant::now();

        while let Some(update) = progress_rx.recv().await {
            // Persiste progresso no SQLite a cada 5 segundos
            if last_db_write.elapsed().as_secs() >= 5 {
                if let Ok(db) = state.db.lock() {
                    let _ = crate::db::update_progress(&db, &id, update.bytes_downloaded);
                }
                last_db_write = std::time::Instant::now();
            }
            let elapsed = last_time.elapsed().as_secs_f64();
            if elapsed >= 0.35 {
                let delta = update.bytes_downloaded.saturating_sub(last_bytes);
                if delta > 0 {
                    current_speed = (delta as f64 / elapsed) as u64;
                }
                last_bytes = update.bytes_downloaded;
                last_time = std::time::Instant::now();
            }

            let speed = if update.bytes_downloaded == 0 { 0 } else { current_speed };

            let eta = if speed > 0 && update.total_bytes > 0 {
                update.total_bytes.saturating_sub(update.bytes_downloaded) / speed
            } else {
                0
            };

            {
                let mut map = state.downloads.lock().await;
                if let Some(d) = map.get_mut(&id) {
                    d.bytes_downloaded = update.bytes_downloaded;
                    d.speed_bps = speed;
                    d.eta_secs = eta;
                    // Update total size if it wasn't set yet (can happen with some providers)
                    if update.total_bytes > 0 && d.size == 0 {
                        d.size = update.total_bytes;
                    }
                    if let Some(children) = d.children.as_mut() {
                        if let Some(child_filename) = update.child_filename.as_deref() {
                            let child_path = update.child_path.as_deref();
                            for child in children.iter_mut() {
                                let matches = child_path
                                    .map(|path| child.path.as_deref() == Some(path))
                                    .unwrap_or_else(|| child.filename == child_filename);

                                if matches {
                                    child.bytes_downloaded = update.child_bytes_downloaded;
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

            state.broadcast(WsEvent::Progress {
                id: id.clone(),
                bytes: update.bytes_downloaded,
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

        match download_task.await {
            Ok(Ok(_bytes)) => {
                {
                    let mut map = state.downloads.lock().await;
                    if let Some(d) = map.get_mut(&id) {
                        d.status = DownloadStatus::Complete;
                        d.bytes_downloaded = d.size;
                        d.speed_bps = 0;
                        d.eta_secs = 0;
                        d.retry_at = None;
                        d.error = None;
                        if let Some(children) = d.children.as_mut() {
                            for child in children.iter_mut() {
                                child.bytes_downloaded = Some(child.size);
                                child.speed_bps = Some(0);
                                child.eta_secs = Some(0);
                                child.status = Some(DownloadStatus::Complete);
                            }
                        }
                    }
                }
                state.active_tasks.lock().await.remove(&id);
                if let Ok(db) = state.db.lock() {
                    let _ = crate::db::update_status(&db, &id, "complete", None);
                }
                state.broadcast(WsEvent::Complete {
                    id: id.clone(),
                    path: dest_path,
                });
                reschedule_pending_downloads(state.clone());
                return;
            }
            Ok(Err(e)) => {
                state.active_tasks.lock().await.remove(&id);
                let retry_policy = classify_retry_policy(&e.to_string(), attempt);
                if attempt < max_retries {
                    let retry_delay_secs = retry_policy.retry_delay_secs;
                    let retry_at = current_unix_secs().saturating_add(retry_delay_secs);
                    {
                        let mut map = state.downloads.lock().await;
                        if let Some(d) = map.get_mut(&id) {
                            d.status = DownloadStatus::Pending;
                            d.speed_bps = 0;
                            d.eta_secs = retry_delay_secs;
                            d.retry_at = Some(retry_at);
                            d.error = Some(retry_policy.wait_message.clone());
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
                    state.broadcast(WsEvent::Status {
                        id: id.clone(),
                        status: DownloadStatus::Pending,
                    });
                    reschedule_pending_downloads(state.clone());

                    for _ in 0..retry_delay_secs {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        let status = {
                            let map = state.downloads.lock().await;
                            map.get(&id).map(|download| download.status.clone())
                        };
                        if matches!(status, Some(DownloadStatus::Paused | DownloadStatus::Cancelled) | None) {
                            reschedule_pending_downloads(state.clone());
                            return;
                        }
                    }
                    continue;
                }
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
                if attempt < max_retries {
                    tokio::time::sleep(std::time::Duration::from_secs((attempt + 1) as u64)).await;
                    continue;
                }
                update_error(&state, &id, "Download interrompido").await;
                reschedule_pending_downloads(state.clone());
                return;
            }
        }
    }
}

fn reschedule_pending_downloads(state: AppState) {
    tokio::spawn(async move {
        schedule_pending_downloads(state).await;
    });
}

pub async fn schedule_pending_downloads(state: AppState) {
    let limit = *state.max_concurrent_downloads.lock().await;
    let active_count = state.active_tasks.lock().await.len();
    if active_count >= limit {
        return;
    }

    let slots = limit.saturating_sub(active_count);
    if slots == 0 {
        return;
    }

    let to_start = {
        let mut map = state.downloads.lock().await;
        let now = current_unix_secs();
        let mut pending = map
            .values()
            .filter(|download| {
                matches!(download.status, DownloadStatus::Pending)
                    && download.retry_at.map(|retry_at| retry_at <= now).unwrap_or(true)
            })
            .map(|download| {
                (
                    download.id.clone(),
                    download.url.clone(),
                    download.dest_path.clone(),
                    download.created_at,
                )
            })
            .collect::<Vec<_>>();

        pending.sort_by(|a, b| a.3.cmp(&b.3));
        let selected = pending.into_iter().take(slots).collect::<Vec<_>>();

        for (download_id, _, _, _) in &selected {
            if let Some(download) = map.get_mut(download_id) {
                download.status = DownloadStatus::Downloading;
                download.error = None;
                download.speed_bps = 0;
                download.eta_secs = 0;
                download.retry_at = None;
            }
        }

        selected
    };

    for (download_id, url, dest_path, _) in to_start {
        let state_clone = state.clone();
        tokio::spawn(async move {
            run_download(state_clone, download_id, url, dest_path).await;
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

        if matches!(download.status, DownloadStatus::Downloading) {
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

    schedule_pending_downloads(state).await;

    Ok(StatusCode::NO_CONTENT)
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
    if let Ok(db) = state.db.lock() {
        let _ = crate::db::update_status(&db, id, "error", Some(message));
    }
    state.broadcast(WsEvent::Error {
        id: id.to_string(),
        message: message.to_string(),
    });
}

/// Carrega downloads interrompidos do SQLite e tenta retomá-los.
pub async fn recover_downloads_from_db(state: AppState) {
    let rows = {
        let Ok(db) = state.db.lock() else { return };
        crate::db::load_resumable(&db).unwrap_or_default()
    };

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0")
        .build()
        .unwrap_or_default();

    for row in rows {
        // Verifica se o link ainda está acessível
        let reachable = match client.head(&row.url).send().await {
            Ok(resp) => {
                let code = resp.status().as_u16();
                code < 400 || code == 206
            }
            Err(_) => false,
        };

        let (status, error) = if reachable {
            (DownloadStatus::Paused, None)
        } else {
            (DownloadStatus::Error, Some("Link expirado ou indisponível. Verifique e tente novamente.".to_string()))
        };

        let download = Download {
            id: row.id.clone(),
            url: row.url,
            provider: row.provider,
            filename: row.filename,
            dest_path: row.dest_path,
            size: row.size,
            bytes_downloaded: row.bytes_downloaded,
            status,
            speed_bps: 0,
            eta_secs: 0,
            is_folder: false,
            children: None,
            retry_count: row.retry_count,
            max_retries: 3,
            speed_limit_kib: 0,
            parallel_parts: 4,
            selected_children: None,
            retry_at: None,
            captcha_type: None,
            captcha_sitekey: None,
            captcha_page_url: None,
            captcha_token: None,
            error,
            created_at: row.created_at,
        };

        {
            let mut map = state.downloads.lock().await;
            map.insert(row.id.clone(), download);
        }

        // Atualiza o status no banco
        if let Ok(db) = state.db.lock() {
            let status_str = if reachable { "paused" } else { "error" };
            let _ = crate::db::update_status(&db, &row.id, status_str, None);
        }
    }
}

fn prettify_download_error(message: &str) -> String {
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

fn classify_retry_policy(message: &str, attempt: u32) -> RetryPolicy {
    let lower = message.to_lowercase();
    let fallback_delay = (attempt + 1) as u64;
    let pretty = prettify_download_error(message);

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
