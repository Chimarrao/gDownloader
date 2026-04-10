use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

use crate::models::{Download, WsEvent};

// Capacidade máxima do canal de broadcast
// Se a UI estiver lenta para processar, eventos mais antigos são descartados
const CHANNEL_CAPACITY: usize = 256;

// AppState é o estado compartilhado entre todos os handlers HTTP e WebSocket
// Arc = "Atomic Reference Counted" — permite compartilhar o mesmo dado entre threads
// sem precisar copiar. É como um ponteiro shared em PHP ou uma referência em JS.
// Clone aqui não copia o dado — copia o ponteiro (barato)
#[derive(Clone)]
pub struct AppState {
    // Transmissor do canal de broadcast
    // Quando chamamos tx.send(evento), todos os clientes WebSocket conectados recebem
    // É como EventEmitter do Node.js, mas thread-safe
    pub tx: Arc<broadcast::Sender<WsEvent>>,

    // Mapa de downloads ativos e recentes
    // Mutex garante que só uma thread acessa o mapa por vez (como synchronized em Java)
    pub downloads: Arc<Mutex<HashMap<String, Download>>>,
}

impl AppState {
    // Cria um novo AppState — chamado uma vez ao iniciar o servidor
    pub fn new() -> Self {
        // broadcast::channel(N) cria um canal onde múltiplos leitores podem receber
        // o mesmo evento. É como um EventEmitter onde vários "listeners" ouvem o mesmo evento.
        // _rx é o receptor inicial — descartamos porque cada WebSocket client cria o seu próprio
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            tx: Arc::new(tx),
            downloads: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Envia um evento para todos os clientes WebSocket conectados
    // O `_ =` ignora o erro caso não haja nenhum cliente conectado (situação normal)
    pub fn broadcast(&self, event: WsEvent) {
        let _ = self.tx.send(event);
    }
}

// Handler do WebSocket — chamado quando o Vue conecta em ws://localhost:PORT/ws
// WebSocketUpgrade é injetado pelo axum quando detecta um "upgrade" HTTP→WebSocket
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>, // State injeta o AppState (dependency injection do axum)
) -> impl IntoResponse {
    // on_upgrade aceita a conexão WebSocket e passa para handle_socket
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

// Gerencia uma conexão WebSocket individual
// Cada cliente Vue que conectar terá sua própria instância desta função rodando
async fn handle_socket(socket: WebSocket, state: AppState) {
    // split() divide o socket em duas metades independentes:
    // sender = para enviar mensagens ao cliente
    // receiver = para receber mensagens do cliente
    // Equivalente a separar writable e readable no Node.js streams
    let (mut sender, mut receiver) = socket.split();

    // Cria um receptor dedicado para este cliente no canal de broadcast
    // Cada subscribe() cria um novo receptor que recebe todos os eventos futuros
    let mut rx = state.tx.subscribe();

    // tokio::select! aguarda múltiplas operações async simultaneamente
    // Executa o primeiro bloco que completar — similar a Promise.race() no JS
    loop {
        tokio::select! {
            // Evento chegou do canal interno (ex: progresso de download)
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        // Serializa o evento para JSON e envia pelo WebSocket
                        if let Ok(json) = serde_json::to_string(&event) {
                            // Se o envio falhar (cliente desconectou), para o loop
                            if sender.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    // canal foi fechado (servidor parando) — encerra a conexão
                    Err(_) => break,
                }
            }
            // Cliente enviou algo (ex: ping do browser)
            msg = receiver.next() => {
                match msg {
                    // Cliente desconectou — None significa stream encerrado
                    None => break,
                    // Mensagem recebida — ignoramos por enquanto (não temos comandos do cliente)
                    Some(_) => {}
                }
            }
        }
    }
}
