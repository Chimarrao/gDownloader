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
    let file_info = provider.get_file_info(&req.url).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(format!("Falha ao obter informações do arquivo: {e}"))),
        )
    })?;

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
        error: None,
        created_at: now,
    };

    // Adiciona ao mapa compartilhado antes de spawnar a task
    // O lock() aguarda exclusividade (como um mutex.lock() em outras linguagens)
    {
        let mut map = state.downloads.lock().await;
        map.insert(id.clone(), download.clone());
    } // O lock é liberado automaticamente aqui (RAII — sem precisar chamar unlock())

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
    let mut map = state.downloads.lock().await;
    map.retain(|_, download| {
        matches!(
            download.status,
            DownloadStatus::Pending | DownloadStatus::Downloading | DownloadStatus::Paused
        )
    });
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
    let (max_retries, speed_limit_bps, parallel_parts) = {
        {
            let mut map = state.downloads.lock().await;
            if let Some(d) = map.get_mut(&id) {
                d.status = DownloadStatus::Downloading;
                d.error = None;
                d.retry_count = 0;
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
        (max_retries, speed_limit_bps, parallel_parts)
    };

    for attempt in 0..=max_retries {
        {
            {
                let mut map = state.downloads.lock().await;
                if let Some(d) = map.get_mut(&id) {
                    d.status = DownloadStatus::Downloading;
                    d.retry_count = attempt;
                    d.bytes_downloaded = 0;
                    d.speed_bps = 0;
                    d.eta_secs = 0;
                    d.error = None;
                }
            }
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
        let download_task = tokio::spawn(async move {
            provider
                .download(&url_clone, &dest_clone, speed_limit_bps, parallel_parts as usize, progress_tx)
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

        while let Some(update) = progress_rx.recv().await {
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
                            for child in children.iter_mut() {
                                if child.filename == child_filename {
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
                state.broadcast(WsEvent::Complete {
                    id: id.clone(),
                    path: dest_path,
                });
                reschedule_pending_downloads(state.clone());
                return;
            }
            Ok(Err(e)) => {
                state.active_tasks.lock().await.remove(&id);
                if attempt < max_retries {
                    {
                        let mut map = state.downloads.lock().await;
                        if let Some(d) = map.get_mut(&id) {
                            d.status = DownloadStatus::Pending;
                            d.error = Some(format!(
                                "Falha na tentativa {} de {}. Tentando novamente...",
                                attempt + 1,
                                max_retries + 1
                            ));
                        }
                    }
                    state.broadcast(WsEvent::Status {
                        id: id.clone(),
                        status: DownloadStatus::Pending,
                    });
                    tokio::time::sleep(std::time::Duration::from_secs((attempt + 1) as u64)).await;
                    continue;
                }
                update_error(&state, &id, &prettify_download_error(&e.to_string())).await;
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
        let mut pending = map
            .values()
            .filter(|download| matches!(download.status, DownloadStatus::Pending))
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
    state.broadcast(WsEvent::Error {
        id: id.to_string(),
        message: message.to_string(),
    });
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
