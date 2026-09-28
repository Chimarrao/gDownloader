package ws

import (
	"encoding/json"
	"log"
	"net/http"
	"sync"
	"time"

	"github.com/gorilla/websocket"
)

// Portado de backend/src/ws.rs — WebSocket handler para eventos de download
// Mantém um hub simples com broadcast para múltiplos clientes.
// Em Go sidecar, não há scheduler de downloads, mas mantemos o canal para compatibilidade
// e para que a UI possa conectar em Go quando Rust estiver indisponível.

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool { return true },
	ReadBufferSize: 1024,
	WriteBufferSize: 1024,
}

type Hub struct {
	mu      sync.RWMutex
	clients map[*websocket.Conn]bool
	broadcast chan []byte
}

var globalHub = &Hub{
	clients: make(map[*websocket.Conn]bool),
	broadcast: make(chan []byte, 256),
}

func init() {
	go globalHub.run()
}

func (h *Hub) run() {
	for msg := range h.broadcast {
		h.mu.RLock()
		for c := range h.clients {
			// Set write deadline to avoid blocking
			_ = c.SetWriteDeadline(time.Now().Add(5 * time.Second))
			if err := c.WriteMessage(websocket.TextMessage, msg); err != nil {
				// will be cleaned up on next read failure
				log.Printf("ws broadcast write error: %v", err)
			}
		}
		h.mu.RUnlock()
	}
}

func (h *Hub) register(conn *websocket.Conn) {
	h.mu.Lock()
	h.clients[conn] = true
	h.mu.Unlock()
}

func (h *Hub) unregister(conn *websocket.Conn) {
	h.mu.Lock()
	delete(h.clients, conn)
	h.mu.Unlock()
	_ = conn.Close()
}

// Broadcast envia JSON para todos os clientes conectados
func Broadcast(v interface{}) {
	data, err := json.Marshal(v)
	if err != nil {
		return
	}
	select {
	case globalHub.broadcast <- data:
	default:
		// drop if full, like Rust's broadcast with capacity
	}
}

func BroadcastRaw(data []byte) {
	select {
	case globalHub.broadcast <- data:
	default:
	}
}

// Handler GET /ws — upgrade para WebSocket
func Handler(w http.ResponseWriter, r *http.Request) {
	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		log.Printf("ws upgrade failed: %v", err)
		return
	}
	globalHub.register(conn)
	defer globalHub.unregister(conn)

	// Keep connection alive, echo pings, and discard incoming messages (as Rust does)
	conn.SetReadDeadline(time.Now().Add(60 * time.Second))
	conn.SetPongHandler(func(string) error {
		_ = conn.SetReadDeadline(time.Now().Add(60 * time.Second))
		return nil
	})
	// Ping ticker
	go func() {
		ticker := time.NewTicker(30 * time.Second)
		defer ticker.Stop()
		for {
			time.Sleep(30 * time.Second)
			// check if still registered
			globalHub.mu.RLock()
			_, ok := globalHub.clients[conn]
			globalHub.mu.RUnlock()
			if !ok {
				return
			}
			_ = conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if err := conn.WriteMessage(websocket.PingMessage, nil); err != nil {
				return
			}
		}
	}()

	for {
		_, _, err := conn.ReadMessage()
		if err != nil {
			break
		}
		// Rust ignores client messages; we do same
		_ = conn.SetReadDeadline(time.Now().Add(60 * time.Second))
	}
}
