// Package torrentengine implementa o suporte a torrents do gDownloader inteiramente em
// Go, usando github.com/anacrolix/torrent (client BitTorrent puro-Go, sem depender de
// nenhum cliente de torrent externo instalado na máquina do usuário).
//
// Kill switch por-torrent: quando TorRequired=true, o torrent SÓ roda em um
// *torrent.Client dedicado cujo único dialer de peers é um SOCKS5 (Tor) — não há
// dialer direto configurado nesse client, então uma conexão de peer nunca escapa
// do circuito Tor. Limitação conhecida (documentada para o usuário): DHT e trackers
// UDP não passam por SOCKS5 (Tor é TCP-only), então no modo Tor o DHT fica
// desligado e só trackers HTTP(S) são usados — trackers UDP eventualmente
// presentes na lista do torrent simplesmente falham em vez de vazar IP by-design,
// mas o dialer de UDP tracker desta lib não é roteável por SOCKS5 nesta versão.
package torrentengine

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/anacrolix/torrent"
	"github.com/anacrolix/torrent/metainfo"
	"github.com/anacrolix/torrent/storage"
	"github.com/google/uuid"
	"golang.org/x/net/proxy"
)

type FileStatus struct {
	Index          int    `json:"index"`
	Path           string `json:"path"`
	Size           int64  `json:"size"`
	BytesCompleted int64  `json:"bytesCompleted"`
	Selected       bool   `json:"selected"`
}

type PeerStatus struct {
	Address       string  `json:"address"`
	ClientName    string  `json:"clientName,omitempty"`
	Source        string  `json:"source"`
	DownloadBps   float64 `json:"downloadBps"`
	PieceCount    int     `json:"pieceCount"`
	PercentPieces float64 `json:"percentPieces"`
}

type TorrentStatus struct {
	ID              string       `json:"id"`
	InfoHash        string       `json:"infoHash"`
	Name            string       `json:"name"`
	Source          string       `json:"source"`
	SourceKind      string       `json:"sourceKind"`
	DestDir         string       `json:"destDir"`
	TorRequired     bool         `json:"torRequired"`
	Paused          bool         `json:"paused"`
	Checking        bool         `json:"checking"`
	Status          string       `json:"status"` // fetching_metadata|downloading|seeding|paused|checking|error|done
	BytesCompleted  int64        `json:"bytesCompleted"`
	TotalBytes      int64        `json:"totalBytes"`
	Progress        float64      `json:"progress"`
	DownloadBps     uint64       `json:"downloadBps"`
	UploadBps       uint64       `json:"uploadBps"`
	NumPeers        int          `json:"numPeers"`
	NumSeeds        int          `json:"numSeeds"`
	CreatedAt       int64        `json:"createdAt"`
	Error           string       `json:"error,omitempty"`
	Files           []FileStatus `json:"files,omitempty"`
}

type record struct {
	id          string
	source      string
	sourceKind  string
	destDir     string
	torRequired bool
	paused      bool
	createdAt   int64

	mu         sync.Mutex
	t          *torrent.Torrent
	err        string
	checking   bool
	selection  map[int]bool // nil = todos os arquivos selecionados
	lastRead   int64
	lastWrite  int64
	lastSample time.Time
	downBps    uint64
	upBps      uint64
}

type Engine struct {
	db *sql.DB

	mu           sync.Mutex
	direct       *torrent.Client
	tor          *torrent.Client
	torSocksPort int
	managed      map[string]*record

	closeOnce sync.Once
}

func NewEngine(db *sql.DB) *Engine {
	e := &Engine{db: db, managed: map[string]*record{}}
	go e.sampleLoop()
	return e
}

// SetTorSocksPort espelha /config/tor-runtime do Rust — chamado pelo main do Electron
// sempre que o daemon Tor gerenciado sobe/desce. 0/negativo = Tor desligado.
func (e *Engine) SetTorSocksPort(port int) {
	e.mu.Lock()
	defer e.mu.Unlock()
	if port == e.torSocksPort {
		return
	}
	e.torSocksPort = port
	// Não recriamos o client Tor já em uso (torrents em andamento continuam com o
	// dialer antigo até serem re-adicionados); só afeta próximos Add() com tor_required.
}

func (e *Engine) torPort() int {
	e.mu.Lock()
	defer e.mu.Unlock()
	return e.torSocksPort
}

func (e *Engine) directClient() (*torrent.Client, error) {
	e.mu.Lock()
	defer e.mu.Unlock()
	if e.direct != nil {
		return e.direct, nil
	}
	cfg := torrent.NewDefaultClientConfig()
	cfg.Seed = true
	cl, err := torrent.NewClient(cfg)
	if err != nil {
		return nil, fmt.Errorf("falha ao iniciar client de torrent: %w", err)
	}
	e.direct = cl
	return cl, nil
}

