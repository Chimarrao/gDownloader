package migrations

import (
	"database/sql"
	"fmt"
)

// Portado de backend/src/migrations.rs:10 — 22 migrações idempotentes (CREATE IF NOT EXISTS)
// Mantém os mesmos version/name para compatibilidade com Rust.

type Migration struct {
	Version int64
	Name    string
	Apply   func(*sql.DB) error
}

var Migrations = []Migration{
	{1, "create_core_tables", migrationCreateCoreTables},
	{2, "ensure_download_columns", migrationEnsureDownloadColumns},
	{3, "create_download_indexes", migrationCreateDownloadIndexes},
	{4, "create_history_table", migrationCreateHistoryTable},
	{5, "create_file_cache_table", migrationCreateFileCacheTable},
	{6, "create_direct_http_resume_table", migrationCreateDirectHttpResumeTable},
	{7, "create_stats_hourly_table", migrationCreateStatsHourlyTable},
	{8, "add_download_pinned_column", migrationAddDownloadPinnedColumn},
	{9, "create_packages_table", migrationCreatePackagesTable},
	{10, "add_history_hash_columns", migrationAddHistoryHashColumns},
	{11, "create_download_events_table", migrationCreateDownloadEventsTable},
	{12, "create_archive_passwords_table", migrationCreateArchivePasswordsTable},
	{13, "create_history_fts", migrationCreateHistoryFts},
	{14, "create_intercept_history_table", migrationCreateInterceptHistoryTable},
	{15, "add_download_network_route", migrationAddDownloadNetworkRoute},
	{16, "add_file_cache_media_columns", migrationAddFileCacheMediaColumns},
	{17, "add_download_auto_tor_on_limit", migrationAddDownloadAutoTorOnLimit},
	{18, "reensure_download_columns", migrationEnsureDownloadColumns},
	{19, "add_media_duration_columns", migrationAddMediaDurationColumns},
	{20, "add_download_thumbnail_columns", migrationAddDownloadThumbnailColumns},
	{21, "create_resolved_download_link_cache", migrationCreateResolvedDownloadLinkCache},
	{22, "add_download_error_kind", migrationAddDownloadErrorKind},
}

func columnExists(db *sql.DB, table, column string) (bool, error) {
	rows, err := db.Query(fmt.Sprintf("PRAGMA table_info(%s)", table))
	if err != nil {
		return false, err
	}
	defer rows.Close()
	var cid int
	var name, typ string
	var notnull, pk int
	var dflt sql.NullString
	for rows.Next() {
		if err := rows.Scan(&cid, &name, &typ, &notnull, &dflt, &pk); err != nil {
			return false, err
		}
		if name == column {
			return true, nil
		}
	}
	return false, nil
}

func addColumnIfMissing(db *sql.DB, table, column, ddl string) error {
	exists, err := columnExists(db, table, column)
	if err != nil {
		return err
	}
	if !exists {
		_, err = db.Exec(ddl)
		return err
	}
	return nil
}

func Run(db *sql.DB) error {
	// Garante tabela de controle de migrações
	if _, err := db.Exec(`CREATE TABLE IF NOT EXISTS legacy_config_migrations(version INTEGER PRIMARY KEY, name TEXT, applied_at INTEGER)`); err != nil {
		return err
	}
	for _, m := range Migrations {
		var exists int
		err := db.QueryRow(`SELECT 1 FROM legacy_config_migrations WHERE version=?`, m.Version).Scan(&exists)
		if err == nil {
			continue // já aplicada
		}
		if err := m.Apply(db); err != nil {
			return fmt.Errorf("migration %d %s: %w", m.Version, m.Name, err)
		}
		if _, err := db.Exec(`INSERT INTO legacy_config_migrations(version,name,applied_at) VALUES(?,?,strftime('%s','now'))`, m.Version, m.Name); err != nil {
			return err
		}
	}
	return nil
}

// --- Implementações idênticas ao Rust ---

