use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::{
    models::{AddDownloadRequest, ApiError, Download, DownloadStatus, WsEvent},
    providers,
    ws::AppState,
};

// Adiciona um novo download à fila e inicia o processo em background
// POST /downloads — body: { "url": "...", "dest_dir": "..." }
pub async fn add_download(
    State(state): State<AppState>,
    Json(req): Json<AddDownloadRequest>,
) -> Result<Json<Download>, (StatusCode, Json<ApiError>)> {
    // Detecta qual provider trata essa URL
    let provider = providers::detect_provider(&req.url).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "URL não reconhecida. Provedores suportados: Mega, MediaFire, Google Drive, PixelDrain",
            )),
        )
    })?;

    // Busca informações do arquivo (nome, tamanho) antes de criar o item na fila
    let file_info = provider.get_file_info(&req.url).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(format!("Falha ao obter informações do arquivo: {e}"))),
        )
    })?;

    // Monta o caminho completo do arquivo de destino
    let dest_path = format!(
        "{}/{}",
        req.dest_dir.trim_end_matches('/'),
        file_info.filename
    );

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
        error: None,
        created_at: now,
    };

    // Adiciona ao mapa compartilhado antes de spawnar a task
    // O lock() aguarda exclusividade (como um mutex.lock() em outras linguagens)
    {
        let mut map = state.downloads.lock().await;
        map.insert(id.clone(), download.clone());
    } // O lock é liberado automaticamente aqui (RAII — sem precisar chamar unlock())

    // Spawna uma task assíncrona para baixar em background
    // tokio::spawn é como criar uma Promise sem await — roda independentemente
    // clone() é necessário porque a closure captura por valor (move)
    let state_clone = state.clone();
    let url_clone = req.url.clone();
    tokio::spawn(async move {
        run_download(state_clone, id, url_clone, dest_path).await;
    });

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
    let mut map = state.downloads.lock().await;
    if map.remove(&id).is_some() {
        // Notifica a UI via WebSocket que o download foi cancelado
        state.broadcast(WsEvent::Error {
            id: id.clone(),
            message: "Cancelado pelo usuário".to_string(),
        });
        Ok(StatusCode::NO_CONTENT) // 204 = sucesso sem body
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::new("Download não encontrado")),
        ))
    }
}

// --- Executa o download em background ---
// Esta função roda em uma task separada do tokio
// É como um Worker ou uma Promise longa rodando em paralelo
async fn run_download(state: AppState, id: String, url: String, dest_path: String) {
    // Atualiza o status para Downloading no mapa compartilhado
    {
        let mut map = state.downloads.lock().await;
        if let Some(d) = map.get_mut(&id) {
            d.status = DownloadStatus::Downloading;
        }
    }

    // Canal interno para receber atualizações de progresso do provider
    // mpsc = Multiple Producer, Single Consumer — o provider envia, nós recebemos
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(64);

    // Detecta o provider para esta URL
    let provider = match providers::detect_provider(&url) {
        Some(p) => p,
        None => {
            update_error(&state, &id, "Provider não encontrado para a URL").await;
            return;
        }
    };

    // Spawna o download em outra task para podermos monitorar progresso ao mesmo tempo
    // JoinHandle permite aguardar o resultado quando a task terminar
    let url_clone = url.clone();
    let dest_clone = dest_path.clone();
    let download_task = tokio::spawn(async move {
        provider.download(&url_clone, &dest_clone, progress_tx).await
    });

    // Fica em loop recebendo atualizações de progresso e enviando ao WebSocket
    let mut last_bytes = 0u64;
    let mut last_time = std::time::Instant::now();

    while let Some(update) = progress_rx.recv().await {
        // Calcula velocidade em bytes/segundo
        let elapsed = last_time.elapsed().as_secs_f64();
        let speed = if elapsed > 0.1 {
            let delta = update.bytes_downloaded.saturating_sub(last_bytes);
            last_bytes = update.bytes_downloaded;
            last_time = std::time::Instant::now();
            (delta as f64 / elapsed) as u64
        } else {
            // Não atualiza velocidade se o intervalo for muito pequeno
            0
        };

        // Calcula ETA (tempo estimado restante)
        let eta = if speed > 0 && update.total_bytes > 0 {
            update.total_bytes.saturating_sub(update.bytes_downloaded) / speed
        } else {
            0
        };

        // Atualiza o mapa e emite evento WebSocket
        {
            let mut map = state.downloads.lock().await;
            if let Some(d) = map.get_mut(&id) {
                d.bytes_downloaded = update.bytes_downloaded;
                d.speed_bps = speed;
                d.eta_secs = eta;
            }
        }

        state.broadcast(WsEvent::Progress {
            id: id.clone(),
            bytes: update.bytes_downloaded,
            total: update.total_bytes,
            speed,
            eta,
            status: DownloadStatus::Downloading,
        });
    }

    // Aguarda a task de download terminar e verifica o resultado
    match download_task.await {
        Ok(Ok(_bytes)) => {
            // Download concluído com sucesso
            {
                let mut map = state.downloads.lock().await;
                if let Some(d) = map.get_mut(&id) {
                    d.status = DownloadStatus::Complete;
                    d.bytes_downloaded = d.size; // Marca como 100%
                }
            }
            state.broadcast(WsEvent::Complete {
                id: id.clone(),
                path: dest_path,
            });
        }
        Ok(Err(e)) => {
            // Download falhou com erro do provider
            update_error(&state, &id, &e.to_string()).await;
        }
        Err(_) => {
            // A task foi cancelada (JoinError)
            update_error(&state, &id, "Download interrompido").await;
        }
    }
}

// Helper: atualiza status para Error e emite evento WebSocket
async fn update_error(state: &AppState, id: &str, message: &str) {
    {
        let mut map = state.downloads.lock().await;
        if let Some(d) = map.get_mut(id) {
            d.status = DownloadStatus::Error;
            d.error = Some(message.to_string());
        }
    }
    state.broadcast(WsEvent::Error {
        id: id.to_string(),
        message: message.to_string(),
    });
}