// torClient devolve (criando se preciso) o client BitTorrent isolado por Tor. Erro se
// o Tor não estiver rodando — nunca cai para o client direto (kill switch).
func (e *Engine) torClientLocked() (*torrent.Client, error) {
	if e.tor != nil {
		return e.tor, nil
	}
	if e.torSocksPort <= 0 {
		return nil, errors.New("tor_required: Tor não está ativo (ligue o Tor na barra superior)")
	}
	socksAddr := fmt.Sprintf("127.0.0.1:%d", e.torSocksPort)
	dialer, err := proxy.SOCKS5("tcp", socksAddr, nil, proxy.Direct)
	if err != nil {
		return nil, fmt.Errorf("falha ao configurar SOCKS5 do Tor: %w", err)
	}

	cfg := torrent.NewDefaultClientConfig()
	cfg.Seed = true
	// BitTorrent-sobre-Tor: Tor só roteia TCP. DHT e uTP (UDP) vazariam o IP real,
	// então ficam desligados neste client — só peers TCP e trackers HTTP(S), ambos
	// forçados a passar pelo dialer SOCKS5 abaixo.
	cfg.NoDHT = true
	cfg.DisableUTP = true
	cfg.DisablePEX = false
	cfg.DialForPeerConns = false // sem isso o client tentaria discar direto também
	cfg.TrackerDialContext = func(ctx context.Context, network, addr string) (net.Conn, error) {
		return dialer.Dial("tcp", addr)
	}
	cl, err := torrent.NewClient(cfg)
	if err != nil {
		return nil, fmt.Errorf("falha ao iniciar client de torrent (Tor): %w", err)
	}
	cl.AddDialer(torDialer{dialer})
	e.tor = cl
	return cl, nil
}

// torDialer adapta um proxy.Dialer (sem contexto) para dialer.T do anacrolix/torrent.
type torDialer struct {
	d proxy.Dialer
}

func (td torDialer) DialerNetwork() string { return "tcp" }
func (td torDialer) Dial(_ context.Context, addr string) (net.Conn, error) {
	return td.d.Dial("tcp", addr)
}

func destStorage(destDir string) (storage.ClientImpl, error) {
	if err := os.MkdirAll(destDir, 0o755); err != nil {
		return nil, err
	}
	completion, err := storage.NewDefaultPieceCompletionForDir(destDir)
	if err != nil {
		return nil, err
	}
	return storage.NewFileWithCompletion(destDir, completion), nil
}

func sourceKindOf(source string) string {
	if strings.HasPrefix(strings.ToLower(source), "magnet:") {
		return "magnet"
	}
	return "file"
}

func specFromSource(source, kind string) (*torrent.TorrentSpec, error) {
	if kind == "magnet" {
		return torrent.TorrentSpecFromMagnetUri(source)
	}
	mi, err := metainfo.LoadFromFile(source)
	if err != nil {
		return nil, fmt.Errorf("falha ao ler arquivo .torrent: %w", err)
	}
	return torrent.TorrentSpecFromMetaInfoErr(mi)
}

// Add adiciona um novo torrent (magnet URI ou caminho para arquivo .torrent) e começa
// a baixar em destDir. Se torRequired=true, exige Tor ativo — nunca inicia sem ele.
func (e *Engine) Add(source, destDir string, torRequired bool) (*TorrentStatus, error) {
	source = strings.TrimSpace(source)
	if source == "" {
		return nil, errors.New("fonte do torrent vazia")
	}
	if destDir == "" {
		return nil, errors.New("pasta de destino vazia")
	}
	id := uuid.NewString()
	rec, err := e.startTorrent(id, source, destDir, torRequired, false, nil)
	if err != nil {
		return nil, err
	}
	rec.createdAt = time.Now().Unix()
	e.persistInsert(rec)
	e.mu.Lock()
	e.managed[id] = rec
	e.mu.Unlock()
	return e.statusOf(rec), nil
}