func migrationCreateCoreTables(db *sql.DB) error {
	_, err := db.Exec(`
          CREATE TABLE IF NOT EXISTS downloads (
              id                     TEXT PRIMARY KEY,
              url                    TEXT NOT NULL,
              provider               TEXT NOT NULL DEFAULT '',
              identity_key           TEXT NOT NULL DEFAULT '',
              filename               TEXT NOT NULL DEFAULT '',
              dest_path              TEXT NOT NULL DEFAULT '',
              size                   INTEGER NOT NULL DEFAULT 0,
              duration_secs          INTEGER,
              bytes_downloaded       INTEGER NOT NULL DEFAULT 0,
              status                 TEXT NOT NULL DEFAULT 'pending',
              is_folder              INTEGER NOT NULL DEFAULT 0,
              children_json          TEXT,
              max_retries            INTEGER NOT NULL DEFAULT 0,
              speed_limit_kib        INTEGER NOT NULL DEFAULT 0,
              parallel_parts         INTEGER NOT NULL DEFAULT 1,
              selected_children_json TEXT,
              error                  TEXT,
              error_kind             TEXT,
              retry_count            INTEGER NOT NULL DEFAULT 0,
              retry_at               INTEGER,
              captcha_type           TEXT,
              captcha_sitekey        TEXT,
              captcha_page_url       TEXT,
              captcha_token          TEXT,
              priority               INTEGER NOT NULL DEFAULT 0,
              created_at             INTEGER NOT NULL,
              started_at             INTEGER,
              completed_at           INTEGER,
              last_progress_at       INTEGER,
              network_route_json     TEXT,
              updated_at             INTEGER NOT NULL
          );
          CREATE TABLE IF NOT EXISTS app_kv (
              key                    TEXT PRIMARY KEY,
              value                  TEXT NOT NULL,
              updated_at             INTEGER NOT NULL
          );`)
	return err
}

func migrationEnsureDownloadColumns(db *sql.DB) error {
	cols := [][2]string{
		{"is_folder", "ALTER TABLE downloads ADD COLUMN is_folder INTEGER NOT NULL DEFAULT 0"},
		{"children_json", "ALTER TABLE downloads ADD COLUMN children_json TEXT"},
		{"max_retries", "ALTER TABLE downloads ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 0"},
		{"speed_limit_kib", "ALTER TABLE downloads ADD COLUMN speed_limit_kib INTEGER NOT NULL DEFAULT 0"},
		{"parallel_parts", "ALTER TABLE downloads ADD COLUMN parallel_parts INTEGER NOT NULL DEFAULT 1"},
		{"selected_children_json", "ALTER TABLE downloads ADD COLUMN selected_children_json TEXT"},
		{"expected_hash_json", "ALTER TABLE downloads ADD COLUMN expected_hash_json TEXT"},
		{"captcha_type", "ALTER TABLE downloads ADD COLUMN captcha_type TEXT"},
		{"captcha_sitekey", "ALTER TABLE downloads ADD COLUMN captcha_sitekey TEXT"},
		{"captcha_page_url", "ALTER TABLE downloads ADD COLUMN captcha_page_url TEXT"},
		{"captcha_token", "ALTER TABLE downloads ADD COLUMN captcha_token TEXT"},
		{"identity_key", "ALTER TABLE downloads ADD COLUMN identity_key TEXT NOT NULL DEFAULT ''"},
		{"priority", "ALTER TABLE downloads ADD COLUMN priority INTEGER NOT NULL DEFAULT 0"},
		{"started_at", "ALTER TABLE downloads ADD COLUMN started_at INTEGER"},
		{"completed_at", "ALTER TABLE downloads ADD COLUMN completed_at INTEGER"},
		{"last_progress_at", "ALTER TABLE downloads ADD COLUMN last_progress_at INTEGER"},
		{"network_route_json", "ALTER TABLE downloads ADD COLUMN network_route_json TEXT"},
		{"duration_secs", "ALTER TABLE downloads ADD COLUMN duration_secs INTEGER"},
	}
	for _, c := range cols {
		if err := addColumnIfMissing(db, "downloads", c[0], c[1]); err != nil {
			return err
		}
	}
	return nil
}

