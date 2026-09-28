package db

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	_ "modernc.org/sqlite"
	"gdownloader-go/internal/models"
)

// Open abre o SQLite em WAL (mesmo que Rust rusqlite bundled) — permite Rust+Go compartilharem o arquivo.
// Portado de backend/src/db.rs:13 — PRAGMA journal_mode=WAL, foreign_keys, wal_autocheckpoint, etc.
func Open(path string) (*sql.DB, error) {
	db, err := sql.Open("sqlite", path+"?_journal_mode=WAL&_synchronous=NORMAL&_busy_timeout=5000&_foreign_keys=ON")
	if err != nil {
		return nil, err
	}
	if err := db.Ping(); err != nil {
		return nil, err
	}
	// Garante WAL mesmo se o driver ignorar o parâmetro de DSN (igual ao Rust)
	for _, pragma := range []string{
		`PRAGMA journal_mode=WAL;`,
		`PRAGMA foreign_keys=ON;`,
		`PRAGMA wal_autocheckpoint=1000;`,
		`PRAGMA journal_size_limit=67108864;`,
		`PRAGMA busy_timeout=5000;`,
	} {
		if _, err := db.Exec(pragma); err != nil {
			// não fatal — continua
			_ = err
		}
	}
	return db, nil
}

// CheckpointWAL faz checkpoint TRUNCATE — portado de db.rs:49, chamado periodicamente pelo main.go
func CheckpointWAL(db *sql.DB) {
	_, _ = db.Exec(`PRAGMA wal_checkpoint(TRUNCATE);`)
}

// Helpers genéricos para settings (app_kv)
func loadPublicSettings(db *sql.DB) (models.PublicSettings, error) {
	var raw string
	err := db.QueryRow(`SELECT value FROM app_kv WHERE key='public_settings'`).Scan(&raw)
	if err != nil {
		return models.PublicSettings{}, err
	}
	var s models.PublicSettings
	if err := json.Unmarshal([]byte(raw), &s); err != nil {
		return models.PublicSettings{}, err
	}
	return s, nil
}

func savePublicSettings(db *sql.DB, s models.PublicSettings) error {
	b, _ := json.Marshal(s)
	_, err := db.Exec(`INSERT INTO app_kv(key,value,updated_at) VALUES('public_settings',?,strftime('%s','now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at`, string(b))
	return err
}

func loadSecureSettings(db *sql.DB) (models.SecureSettings, error) {
	var raw string
	err := db.QueryRow(`SELECT value FROM app_kv WHERE key='secure_settings'`).Scan(&raw)
	if err != nil {
		return models.SecureSettings{}, err
	}
	var s models.SecureSettings
	if err := json.Unmarshal([]byte(raw), &s); err != nil {
		return models.SecureSettings{}, err
	}
	return s, nil
}

func saveSecureSettings(db *sql.DB, s models.SecureSettings) error {
	b, _ := json.Marshal(s)
	_, err := db.Exec(`INSERT INTO app_kv(key,value,updated_at) VALUES('secure_settings',?,strftime('%s','now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at`, string(b))
	return err
}

// Wrapper exportado para handlers
func LoadPublicSettings(db *sql.DB) (models.PublicSettings, error) { return loadPublicSettings(db) }
func SavePublicSettings(db *sql.DB, s models.PublicSettings) error { return savePublicSettings(db, s) }
func LoadSecureSettings(db *sql.DB) (models.SecureSettings, error) { return loadSecureSettings(db) }
func SaveSecureSettings(db *sql.DB, s models.SecureSettings) error { return saveSecureSettings(db, s) }

func GetDBPath(db *sql.DB) *string { return nil }

// Packages helpers — portado de routes/packages.rs + migrations 9
func ListPackages(db *sql.DB) ([]models.Package, error) {
	rows, err := db.Query(`SELECT id, name, color, comment, dest_dir_override, priority, created_at FROM packages ORDER BY priority DESC, created_at DESC`)
	if err != nil {
		if strings.Contains(err.Error(), "no such table") {
			return []models.Package{}, nil
		}
		return nil, err
	}
	defer rows.Close()
	var out []models.Package
	for rows.Next() {
		var p models.Package
		var comment sql.NullString
		var dest sql.NullString
		var created int64
		if err := rows.Scan(&p.ID, &p.Name, &p.Color, &comment, &dest, &p.Priority, &created); err != nil {
			return nil, err
		}
		if comment.Valid {
			p.Comment = &comment.String
		}
		if dest.Valid {
			p.DestDirOverride = &dest.String
		}
		p.CreatedAt = uint64(created)
		out = append(out, p)
	}
	if out == nil {
		out = []models.Package{}
	}
	return out, nil
}