func (e *Engine) startTorrent(id, source, destDir string, torRequired, paused bool, selection map[int]bool) (*record, error) {
	kind := sourceKindOf(source)
	spec, err := specFromSource(source, kind)
	if err != nil {
		return nil, err
	}
	st, err := destStorage(destDir)
	if err != nil {
		return nil, fmt.Errorf("falha ao preparar pasta de destino: %w", err)
	}
	spec.Storage = st

	var cl *torrent.Client
	if torRequired {
		e.mu.Lock()
		cl, err = e.torClientLocked()
		e.mu.Unlock()
		if err != nil {
			return nil, err
		}
	} else {
		cl, err = e.directClient()
		if err != nil {
			return nil, err
		}
	}

	t, isNew, err := cl.AddTorrentSpec(spec)
	if err != nil {
		return nil, fmt.Errorf("falha ao adicionar torrent: %w", err)
	}
	_ = isNew

	rec := &record{
		id:          id,
		source:      source,
		sourceKind:  kind,
		destDir:     destDir,
		torRequired: torRequired,
		paused:      paused,
		t:           t,
		selection:   selection,
		lastSample:  time.Now(),
	}

	go func() {
		select {
		case <-t.GotInfo():
			rec.mu.Lock()
			applySelectionLocked(t, rec.selection)
			shouldPause := rec.paused
			rec.mu.Unlock()
			if shouldPause {
				t.DisallowDataDownload()
			} else {
				t.DownloadAll()
			}
		case <-time.After(10 * time.Minute):
			rec.mu.Lock()
			rec.err = "tempo esgotado esperando metadados do torrent (sem peers?)"
			rec.mu.Unlock()
		}
	}()

	if paused {
		t.DisallowDataDownload()
	}

	return rec, nil
}

func applySelectionLocked(t *torrent.Torrent, selection map[int]bool) {
	if selection == nil {
		t.DownloadAll()
		return
	}
	for i, f := range t.Files() {
		if selection[i] {
			f.Download()
		} else {
			f.Cancel()
		}
	}
}

func (e *Engine) get(id string) (*record, error) {
	e.mu.Lock()
	defer e.mu.Unlock()
	rec, ok := e.managed[id]
	if !ok {
		return nil, errors.New("torrent não encontrado")
	}
	return rec, nil
}

func (e *Engine) List() []*TorrentStatus {
	e.mu.Lock()
	recs := make([]*record, 0, len(e.managed))
	for _, r := range e.managed {
		recs = append(recs, r)
	}
	e.mu.Unlock()

	out := make([]*TorrentStatus, 0, len(recs))
	for _, r := range recs {
		out = append(out, e.statusOf(r))
	}
	return out
}

func (e *Engine) Get(id string) (*TorrentStatus, error) {
	rec, err := e.get(id)
	if err != nil {
		return nil, err
	}
	return e.statusOf(rec), nil
}

func (e *Engine) statusOf(rec *record) *TorrentStatus {
	rec.mu.Lock()
	defer rec.mu.Unlock()

	st := &TorrentStatus{
		ID:          rec.id,
		Source:      rec.source,
		SourceKind:  rec.sourceKind,
		DestDir:     rec.destDir,
		TorRequired: rec.torRequired,
		Paused:      rec.paused,
		Checking:    rec.checking,
		CreatedAt:   rec.createdAt,
		Error:       rec.err,
		DownloadBps: rec.downBps,
		UploadBps:   rec.upBps,
	}

	t := rec.t
	if t == nil {
		st.Status = "error"
		return st
	}
	st.InfoHash = t.InfoHash().HexString()

	select {
	case <-t.GotInfo():
		st.Name = t.Name()
		st.TotalBytes = t.Length()
		st.BytesCompleted = t.BytesCompleted()
		if st.TotalBytes > 0 {
			st.Progress = float64(st.BytesCompleted) / float64(st.TotalBytes)
		}
		gauges := t.Stats().TorrentGauges
		st.NumPeers = gauges.ActivePeers
		st.NumSeeds = gauges.ConnectedSeeders

		files := t.Files()
		st.Files = make([]FileStatus, 0, len(files))
		for i, f := range files {
			selected := rec.selection == nil || rec.selection[i]
			st.Files = append(st.Files, FileStatus{
				Index:          i,
				Path:           f.Path(),
				Size:           f.Length(),
				BytesCompleted: f.BytesCompleted(),
				Selected:       selected,
			})
		}

		switch {
		case rec.err != "":
			st.Status = "error"
		case rec.checking:
			st.Status = "checking"
		case rec.paused:
			st.Status = "paused"
		case st.TotalBytes > 0 && st.BytesCompleted >= st.TotalBytes:
			st.Status = "seeding"
		default:
			st.Status = "downloading"
		}
	default:
		st.Name = strings.TrimSpace(rec.source)
		if rec.err != "" {
			st.Status = "error"
		} else {
			st.Status = "fetching_metadata"
		}
	}

	return st
}