func migrationAddDownloadErrorKind(db *sql.DB) error {
	return addColumnIfMissing(db, "downloads", "error_kind", "ALTER TABLE downloads ADD COLUMN error_kind TEXT")
}
func migrationAddDownloadNetworkRoute(db *sql.DB) error {
	return addColumnIfMissing(db, "downloads", "network_route_json", "ALTER TABLE downloads ADD COLUMN network_route_json TEXT")
}
func migrationAddDownloadAutoTorOnLimit(db *sql.DB) error {
	return addColumnIfMissing(db, "downloads", "auto_tor_on_limit", "ALTER TABLE downloads ADD COLUMN auto_tor_on_limit INTEGER NOT NULL DEFAULT 0")
}
func migrationCreateDownloadIndexes(db *sql.DB) error {
	_, err := db.Exec(`
          CREATE INDEX IF NOT EXISTS idx_downloads_status_created_at ON downloads(status, created_at DESC);
          CREATE INDEX IF NOT EXISTS idx_downloads_status_retry_at ON downloads(status, retry_at);
          CREATE INDEX IF NOT EXISTS idx_downloads_provider_status_created_at ON downloads(provider, status, created_at DESC);
          CREATE INDEX IF NOT EXISTS idx_downloads_identity_key ON downloads(identity_key);`)
	return err
}
func migrationCreateHistoryTable(db *sql.DB) error {
	_, err := db.Exec(`
          CREATE TABLE IF NOT EXISTS download_history (
              id          TEXT PRIMARY KEY,
              url         TEXT NOT NULL DEFAULT '',
              title       TEXT NOT NULL DEFAULT '',
              thumbnail   TEXT NOT NULL DEFAULT '',
              date        TEXT NOT NULL DEFAULT '',
              format_id   TEXT NOT NULL DEFAULT '',
              output_path TEXT,
              updated_at  INTEGER NOT NULL DEFAULT 0
          );
          CREATE INDEX IF NOT EXISTS idx_download_history_date ON download_history(date DESC, updated_at DESC);`)
	return err
}
func migrationCreateFileCacheTable(db *sql.DB) error {
	_, err := db.Exec(`
          CREATE TABLE IF NOT EXISTS file_info_cache (
              url             TEXT PRIMARY KEY,
              provider_id     TEXT NOT NULL DEFAULT '',
              name            TEXT NOT NULL DEFAULT '',
              size            INTEGER NOT NULL DEFAULT 0,
              mime_type       TEXT,
              is_folder       INTEGER NOT NULL DEFAULT 0,
              children_json   TEXT,
              cached_at       INTEGER NOT NULL,
              last_checked_at INTEGER
          );
          CREATE INDEX IF NOT EXISTS idx_file_info_cache_cached_at ON file_info_cache(cached_at DESC);`)
	return err
}
func migrationCreateResolvedDownloadLinkCache(db *sql.DB) error {
	_, err := db.Exec(`
          CREATE TABLE IF NOT EXISTS resolved_download_link_cache (
              provider_id TEXT NOT NULL,
              source_url  TEXT NOT NULL,
              direct_url  TEXT NOT NULL,
              referer_url TEXT,
              created_at  INTEGER NOT NULL,
              expires_at  INTEGER NOT NULL,
              last_used_at INTEGER NOT NULL,
              PRIMARY KEY (provider_id, source_url)
          );
          CREATE INDEX IF NOT EXISTS idx_resolved_download_link_cache_expiry ON resolved_download_link_cache(expires_at);`)
	return err
}
func migrationAddFileCacheMediaColumns(db *sql.DB) error {
	if err := addColumnIfMissing(db, "file_info_cache", "thumbnail_url", "ALTER TABLE file_info_cache ADD COLUMN thumbnail_url TEXT"); err != nil {
		return err
	}
	if err := addColumnIfMissing(db, "file_info_cache", "channel_name", "ALTER TABLE file_info_cache ADD COLUMN channel_name TEXT"); err != nil {
		return err
	}
	return addColumnIfMissing(db, "file_info_cache", "channel_thumbnail_url", "ALTER TABLE file_info_cache ADD COLUMN channel_thumbnail_url TEXT")
}
func migrationAddMediaDurationColumns(db *sql.DB) error {
	if err := addColumnIfMissing(db, "downloads", "duration_secs", "ALTER TABLE downloads ADD COLUMN duration_secs INTEGER"); err != nil {
		return err
	}
	return addColumnIfMissing(db, "file_info_cache", "duration_secs", "ALTER TABLE file_info_cache ADD COLUMN duration_secs INTEGER")
}
func migrationAddDownloadThumbnailColumns(db *sql.DB) error {
	if err := addColumnIfMissing(db, "downloads", "thumbnail_url", "ALTER TABLE downloads ADD COLUMN thumbnail_url TEXT"); err != nil {
		return err
	}
	if err := addColumnIfMissing(db, "downloads", "channel_name", "ALTER TABLE downloads ADD COLUMN channel_name TEXT"); err != nil {
		return err
	}
	if err := addColumnIfMissing(db, "downloads", "channel_thumbnail_url", "ALTER TABLE downloads ADD COLUMN channel_thumbnail_url TEXT"); err != nil {
		return err
	}
	return addColumnIfMissing(db, "downloads", "thumbnail_data", "ALTER TABLE downloads ADD COLUMN thumbnail_data TEXT")
}
func migrationCreateDirectHttpResumeTable(db *sql.DB) error {
	_, err := db.Exec(`
          CREATE TABLE IF NOT EXISTS direct_http_parts (
              download_key     TEXT NOT NULL,
              part_index       INTEGER NOT NULL,
              url              TEXT NOT NULL DEFAULT '',
              etag             TEXT,
              last_modified    TEXT,
              start_byte       INTEGER NOT NULL,
              end_byte         INTEGER NOT NULL,
              bytes_downloaded INTEGER NOT NULL DEFAULT 0,
              updated_at       INTEGER NOT NULL,
              PRIMARY KEY(download_key, part_index)
          );
          CREATE INDEX IF NOT EXISTS idx_direct_http_parts_updated_at ON direct_http_parts(updated_at DESC);`)
	return err
}
func migrationCreateStatsHourlyTable(db *sql.DB) error {
	_, err := db.Exec(`
          CREATE TABLE IF NOT EXISTS stats_hourly (
              id          INTEGER PRIMARY KEY AUTOINCREMENT,
              url         TEXT NOT NULL,
              provider     TEXT NOT NULL DEFAULT '',
              status      TEXT NOT NULL DEFAULT 'pending',
              is_folder   INTEGER NOT NULL DEFAULT 0,
              size        INTEGER NOT NULL DEFAULT 0,
              downloaded   INTEGER NOT NULL DEFAULT 0,
              error       TEXT,
              retry_count INTEGER NOT NULL DEFAULT 0,
              created_at  INTEGER NOT NULL,
              updated_at  INTEGER NOT NULL
          );
          CREATE INDEX IF NOT EXISTS idx_stats_hourly_url ON stats_hourly(url);
          CREATE INDEX IF NOT EXISTS idx_stats_hourly_provider ON stats_hourly(provider);
          CREATE INDEX IF NOT EXISTS idx_stats_hourly_status ON stats_hourly(status);
          CREATE INDEX IF NOT EXISTS idx_stats_hourly_created_at ON stats_hourly(created_at DESC);`)
	return err
}
func migrationAddDownloadPinnedColumn(db *sql.DB) error {
	return addColumnIfMissing(db, "downloads", "pinned", "ALTER TABLE downloads ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0")
}
func migrationCreatePackagesTable(db *sql.DB) error {
	if _, err := db.Exec(`
          CREATE TABLE IF NOT EXISTS packages (
              id              TEXT PRIMARY KEY,
              name            TEXT NOT NULL DEFAULT '',
              color           TEXT NOT NULL DEFAULT '#7c6fff',
              comment         TEXT,
              dest_dir_override TEXT,
              priority        INTEGER NOT NULL DEFAULT 0,
              created_at      INTEGER NOT NULL
          );
          CREATE INDEX IF NOT EXISTS idx_packages_priority ON packages(priority DESC, created_at DESC);`); err != nil {
		return err
	}
	return addColumnIfMissing(db, "downloads", "package_id", "ALTER TABLE downloads ADD COLUMN package_id TEXT")
}
func migrationAddHistoryHashColumns(db *sql.DB) error {
	if err := addColumnIfMissing(db, "download_history", "sha256_hash", "ALTER TABLE download_history ADD COLUMN sha256_hash TEXT"); err != nil {
		return err
	}
	_, err := db.Exec(`CREATE INDEX IF NOT EXISTS idx_download_history_sha256 ON download_history(sha256_hash);`)
	return err
}
func migrationCreateDownloadEventsTable(db *sql.DB) error {
	_, err := db.Exec(`
          CREATE TABLE IF NOT EXISTS download_events (
              id          INTEGER PRIMARY KEY AUTOINCREMENT,
              download_id TEXT NOT NULL,
              kind        TEXT NOT NULL DEFAULT '',
              message     TEXT NOT NULL DEFAULT '',
              created_at  INTEGER NOT NULL
          );
          CREATE INDEX IF NOT EXISTS idx_download_events_download_created ON download_events(download_id, created_at DESC);`)
	return err
}
func migrationCreateArchivePasswordsTable(db *sql.DB) error {
	_, err := db.Exec(`
          CREATE TABLE IF NOT EXISTS archive_passwords (
              password      TEXT PRIMARY KEY,
              success_count INTEGER NOT NULL DEFAULT 0,
              last_used_at  INTEGER,
              source        TEXT NOT NULL DEFAULT 'manual'
          );
          CREATE INDEX IF NOT EXISTS idx_archive_passwords_success ON archive_passwords(success_count DESC, last_used_at DESC);`)
	return err
}
func migrationCreateHistoryFts(db *sql.DB) error {
	if err := addColumnIfMissing(db, "download_history", "host", "ALTER TABLE download_history ADD COLUMN host TEXT NOT NULL DEFAULT ''"); err != nil {
		return err
	}
	_, err := db.Exec(`
          DROP TRIGGER IF EXISTS download_history_ai;
          DROP TRIGGER IF EXISTS download_history_ad;
          DROP TRIGGER IF EXISTS download_history_au;
          DROP TABLE IF EXISTS history_fts;
          CREATE INDEX IF NOT EXISTS idx_download_history_host ON download_history(host);
          CREATE VIRTUAL TABLE IF NOT EXISTS history_fts USING fts5(filename, url, host);
          CREATE TRIGGER IF NOT EXISTS download_history_ai AFTER INSERT ON download_history BEGIN
              INSERT INTO history_fts(rowid, filename, url, host) VALUES (new.rowid, new.title, new.url, new.host);
          END;
          CREATE TRIGGER IF NOT EXISTS download_history_ad AFTER DELETE ON download_history BEGIN
              DELETE FROM history_fts WHERE rowid = old.rowid;
          END;
          CREATE TRIGGER IF NOT EXISTS download_history_au AFTER UPDATE ON download_history BEGIN
              DELETE FROM history_fts WHERE rowid = old.rowid;
              INSERT INTO history_fts(rowid, filename, url, host) VALUES (new.rowid, new.title, new.url, new.host);
          END;`)
	return err
}
func migrationCreateInterceptHistoryTable(db *sql.DB) error {
	_, err := db.Exec(`
          CREATE TABLE IF NOT EXISTS intercept_history (
              id         TEXT PRIMARY KEY,
              url        TEXT NOT NULL DEFAULT '',
              filename   TEXT NOT NULL DEFAULT '',
              mime_type  TEXT NOT NULL DEFAULT '',
              size       INTEGER NOT NULL DEFAULT 0,
              status     TEXT NOT NULL DEFAULT 'queued',
              created_at INTEGER NOT NULL
          );
          CREATE INDEX IF NOT EXISTS idx_intercept_history_created ON intercept_history(created_at DESC);`)
	return err
}