func InsertPackage(db *sql.DB, p models.Package) error {
	_, err := db.Exec(`INSERT INTO packages (id, name, color, comment, dest_dir_override, priority, created_at) VALUES (?,?,?,?,?,?,?)`, p.ID, p.Name, p.Color, p.Comment, p.DestDirOverride, p.Priority, int64(p.CreatedAt))
	return err
}

func DeletePackage(db *sql.DB, id string) error {
	if _, err := db.Exec(`UPDATE downloads SET package_id = NULL WHERE package_id = ?`, id); err != nil {
		return err
	}
	_, err := db.Exec(`DELETE FROM packages WHERE id = ?`, id)
	return err
}

func AssignDownloadToPackage(db *sql.DB, packageID, downloadID string) error {
	_, err := db.Exec(`UPDATE downloads SET package_id = ? WHERE id = ?`, packageID, downloadID)
	return err
}

func UnassignDownloadFromPackage(db *sql.DB, downloadID string) error {
	_, err := db.Exec(`UPDATE downloads SET package_id = NULL WHERE id = ?`, downloadID)
	return err
}

// Intercept history — portado de db.rs:782
func InsertInterceptHistory(db *sql.DB, id, url, filename, mimeType string, size uint64, status string, createdAt uint64) error {
	_, err := db.Exec(`INSERT INTO intercept_history (id, url, filename, mime_type, size, status, created_at) VALUES (?,?,?,?,?,?,?)`, id, url, filename, mimeType, int64(size), status, int64(createdAt))
	return err
}

func ListInterceptHistory(db *sql.DB, limit int) ([]map[string]interface{}, error) {
	rows, err := db.Query(`SELECT id, url, filename, mime_type, size, status, created_at FROM intercept_history ORDER BY created_at DESC LIMIT ?`, limit)
	if err != nil {
		if strings.Contains(err.Error(), "no such table") {
			return []map[string]interface{}{}, nil
		}
		return nil, err
	}
	defer rows.Close()
	var out []map[string]interface{}
	for rows.Next() {
		var id, url, filename, mimeType, status string
		var size, createdAt int64
		if err := rows.Scan(&id, &url, &filename, &mimeType, &size, &status, &createdAt); err != nil {
			return nil, err
		}
		out = append(out, map[string]interface{}{
			"id": id, "url": url, "filename": filename, "mimeType": mimeType, "size": size, "status": status, "createdAt": uint64(createdAt),
		})
	}
	if out == nil {
		out = []map[string]interface{}{}
	}
	return out, nil
}

// File info cache — portado de db.rs:1095
func LoadCachedFileInfo(db *sql.DB, rawURL string) (*models.CachedFileInfo, error) {
	row := db.QueryRow(`SELECT url, provider_id, name, size, mime_type, is_folder, children_json, thumbnail_url, channel_name, channel_thumbnail_url, cached_at, last_checked_at, duration_secs FROM file_info_cache WHERE url = ?`, rawURL)
	var c models.CachedFileInfo
	var mime sql.NullString
	var childrenJSON sql.NullString
	var thumb sql.NullString
	var chName sql.NullString
	var chThumb sql.NullString
	var cachedAt, lastChecked sql.NullInt64
	var duration sql.NullInt64
	var isFolder int
	var size int64
	var providerID, name, url string
	if err := row.Scan(&url, &providerID, &name, &size, &mime, &isFolder, &childrenJSON, &thumb, &chName, &chThumb, &cachedAt, &lastChecked, &duration); err != nil {
		if err == sql.ErrNoRows {
			return nil, nil
		}
		if strings.Contains(err.Error(), "no such table") {
			return nil, nil
		}
		return nil, err
	}
	c.URL = url
	c.ProviderID = providerID
	c.Name = name
	c.Size = uint64(size)
	if mime.Valid {
		c.MimeType = &mime.String
	}
	c.IsFolder = isFolder != 0
	if childrenJSON.Valid && childrenJSON.String != "" {
		var children []models.FileChildInfo
		// children stored as JSON; Rust stores Option<Vec<FileChildInfo>> as JSON
		_ = json.Unmarshal([]byte(childrenJSON.String), &children)
		// Also try wrapper Option
		if children != nil {
			c.Children = &children
		} else {
			var opt *[]models.FileChildInfo
			if err := json.Unmarshal([]byte(childrenJSON.String), &opt); err == nil {
				c.Children = opt
			}
		}
	}
	if thumb.Valid {
		c.ThumbnailURL = &thumb.String
	}
	if chName.Valid {
		c.ChannelName = &chName.String
	}
	if chThumb.Valid {
		c.ChannelThumbnailURL = &chThumb.String
	}
	if cachedAt.Valid {
		c.CachedAt = uint64(cachedAt.Int64)
	}
	if lastChecked.Valid {
		v := uint64(lastChecked.Int64)
		c.LastCheckedAt = &v
	}
	if duration.Valid {
		v := uint64(duration.Int64)
		c.DurationSecs = &v
	}
	return &c, nil
}