func (e *Engine) Pause(id string) error {
	rec, err := e.get(id)
	if err != nil {
		return err
	}
	rec.mu.Lock()
	rec.paused = true
	t := rec.t
	rec.mu.Unlock()
	if t != nil {
		t.DisallowDataDownload()
	}
	e.persistPaused(id, true)
	return nil
}

func (e *Engine) Resume(id string) error {
	rec, err := e.get(id)
	if err != nil {
		return err
	}
	rec.mu.Lock()
	rec.paused = false
	rec.err = ""
	t := rec.t
	rec.mu.Unlock()
	if t != nil {
		t.AllowDataDownload()
	}
	e.persistPaused(id, false)
	return nil
}

// Remove para o torrent e o esquece. deleteFiles apaga os dados baixados do disco.
func (e *Engine) Remove(id string, deleteFiles bool) error {
	rec, err := e.get(id)
	if err != nil {
		return err
	}
	rec.mu.Lock()
	t := rec.t
	destDir := rec.destDir
	rec.mu.Unlock()

	if t != nil {
		t.Drop()
	}
	e.mu.Lock()
	delete(e.managed, id)
	e.mu.Unlock()
	e.persistRemoved(id)

	if deleteFiles {
		if err := os.RemoveAll(destDir); err != nil {
			log.Printf("torrentengine: falha ao apagar %s: %v", destDir, err)
		}
	}
	return nil
}

func (e *Engine) Recheck(id string) error {
	rec, err := e.get(id)
	if err != nil {
		return err
	}
	rec.mu.Lock()
	if rec.checking {
		rec.mu.Unlock()
		return nil
	}
	rec.checking = true
	t := rec.t
	rec.mu.Unlock()
	if t == nil {
		rec.mu.Lock()
		rec.checking = false
		rec.mu.Unlock()
		return errors.New("torrent ainda não tem metadados")
	}
	go func() {
		_ = t.VerifyDataContext(context.Background())
		rec.mu.Lock()
		rec.checking = false
		rec.mu.Unlock()
	}()
	return nil
}

func (e *Engine) SelectFiles(id string, indices []int) error {
	rec, err := e.get(id)
	if err != nil {
		return err
	}
	rec.mu.Lock()
	sel := make(map[int]bool, len(indices))
	for _, i := range indices {
		sel[i] = true
	}
	rec.selection = sel
	t := rec.t
	rec.mu.Unlock()
	if t != nil {
		select {
		case <-t.GotInfo():
			applySelectionLocked(t, sel)
		default:
		}
	}
	e.persistSelection(id, sel)
	return nil
}

func (e *Engine) Peers(id string) ([]PeerStatus, error) {
	rec, err := e.get(id)
	if err != nil {
		return nil, err
	}
	rec.mu.Lock()
	t := rec.t
	rec.mu.Unlock()
	if t == nil {
		return nil, nil
	}
	conns := t.PeerConns()
	out := make([]PeerStatus, 0, len(conns))
	numPieces := t.NumPieces()
	for _, pc := range conns {
		stats := pc.Peer.Stats()
		percent := 0.0
		if numPieces > 0 {
			percent = float64(stats.RemotePieceCount) / float64(numPieces) * 100
		}
		clientName, _ := pc.PeerClientName.Load().(string)
		out = append(out, PeerStatus{
			Address:       pc.RemoteAddr.String(),
			ClientName:    clientName,
			Source:        string(pc.Discovery),
			DownloadBps:   stats.DownloadRate,
			PieceCount:    stats.RemotePieceCount,
			PercentPieces: percent,
		})
	}
	return out, nil
}

func (e *Engine) sampleLoop() {
	ticker := time.NewTicker(time.Second)
	defer ticker.Stop()
	for range ticker.C {
		e.mu.Lock()
		recs := make([]*record, 0, len(e.managed))
		for _, r := range e.managed {
			recs = append(recs, r)
		}
		e.mu.Unlock()

		for _, rec := range recs {
			rec.mu.Lock()
			t := rec.t
			last := rec.lastSample
			lastRead := rec.lastRead
			lastWrite := rec.lastWrite
			rec.mu.Unlock()
			if t == nil {
				continue
			}
			select {
			case <-t.GotInfo():
			default:
				continue
			}
			stats := t.Stats()
			read := stats.BytesReadUsefulData.Int64()
			written := stats.BytesWrittenData.Int64()
			elapsed := time.Since(last).Seconds()
			if elapsed <= 0 {
				elapsed = 1
			}
			downBps := uint64(0)
			upBps := uint64(0)
			if lastRead > 0 && read > lastRead {
				downBps = uint64(float64(read-lastRead) / elapsed)
			}
			if lastWrite > 0 && written > lastWrite {
				upBps = uint64(float64(written-lastWrite) / elapsed)
			}
			rec.mu.Lock()
			rec.lastRead = read
			rec.lastWrite = written
			rec.lastSample = time.Now()
			rec.downBps = downBps
			rec.upBps = upBps
			rec.mu.Unlock()
		}
	}
}

