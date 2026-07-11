use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{broadcast, Mutex};
use tokio::task::AbortHandle;

use crate::models::{Download, WsEvent};
use crate::stats::StatsManager;

// Capacidade máxima do buffer do canal de broadcast
// Se a UI estiver lenta para processar mensagens, eventos antigos são descartados
// para não travar o servidor — como uma fila circular com limite
const CHANNEL_CAPACITY: usize = 256;

// AppState = estado global compartilhado entre todos os handlers HTTP e WebSocket
// Arc = "Atomic Reference Counted" — permite múltiplas referências ao mesmo dado
//       entre threads sem copiar. É como passar um objeto por referência no PHP,
//       mas thread-safe. O dado só é destruído quando todas as referências somem.
// Clone aqui não copia o dado — só incrementa o contador de referências (barato)
#[derive(Clone)]
pub struct AppState {
    pub tx: Arc<broadcast::Sender<WsEvent>>,
    pub downloads: Arc<Mutex<HashMap<String, Download>>>,
    pub active_tasks: Arc<Mutex<HashMap<String, AbortHandle>>>,
    /// Limites de velocidade vivos (bytes/s, 0 = ilimitado) dos downloads em execução.
    /// O handler PATCH atualiza o valor atômico e o loop de streaming do provider lê
    /// na próxima iteração — logo mudanças valem em tempo real (inclusive Mega).
    pub speed_limits: Arc<Mutex<HashMap<String, crate::providers::SpeedLimitBps>>>,
    pub max_concurrent_downloads: Arc<Mutex<usize>>,
    pub db: Arc<StdMutex<rusqlite::Connection>>,
    pub db_path: Option<String>,
    pub stats: Arc<StatsManager>,
    /// Porta SOCKS do daemon Tor quando ele está rodando para roteamento
    /// isolado por-download. `None` quando o Tor isolado não está disponível.
    /// Definido pelo processo main via `POST /config/tor-runtime`, independente
    /// do `proxy_mode` global.
    pub isolated_tor_port: Arc<Mutex<Option<u16>>>,
}

// Como um class em PHP — agrupa métodos desta struct
impl AppState {
    pub fn new(db: rusqlite::Connection) -> Self {
        Self::new_with_max(db, 3)
    }

    pub fn new_with_max(db: rusqlite::Connection, max_concurrent_downloads: usize) -> Self {
        Self::new_with_max_and_db_path(db, max_concurrent_downloads, None)
    }

    pub fn new_with_max_and_db_path(
        db: rusqlite::Connection,
        max_concurrent_downloads: usize,
        db_path: Option<String>,
    ) -> Self {
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            tx: Arc::new(tx),
            downloads: Arc::new(Mutex::new(HashMap::new())),
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
            speed_limits: Arc::new(Mutex::new(HashMap::new())),
            max_concurrent_downloads: Arc::new(Mutex::new(max_concurrent_downloads.max(1))),
            db: Arc::new(StdMutex::new(db)),
            db_path,
            stats: Arc::new(StatsManager::new(60)),
            isolated_tor_port: Arc::new(Mutex::new(None)),
        }
    }

    // Envia um evento para todos os clientes WebSocket conectados
    // `let _ =` descarta o Result — se não há clientes, o erro é ignorado (situação normal)
    pub fn broadcast(&self, event: WsEvent) {
        let _ = self.tx.send(event);
    }
}

// Handler de upgrade HTTP → WebSocket
// Chamado pelo Axum quando o Vue conecta em ws://localhost:PORT/ws
// WebSocketUpgrade é injetado automaticamente pelo Axum quando detecta o header "Upgrade: websocket"
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    // State injeta o AppState — como injeção de dependência no Axum
    // Equivalente a obter um service do container DI no Laravel
    State(state): State<AppState>,
) -> impl IntoResponse {
    // on_upgrade aceita o handshake e passa o socket para handle_socket
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

// Gerencia uma conexão WebSocket individual
// Cada cliente Vue que conectar terá sua própria execução desta função
// rodando em paralelo — como processos independentes no PHP-FPM, mas dentro de uma task tokio
async fn handle_socket(socket: WebSocket, state: AppState) {
    // split() divide o socket em duas metades independentes:
    //   sender   = para enviar mensagens ao cliente Vue
    //   receiver = para receber mensagens do cliente Vue
    // Equivalente a separar os streams readable/writable de uma conexão TCP no Node.js
    let (mut sender, mut receiver) = socket.split();

    // Cria um receptor dedicado para este cliente no canal de broadcast
    // Cada subscribe() cria uma nova "assinatura" — recebe todos os eventos futuros
    // Eventos anteriores à conexão não são entregues (sem replay)
    let mut rx = state.tx.subscribe();

    // tokio::select! aguarda múltiplas operações async ao mesmo tempo
    // Executa o primeiro bloco que completar — como Promise.race() no JavaScript
    // O loop continua até o cliente desconectar ou o servidor encerrar
    loop {
        tokio::select! {
            // Evento interno chegou (ex: progresso de download de outra task)
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        // Serializa o evento para JSON — como json_encode() no PHP
                        if let Ok(json) = serde_json::to_string(&event) {
                            // Envia pelo WebSocket; se falhar, o cliente desconectou
                            if sender.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    // Canal fechado (servidor sendo encerrado) — encerra o loop
                    Err(_) => break,
                }
            }
            // Mensagem chegou do cliente (ex: ping automático do browser)
            msg = receiver.next() => {
                match msg {
                    // None = stream encerrado = cliente desconectou
                    None => break,
                    // Mensagem recebida — ignorada por enquanto (sem comandos do cliente)
                    Some(_) => {}
                }
            }
        }
    }
}