func SaveCachedFileInfo(db *sql.DB, url, providerID, name string, size uint64, durationSecs *uint64, mimeType *string, isFolder bool, children *[]models.FileChildInfo, thumbnailURL *string, channelName *string, channelThumbnailURL *string) error {
	childrenJSON, _ := json.Marshal(children)
	var mime interface{}
	if mimeType != nil {
		mime = *mimeType
	}
	var dur interface{}
	if durationSecs != nil {
		dur = int64(*durationSecs)
	}
	now := nowSecs()
	_, err := db.Exec(`INSERT INTO file_info_cache (url, provider_id, name, size, mime_type, is_folder, children_json, thumbnail_url, channel_name, channel_thumbnail_url, cached_at, last_checked_at, duration_secs) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(url) DO UPDATE SET provider_id=excluded.provider_id, name=excluded.name, size=excluded.size, mime_type=excluded.mime_type, is_folder=excluded.is_folder, children_json=excluded.children_json, thumbnail_url=excluded.thumbnail_url, channel_name=excluded.channel_name, channel_thumbnail_url=excluded.channel_thumbnail_url, duration_secs=excluded.duration_secs, cached_at=excluded.cached_at, last_checked_at=excluded.last_checked_at`, url, providerID, name, int64(size), mime, boolToInt(isFolder), string(childrenJSON), thumbnailURL, channelName, channelThumbnailURL, now, now, dur)
	return err
}

func FileInfoCacheStats(db *sql.DB) (uint64, uint64, error) {
	var count int64
	var bytes sql.NullInt64
	err := db.QueryRow(`SELECT COUNT(*), COALESCE(SUM(LENGTH(url) + LENGTH(provider_id) + LENGTH(name) + COALESCE(LENGTH(mime_type),0) + COALESCE(LENGTH(children_json),0)),0) FROM file_info_cache`).Scan(&count, &bytes)
	if err != nil {
		if strings.Contains(err.Error(), "no such table") {
			return 0, 0, nil
		}
		return 0, 0, err
	}
	var b uint64
	if bytes.Valid {
		b = uint64(bytes.Int64)
	}
	return uint64(count), b, nil
}

func ClearFileInfoCache(db *sql.DB) (uint64, error) {
	res, err := db.Exec(`DELETE FROM file_info_cache`)
	if err != nil {
		if strings.Contains(err.Error(), "no such table") {
			return 0, nil
		}
		return 0, err
	}
	n, _ := res.RowsAffected()
	return uint64(n), nil
}

func nowSecs() int64 {
	return time.Now().Unix()
}

func boolToInt(b bool) int { if b { return 1 }; return 0 }

func LoadLegacyMigrations(db *sql.DB) ([]models.LegacyConfigMigration, error) {
	rows, err := db.Query(`SELECT version, name, applied_at FROM legacy_config_migrations ORDER BY version`)
	if err != nil {
		// Tabela pode não existir em DBs antigos antes da migração 1
		if strings.Contains(err.Error(), "no such table") {
			return []models.LegacyConfigMigration{}, nil
		}
		return nil, err
	}
	defer rows.Close()
	var out []models.LegacyConfigMigration
	for rows.Next() {
		var m models.LegacyConfigMigration
		if err := rows.Scan(&m.Version, &m.Name, &m.AppliedAt); err != nil {
			return nil, err
		}
		out = append(out, m)
	}
	return out, nil
}

func MarkLegacyMigration(db *sql.DB, version int64, name string) error {
	_, err := db.Exec(`INSERT INTO legacy_config_migrations(version,name,applied_at) VALUES(?,?,strftime('%s','now')) ON CONFLICT(version) DO NOTHING`, version, name)
	return err
}