// --- Persistência (tabela `torrents`, compartilhada com o Rust só pelo arquivo
// SQLite — só o Go lê/escreve nela) ---

func (e *Engine) persistInsert(rec *record) {
	if e.db == nil {
		return
	}
	selJSON := selectionJSON(rec.selection)
	_, err := e.db.Exec(
		`INSERT INTO torrents (id, info_hash, name, source, source_kind, dest_dir, tor_required, paused, file_selection_json, created_at)
		 VALUES (?, '', '', ?, ?, ?, ?, ?, ?, ?)`,
		rec.id, rec.source, rec.sourceKind, rec.destDir, boolToInt(rec.torRequired), boolToInt(rec.paused), selJSON, rec.createdAt,
	)
	if err != nil {
		log.Printf("torrentengine: falha ao persistir torrent %s: %v", rec.id, err)
	}
}

func (e *Engine) persistPaused(id string, paused bool) {
	if e.db == nil {
		return
	}
	if _, err := e.db.Exec(`UPDATE torrents SET paused = ? WHERE id = ?`, boolToInt(paused), id); err != nil {
		log.Printf("torrentengine: falha ao persistir pause de %s: %v", id, err)
	}
}

func (e *Engine) persistSelection(id string, sel map[int]bool) {
	if e.db == nil {
		return
	}
	if _, err := e.db.Exec(`UPDATE torrents SET file_selection_json = ? WHERE id = ?`, selectionJSON(sel), id); err != nil {
		log.Printf("torrentengine: falha ao persistir seleção de %s: %v", id, err)
	}
}

func (e *Engine) persistRemoved(id string) {
	if e.db == nil {
		return
	}
	if _, err := e.db.Exec(`UPDATE torrents SET removed_at = ? WHERE id = ?`, time.Now().Unix(), id); err != nil {
		log.Printf("torrentengine: falha ao persistir remoção de %s: %v", id, err)
	}
}

func selectionJSON(sel map[int]bool) *string {
	if sel == nil {
		return nil
	}
	indices := make([]int, 0, len(sel))
	for i, v := range sel {
		if v {
			indices = append(indices, i)
		}
	}
	b, err := json.Marshal(indices)
	if err != nil {
		return nil
	}
	s := string(b)
	return &s
}

func boolToInt(b bool) int {
	if b {
		return 1
	}
	return 0
}

// LoadPersisted recarrega, na inicialização, todos os torrents ainda não removidos
// (dest_dir, fonte, tor_required, paused, seleção de arquivos) e os reinicia. Pastas
// com dados parciais já no disco são reconferidas automaticamente pela lib (hash
// check) via storage.NewFileWithCompletion.
func (e *Engine) LoadPersisted() {
	if e.db == nil {
		return
	}
	rows, err := e.db.Query(`SELECT id, source, source_kind, dest_dir, tor_required, paused, file_selection_json, created_at
	                          FROM torrents WHERE removed_at IS NULL`)
	if err != nil {
		log.Printf("torrentengine: falha ao carregar torrents persistidos: %v", err)
		return
	}
	defer rows.Close()

	type saved struct {
		id, source, sourceKind, destDir string
		torRequired, paused             bool
		selectionJSON                   sql.NullString
		createdAt                       int64
	}
	var items []saved
	for rows.Next() {
		var s saved
		var torReq, paused int
		if err := rows.Scan(&s.id, &s.source, &s.sourceKind, &s.destDir, &torReq, &paused, &s.selectionJSON, &s.createdAt); err != nil {
			continue
		}
		s.torRequired = torReq != 0
		s.paused = paused != 0
		items = append(items, s)
	}

	for _, s := range items {
		var selection map[int]bool
		if s.selectionJSON.Valid && s.selectionJSON.String != "" {
			var indices []int
			if err := json.Unmarshal([]byte(s.selectionJSON.String), &indices); err == nil {
				selection = make(map[int]bool, len(indices))
				for _, i := range indices {
					selection[i] = true
				}
			}
		}
		rec, err := e.startTorrent(s.id, s.source, s.destDir, s.torRequired, s.paused, selection)
		if err != nil {
			log.Printf("torrentengine: falha ao restaurar torrent %s: %v", s.id, err)
			continue
		}
		rec.createdAt = s.createdAt
		e.mu.Lock()
		e.managed[s.id] = rec
		e.mu.Unlock()
	}
}