// History helpers (portado de db.rs)
func SearchHistory(db *sql.DB, q, host, from, to *string, page, pageSize int) ([]models.HistoryItem, error) {
	// Usa FTS quando q presente, senão scan normal
	var rows *sql.Rows
	var err error
	if q != nil && *q != "" {
		// FTS5
		query := `
			SELECT h.id, h.url, h.title, h.host, h.thumbnail, h.date, h.format_id, h.output_path, h.sha256_hash
			FROM history_fts f JOIN download_history h ON h.rowid = f.rowid
			WHERE history_fts MATCH ? ORDER BY h.date DESC LIMIT ? OFFSET ?`
		rows, err = db.Query(query, *q, pageSize, page*pageSize)
	} else {
		// Filtro simples por host/data
		where := []string{"1=1"}
		args := []interface{}{}
		if host != nil && *host != "" {
			where = append(where, "host = ?")
			args = append(args, *host)
		}
		if from != nil && *from != "" {
			where = append(where, "date >= ?")
			args = append(args, *from)
		}
		if to != nil && *to != "" {
			where = append(where, "date <= ?")
			args = append(args, *to)
		}
		args = append(args, pageSize, page*pageSize)
		query := fmt.Sprintf(`SELECT id, url, title, host, thumbnail, date, format_id, output_path, sha256_hash FROM download_history WHERE %s ORDER BY date DESC LIMIT ? OFFSET ?`, strings.Join(where, " AND "))
		rows, err = db.Query(query, args...)
	}
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []models.HistoryItem
	for rows.Next() {
		var h models.HistoryItem
		var outputPath sql.NullString
		var sha sql.NullString
		var hostVal sql.NullString
		if err := rows.Scan(&h.ID, &h.URL, &h.Title, &hostVal, &h.Thumbnail, &h.Date, &h.FormatID, &outputPath, &sha); err != nil {
			return nil, err
		}
		if hostVal.Valid {
			h.Host = hostVal.String
		}
		if outputPath.Valid {
			h.OutputPath = &outputPath.String
		}
		if sha.Valid {
			h.Sha256Hash = &sha.String
		}
		out = append(out, h)
	}
	return out, nil
}

func ListHistoryHosts(db *sql.DB) ([]string, error) {
	rows, err := db.Query(`SELECT DISTINCT host FROM download_history WHERE host != '' ORDER BY host`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []string
	for rows.Next() {
		var h string
		if err := rows.Scan(&h); err != nil {
			return nil, err
		}
		out = append(out, h)
	}
	return out, nil
}

func ReplaceHistory(db *sql.DB, items []models.HistoryItem) error {
	tx, err := db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(`DELETE FROM download_history`); err != nil {
		return err
	}
	for _, h := range items {
		if _, err := tx.Exec(`INSERT INTO download_history(id,url,title,host,thumbnail,date,format_id,output_path,sha256_hash) VALUES(?,?,?,?,?,?,?,?,?)`,
			h.ID, h.URL, h.Title, h.Host, h.Thumbnail, h.Date, h.FormatID, h.OutputPath, h.Sha256Hash); err != nil {
			return err
		}
	}
	return tx.Commit()
}

func UpsertHistoryItem(db *sql.DB, h models.HistoryItem) error {
	_, err := db.Exec(`INSERT INTO download_history(id,url,title,host,thumbnail,date,format_id,output_path,sha256_hash) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET url=excluded.url, title=excluded.title, host=excluded.host, thumbnail=excluded.thumbnail, date=excluded.date, format_id=excluded.format_id, output_path=excluded.output_path, sha256_hash=excluded.sha256_hash`,
		h.ID, h.URL, h.Title, h.Host, h.Thumbnail, h.Date, h.FormatID, h.OutputPath, h.Sha256Hash)
	return err
}

func DeleteHistoryItem(db *sql.DB, id string) error {
	_, err := db.Exec(`DELETE FROM download_history WHERE id=?`, id)
	return err
}

func ClearHistory(db *sql.DB) error {
	_, err := db.Exec(`DELETE FROM download_history`)
	return err
}

func SaveRawSettings(db *sql.DB, key string, raw json.RawMessage) error {
	_, err := db.Exec(`INSERT INTO app_kv(key,value,updated_at) VALUES(?,?,strftime('%s','now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at`, key, string(raw))
	return err
}

func ReplaceHistoryRaw(db *sql.DB, raw json.RawMessage) error {
	var items []models.HistoryItem
	if err := json.Unmarshal(raw, &items); err != nil {
		return err
	}
	return ReplaceHistory(db, items)
}

func UpsertHistoryItemRaw(db *sql.DB, raw json.RawMessage) error {
	var h models.HistoryItem
	if err := json.Unmarshal(raw, &h); err != nil {
		return err
	}
	return UpsertHistoryItem(db, h)
}
