use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{
    ArchivePassword, CachedFileInfo, Download, DownloadEvent, DownloadStatus, DuplicateDownload,
    DuplicateGroup, FileChildInfo, HistoryItem, LegacyConfigMigration, PublicSettings,
    SecureSettings,
};

struct Migration {
    version: i64,
    name: &'static str,
    apply: fn(&Connection) -> Result<()>,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "create_core_tables",
        apply: migration_create_core_tables,
    },
    Migration {
        version: 2,
        name: "ensure_download_columns",
        apply: migration_ensure_download_columns,
    },
    Migration {
        version: 3,
        name: "create_download_indexes",
        apply: migration_create_download_indexes,
    },
    Migration {
        version: 4,
        name: "create_history_table",
        apply: migration_create_history_table,
    },
    Migration {
        version: 5,
        name: "create_file_cache_table",
        apply: migration_create_file_cache_table,
    },
    Migration {
        version: 6,
        name: "create_direct_http_resume_table",
        apply: migration_create_direct_http_resume_table,
    },
    Migration {
        version: 7,
        name: "create_stats_hourly_table",
        apply: migration_create_stats_hourly_table,
    },
    Migration {
        version: 8,
        name: "add_download_pinned_column",
        apply: migration_add_download_pinned_column,
    },
    Migration {
        version: 9,
        name: "create_packages_table",
        apply: migration_create_packages_table,
    },
    Migration {
        version: 10,
        name: "add_history_hash_columns",
        apply: migration_add_history_hash_columns,
    },
    Migration {
        version: 11,
        name: "create_download_events_table",
        apply: migration_create_download_events_table,
    },
    Migration {
        version: 12,
        name: "create_archive_passwords_table",
        apply: migration_create_archive_passwords_table,
    },
];

pub fn init(db_path: &str) -> Result<Connection> {
    if let Some(parent) = std::path::Path::new(db_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut conn = Connection::open(db_path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS schema_migrations (
             version     INTEGER PRIMARY KEY,
             name        TEXT NOT NULL,
             applied_at  INTEGER NOT NULL
         );",
    )?;
    apply_migrations(&mut conn)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}

fn apply_migrations(conn: &mut Connection) -> Result<()> {
    let applied = {
        let mut stmt = conn.prepare("SELECT version FROM schema_migrations ORDER BY version ASC")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, i64>(0))?
            .filter_map(|row| row.ok())
            .collect::<std::collections::HashSet<_>>();
        rows
    };

    for migration in MIGRATIONS {
        if applied.contains(&migration.version) {
            continue;
        }

        let tx = conn.transaction()?;
        (migration.apply)(&tx)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
            params![migration.version, migration.name, now_secs()],
        )?;
        tx.commit()?;
    }

    Ok(())
}

fn migration_create_core_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS downloads (
             id                     TEXT PRIMARY KEY,
             url                    TEXT NOT NULL,
             provider               TEXT NOT NULL DEFAULT '',
             identity_key           TEXT NOT NULL DEFAULT '',
             filename               TEXT NOT NULL DEFAULT '',
             dest_path              TEXT NOT NULL DEFAULT '',
             size                   INTEGER NOT NULL DEFAULT 0,
             bytes_downloaded       INTEGER NOT NULL DEFAULT 0,
             status                 TEXT NOT NULL DEFAULT 'pending',
             is_folder              INTEGER NOT NULL DEFAULT 0,
             children_json          TEXT,
             max_retries            INTEGER NOT NULL DEFAULT 0,
             speed_limit_kib        INTEGER NOT NULL DEFAULT 0,
             parallel_parts         INTEGER NOT NULL DEFAULT 1,
             selected_children_json TEXT,
             error                  TEXT,
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
             updated_at             INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS app_kv (
             key                    TEXT PRIMARY KEY,
             value                  TEXT NOT NULL,
             updated_at             INTEGER NOT NULL
         );",
    )?;
    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|row| row.ok())
        .collect::<std::collections::HashSet<_>>();
    Ok(columns.contains(column))
}

fn add_column_if_missing(conn: &Connection, table: &str, column: &str, ddl: &str) -> Result<()> {
    if !column_exists(conn, table, column)? {
        conn.execute(ddl, [])?;
    }
    Ok(())
}

fn migration_ensure_download_columns(conn: &Connection) -> Result<()> {
    let columns = [
        (
            "is_folder",
            "ALTER TABLE downloads ADD COLUMN is_folder INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "children_json",
            "ALTER TABLE downloads ADD COLUMN children_json TEXT",
        ),
        (
            "max_retries",
            "ALTER TABLE downloads ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "speed_limit_kib",
            "ALTER TABLE downloads ADD COLUMN speed_limit_kib INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "parallel_parts",
            "ALTER TABLE downloads ADD COLUMN parallel_parts INTEGER NOT NULL DEFAULT 1",
        ),
        (
            "selected_children_json",
            "ALTER TABLE downloads ADD COLUMN selected_children_json TEXT",
        ),
        (
            "expected_hash_json",
            "ALTER TABLE downloads ADD COLUMN expected_hash_json TEXT",
        ),
        (
            "captcha_type",
            "ALTER TABLE downloads ADD COLUMN captcha_type TEXT",
        ),
        (
            "captcha_sitekey",
            "ALTER TABLE downloads ADD COLUMN captcha_sitekey TEXT",
        ),
        (
            "captcha_page_url",
            "ALTER TABLE downloads ADD COLUMN captcha_page_url TEXT",
        ),
        (
            "captcha_token",
            "ALTER TABLE downloads ADD COLUMN captcha_token TEXT",
        ),
        (
            "identity_key",
            "ALTER TABLE downloads ADD COLUMN identity_key TEXT NOT NULL DEFAULT ''",
        ),
        (
            "priority",
            "ALTER TABLE downloads ADD COLUMN priority INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "started_at",
            "ALTER TABLE downloads ADD COLUMN started_at INTEGER",
        ),
        (
            "completed_at",
            "ALTER TABLE downloads ADD COLUMN completed_at INTEGER",
        ),
        (
            "last_progress_at",
            "ALTER TABLE downloads ADD COLUMN last_progress_at INTEGER",
        ),
    ];

    for (column, ddl) in columns {
        add_column_if_missing(conn, "downloads", column, ddl)?;
    }

    Ok(())
}

fn migration_create_download_indexes(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_downloads_status_created_at
             ON downloads(status, created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_downloads_status_retry_at
             ON downloads(status, retry_at);
         CREATE INDEX IF NOT EXISTS idx_downloads_provider_status_created_at
             ON downloads(provider, status, created_at DESC);
         CREATE INDEX IF NOT EXISTS idx_downloads_identity_key
             ON downloads(identity_key);",
    )?;
    Ok(())
}

fn migration_create_history_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS download_history (
             id          TEXT PRIMARY KEY,
             url         TEXT NOT NULL DEFAULT '',
             title       TEXT NOT NULL DEFAULT '',
             thumbnail   TEXT NOT NULL DEFAULT '',
             date        TEXT NOT NULL DEFAULT '',
             format_id   TEXT NOT NULL DEFAULT '',
             output_path TEXT,
             updated_at  INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_download_history_date
             ON download_history(date DESC, updated_at DESC);",
    )?;
    Ok(())
}

fn migration_create_file_cache_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS file_info_cache (
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
         CREATE INDEX IF NOT EXISTS idx_file_info_cache_cached_at
             ON file_info_cache(cached_at DESC);",
    )?;
    Ok(())
}

fn migration_create_direct_http_resume_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS direct_http_parts (
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
         CREATE INDEX IF NOT EXISTS idx_direct_http_parts_updated_at
             ON direct_http_parts(updated_at DESC);",
    )?;
    Ok(())
}

fn migration_create_stats_hourly_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS stats_hourly (
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
         CREATE INDEX IF NOT EXISTS idx_stats_hourly_url
             ON stats_hourly(url);
         CREATE INDEX IF NOT EXISTS idx_stats_hourly_provider
             ON stats_hourly(provider);
         CREATE INDEX IF NOT EXISTS idx_stats_hourly_status
             ON stats_hourly(status);
         CREATE INDEX IF NOT EXISTS idx_stats_hourly_created_at
             ON stats_hourly(created_at DESC);",
    )?;
    Ok(())
}

fn migration_add_download_pinned_column(conn: &Connection) -> Result<()> {
    add_column_if_missing(conn, "downloads", "pinned", "ALTER TABLE downloads ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0")?;
    Ok(())
}

fn migration_create_packages_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS packages (
             id              TEXT PRIMARY KEY,
             name            TEXT NOT NULL DEFAULT '',
             color           TEXT NOT NULL DEFAULT '#7c6fff',
             comment         TEXT,
             dest_dir_override TEXT,
             priority        INTEGER NOT NULL DEFAULT 0,
             created_at      INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_packages_priority
             ON packages(priority DESC, created_at DESC);",
    )?;
    add_column_if_missing(conn, "downloads", "package_id",
        "ALTER TABLE downloads ADD COLUMN package_id TEXT")?;
    Ok(())
}

fn migration_add_history_hash_columns(conn: &Connection) -> Result<()> {
    add_column_if_missing(
        conn,
        "download_history",
        "sha256_hash",
        "ALTER TABLE download_history ADD COLUMN sha256_hash TEXT",
    )?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_download_history_sha256
             ON download_history(sha256_hash);",
    )?;
    Ok(())
}

fn migration_create_download_events_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS download_events (
             id          INTEGER PRIMARY KEY AUTOINCREMENT,
             download_id TEXT NOT NULL,
             kind        TEXT NOT NULL DEFAULT '',
             message     TEXT NOT NULL DEFAULT '',
             created_at  INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_download_events_download_created
             ON download_events(download_id, created_at DESC);",
    )?;
    Ok(())
}

fn migration_create_archive_passwords_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS archive_passwords (
             password      TEXT PRIMARY KEY,
             success_count INTEGER NOT NULL DEFAULT 0,
             last_used_at  INTEGER,
             source        TEXT NOT NULL DEFAULT 'manual'
         );
         CREATE INDEX IF NOT EXISTS idx_archive_passwords_success
             ON archive_passwords(success_count DESC, last_used_at DESC);",
    )?;
    Ok(())
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn status_str(s: &DownloadStatus) -> &'static str {
    match s {
        DownloadStatus::Pending => "pending",
        DownloadStatus::Downloading => "downloading",
        DownloadStatus::Verifying => "verifying",
        DownloadStatus::Paused => "paused",
        DownloadStatus::Complete => "complete",
        DownloadStatus::Corrupted => "corrupted",
        DownloadStatus::Error => "error",
        DownloadStatus::Cancelled => "cancelled",
        DownloadStatus::RateLimited => "rate_limited",
        DownloadStatus::WaitingCaptcha => "waiting_captcha",
        DownloadStatus::DiskFull => "disk_full",
    }
}

fn parse_status(raw: &str) -> DownloadStatus {
    match raw {
        "pending" => DownloadStatus::Pending,
        "downloading" => DownloadStatus::Downloading,
        "verifying" => DownloadStatus::Verifying,
        "paused" => DownloadStatus::Paused,
        "complete" => DownloadStatus::Complete,
        "corrupted" => DownloadStatus::Corrupted,
        "error" => DownloadStatus::Error,
        "cancelled" => DownloadStatus::Cancelled,
        "rate_limited" => DownloadStatus::RateLimited,
        "waiting_captcha" => DownloadStatus::WaitingCaptcha,
        "disk_full" => DownloadStatus::DiskFull,
        _ => DownloadStatus::Pending,
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Option<String> {
    serde_json::to_string(value).ok()
}

pub fn upsert(conn: &Connection, d: &Download) -> Result<()> {
    conn.execute(
        "INSERT INTO downloads
             (id, url, provider, identity_key, filename, dest_path, size, bytes_downloaded,
             status, is_folder, children_json, max_retries, speed_limit_kib,
              parallel_parts, selected_children_json, expected_hash_json,
              error, retry_count, retry_at, captcha_type, captcha_sitekey,
              captcha_page_url, captcha_token, priority, created_at, started_at,
              completed_at, last_progress_at, pinned, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30)
         ON CONFLICT(id) DO UPDATE SET
             url                    = excluded.url,
             provider               = excluded.provider,
             identity_key           = excluded.identity_key,
             filename               = excluded.filename,
             dest_path              = excluded.dest_path,
             size                   = excluded.size,
             bytes_downloaded       = excluded.bytes_downloaded,
             status                 = excluded.status,
             is_folder              = excluded.is_folder,
             children_json          = excluded.children_json,
             max_retries            = excluded.max_retries,
             speed_limit_kib        = excluded.speed_limit_kib,
             parallel_parts         = excluded.parallel_parts,
             selected_children_json = excluded.selected_children_json,
             expected_hash_json     = excluded.expected_hash_json,
             error                  = excluded.error,
             retry_count            = excluded.retry_count,
             retry_at               = excluded.retry_at,
             captcha_type           = excluded.captcha_type,
             captcha_sitekey        = excluded.captcha_sitekey,
             captcha_page_url       = excluded.captcha_page_url,
             captcha_token          = excluded.captcha_token,
             priority               = excluded.priority,
             started_at             = excluded.started_at,
             completed_at           = excluded.completed_at,
             last_progress_at       = excluded.last_progress_at,
             pinned                 = excluded.pinned,
             updated_at             = excluded.updated_at",
        params![
            d.id,
            d.url,
            d.provider,
            d.identity_key,
            d.filename,
            d.dest_path,
            d.size as i64,
            d.bytes_downloaded as i64,
            status_str(&d.status),
            if d.is_folder { 1 } else { 0 },
            to_json(&d.children),
            d.max_retries as i64,
            d.speed_limit_kib as i64,
            d.parallel_parts as i64,
            to_json(&d.selected_children),
            to_json(&d.expected_hash),
            d.error,
            d.retry_count as i64,
            d.retry_at.map(|x| x as i64),
            d.captcha_type,
            d.captcha_sitekey,
            d.captcha_page_url,
            d.captcha_token,
            d.priority,
            d.created_at as i64,
            d.started_at.map(|value| value as i64),
            d.completed_at.map(|value| value as i64),
            d.last_progress_at.map(|value| value as i64),
            if d.pinned { 1i64 } else { 0i64 },
            now_secs(),
        ],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM downloads WHERE id=?1", params![id])?;
    Ok(())
}

pub fn delete_finished(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM downloads WHERE status IN ('complete','cancelled','error','corrupted')",
        [],
    )?;
    Ok(())
}

pub fn insert_download_event(
    conn: &Connection,
    download_id: &str,
    kind: &str,
    message: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO download_events (download_id, kind, message, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![download_id, kind, message, now_secs()],
    )?;
    Ok(())
}

pub fn list_download_events(conn: &Connection, download_id: &str, limit: usize) -> Result<Vec<DownloadEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id, download_id, kind, message, created_at
         FROM download_events
         WHERE download_id = ?1
         ORDER BY created_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![download_id, limit.min(200) as i64], |row| {
            Ok(DownloadEvent {
                id: row.get(0)?,
                download_id: row.get(1)?,
                kind: row.get(2)?,
                message: row.get(3)?,
                created_at: row.get::<_, i64>(4)? as u64,
            })
        })?
        .filter_map(|row| row.ok())
        .collect();
    Ok(rows)
}

pub fn list_archive_passwords(conn: &Connection, limit: usize) -> Result<Vec<ArchivePassword>> {
    let mut stmt = conn.prepare(
        "SELECT password, success_count, last_used_at, source
         FROM archive_passwords
         ORDER BY success_count DESC, COALESCE(last_used_at, 0) DESC, password ASC
         LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit.min(500) as i64], |row| {
            Ok(ArchivePassword {
                password: row.get(0)?,
                success_count: row.get::<_, i64>(1)? as u64,
                last_used_at: row.get::<_, Option<i64>>(2)?.map(|value| value as u64),
                source: row.get(3)?,
            })
        })?
        .filter_map(|row| row.ok())
        .collect();
    Ok(rows)
}

pub fn add_archive_password(conn: &Connection, password: &str, source: &str) -> Result<()> {
    let normalized = password.trim();
    if normalized.is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO archive_passwords (password, success_count, last_used_at, source)
         VALUES (?1, 0, NULL, ?2)
         ON CONFLICT(password) DO UPDATE SET
             source = CASE
                 WHEN archive_passwords.source = 'auto' THEN archive_passwords.source
                 ELSE excluded.source
             END",
        params![normalized, source],
    )?;
    Ok(())
}

pub fn record_archive_password_success(conn: &Connection, password: &str, source: &str) -> Result<()> {
    let normalized = password.trim();
    if normalized.is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO archive_passwords (password, success_count, last_used_at, source)
         VALUES (?1, 1, ?2, ?3)
         ON CONFLICT(password) DO UPDATE SET
             success_count = archive_passwords.success_count + 1,
             last_used_at = excluded.last_used_at,
             source = excluded.source",
        params![normalized, now_secs(), source],
    )?;
    Ok(())
}

pub fn delete_archive_password(conn: &Connection, password: &str) -> Result<()> {
    conn.execute("DELETE FROM archive_passwords WHERE password = ?1", params![password])?;
    Ok(())
}

pub fn load_all_downloads(conn: &Connection) -> Result<Vec<Download>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, provider, identity_key, filename, dest_path, size, bytes_downloaded, status,
                is_folder, children_json, retry_count, max_retries, speed_limit_kib,
                parallel_parts, selected_children_json, expected_hash_json, retry_at,
                captcha_type, captcha_sitekey, captcha_page_url, captcha_token,
                error, priority, created_at, started_at, completed_at, last_progress_at,
                COALESCE(pinned, 0) as pinned, package_id
         FROM downloads
         ORDER BY priority DESC, created_at DESC",
    )?;
    let rows = stmt
        .query_map([], |row| {
            let children_json: Option<String> = row.get(10)?;
            let selected_children_json: Option<String> = row.get(15)?;
            let expected_hash_json: Option<String> = row.get(16)?;

            Ok(Download {
                id: row.get(0)?,
                url: row.get(1)?,
                provider: row.get(2)?,
                identity_key: row.get(3)?,
                filename: row.get(4)?,
                dest_path: row.get(5)?,
                size: row.get::<_, i64>(6)? as u64,
                bytes_downloaded: row.get::<_, i64>(7)? as u64,
                status: parse_status(&row.get::<_, String>(8)?),
                is_folder: row.get::<_, i64>(9)? != 0,
                children: children_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Option<Vec<FileChildInfo>>>(raw).ok())
                    .flatten(),
                retry_count: row.get::<_, i64>(11)? as u32,
                max_retries: row.get::<_, i64>(12)? as u32,
                speed_limit_kib: row.get::<_, i64>(13)? as u64,
                parallel_parts: row.get::<_, i64>(14)? as u32,
                selected_children: selected_children_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Option<Vec<String>>>(raw).ok())
                    .flatten(),
                expected_hash: expected_hash_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Option<crate::models::ExpectedHash>>(raw).ok())
                    .flatten(),
                retry_at: row.get::<_, Option<i64>>(17)?.map(|value| value as u64),
                captcha_type: row.get(18)?,
                captcha_sitekey: row.get(19)?,
                captcha_page_url: row.get(20)?,
                captcha_token: row.get(21)?,
                error: row.get(22)?,
                priority: row.get::<_, i64>(23)? as i32,
                created_at: row.get::<_, i64>(24)? as u64,
                started_at: row.get::<_, Option<i64>>(25)?.map(|value| value as u64),
                completed_at: row.get::<_, Option<i64>>(26)?.map(|value| value as u64),
                last_progress_at: row.get::<_, Option<i64>>(27)?.map(|value| value as u64),
                pinned: row.get::<_, i64>(28)? != 0,
                package_id: row.get(29).ok().flatten(),
                speed_bps: 0,
                eta_secs: 0,
            })
        })?
        .filter_map(|row| row.ok())
        .collect();
    Ok(rows)
}

pub fn load_public_settings(conn: &Connection) -> Result<PublicSettings> {
    let raw = conn
        .query_row(
            "SELECT value FROM app_kv WHERE key = 'public_settings'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    Ok(raw
        .as_deref()
        .map(serde_json::from_str::<PublicSettings>)
        .transpose()?
        .unwrap_or_default())
}

pub fn load_legacy_config_migrations(conn: &Connection) -> Result<Vec<LegacyConfigMigration>> {
    let raw = conn
        .query_row(
            "SELECT value FROM app_kv WHERE key = 'legacy_config_migrations'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    Ok(raw
        .as_deref()
        .map(serde_json::from_str::<Vec<LegacyConfigMigration>>)
        .transpose()?
        .unwrap_or_default())
}

pub fn mark_legacy_config_migration(conn: &Connection, version: i64, name: &str) -> Result<()> {
    let mut current = load_legacy_config_migrations(conn)?;
    if current.iter().any(|item| item.version == version) {
        return Ok(());
    }

    current.push(LegacyConfigMigration {
        version,
        name: name.to_string(),
        applied_at: now_secs() as u64,
    });
    current.sort_by(|left, right| left.version.cmp(&right.version));

    conn.execute(
        "INSERT INTO app_kv (key, value, updated_at)
         VALUES ('legacy_config_migrations', ?1, ?2)
         ON CONFLICT(key) DO UPDATE SET
             value = excluded.value,
             updated_at = excluded.updated_at",
        params![serde_json::to_string(&current)?, now_secs()],
    )?;
    Ok(())
}

pub fn save_public_settings(conn: &Connection, settings: &PublicSettings) -> Result<()> {
    let payload = serde_json::to_string(settings)?;
    conn.execute(
        "INSERT INTO app_kv (key, value, updated_at)
         VALUES ('public_settings', ?1, ?2)
         ON CONFLICT(key) DO UPDATE SET
             value = excluded.value,
             updated_at = excluded.updated_at",
        params![payload, now_secs()],
    )?;
    Ok(())
}

pub fn load_secure_settings(conn: &Connection) -> Result<SecureSettings> {
    let raw = conn
        .query_row(
            "SELECT value FROM app_kv WHERE key = 'secure_settings'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    Ok(raw
        .as_deref()
        .map(serde_json::from_str::<SecureSettings>)
        .transpose()?
        .unwrap_or_default())
}

pub fn load_secure_settings_from_path(db_path: &str) -> Result<SecureSettings> {
    let conn = Connection::open(db_path)?;
    load_secure_settings(&conn)
}

pub fn save_secure_settings(conn: &Connection, settings: &SecureSettings) -> Result<()> {
    let payload = serde_json::to_string(settings)?;
    conn.execute(
        "INSERT INTO app_kv (key, value, updated_at)
         VALUES ('secure_settings', ?1, ?2)
         ON CONFLICT(key) DO UPDATE SET
             value = excluded.value,
             updated_at = excluded.updated_at",
        params![payload, now_secs()],
    )?;
    Ok(())
}

pub fn load_history(conn: &Connection) -> Result<Vec<HistoryItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, title, thumbnail, date, format_id, output_path, sha256_hash
         FROM download_history
         ORDER BY date DESC, updated_at DESC",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(HistoryItem {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                thumbnail: row.get(3)?,
                date: row.get(4)?,
                format_id: row.get(5)?,
                output_path: row.get(6)?,
                sha256_hash: row.get(7)?,
            })
        })?
        .filter_map(|row| row.ok())
        .collect();
    Ok(rows)
}

pub fn replace_history(conn: &Connection, items: &[HistoryItem]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM download_history", [])?;

    {
        let mut stmt = tx.prepare(
            "INSERT INTO download_history
                 (id, url, title, thumbnail, date, format_id, output_path, sha256_hash, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;

        for item in items {
            stmt.execute(params![
                item.id,
                item.url,
                item.title,
                item.thumbnail,
                item.date,
                item.format_id,
                item.output_path,
                item.sha256_hash,
                now_secs(),
            ])?;
        }
    }

    tx.commit()?;
    Ok(())
}

pub fn clear_history(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM download_history", [])?;
    Ok(())
}

pub fn find_completed_duplicate_by_identity(
    conn: &Connection,
    identity_key: &str,
) -> Result<Option<DuplicateDownload>> {
    Ok(conn.query_row(
        "SELECT id, filename, url, provider, dest_path, status, completed_at, identity_key, expected_hash_json
         FROM downloads
         WHERE identity_key = ?1 AND status = 'complete'
         ORDER BY completed_at DESC, created_at DESC
         LIMIT 1",
        params![identity_key],
        duplicate_download_from_row,
    )
    .optional()?)
}

pub fn find_history_duplicate_by_sha256(
    conn: &Connection,
    sha256_hash: &str,
) -> Result<Option<DuplicateDownload>> {
    Ok(conn.query_row(
        "SELECT id, title, url, output_path, date, sha256_hash
         FROM download_history
         WHERE sha256_hash = ?1 AND sha256_hash IS NOT NULL AND sha256_hash != ''
         ORDER BY date DESC, updated_at DESC
         LIMIT 1",
        params![sha256_hash],
        |row| {
            Ok(DuplicateDownload {
                id: row.get(0)?,
                filename: row.get(1)?,
                url: row.get(2)?,
                provider: "history".to_string(),
                path: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                status: DownloadStatus::Complete,
                completed_at: None,
                identity_key: None,
                sha256_hash: row.get(5)?,
            })
        },
    )
    .optional()?)
}

pub fn load_duplicate_groups(conn: &Connection) -> Result<Vec<DuplicateGroup>> {
    let mut groups = Vec::new();

    let mut stmt = conn.prepare(
        "SELECT identity_key
         FROM downloads
         WHERE identity_key != ''
         GROUP BY identity_key
         HAVING COUNT(*) > 1",
    )?;
    let keys = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|row| row.ok())
        .collect::<Vec<_>>();

    for key in keys {
        let mut items_stmt = conn.prepare(
            "SELECT id, filename, url, provider, dest_path, status, completed_at, identity_key, expected_hash_json
             FROM downloads
             WHERE identity_key = ?1
             ORDER BY completed_at DESC, created_at DESC",
        )?;
        let items = items_stmt
            .query_map(params![key], duplicate_download_from_row)?
            .filter_map(|row| row.ok())
            .collect::<Vec<_>>();
        if items.len() > 1 {
            groups.push(DuplicateGroup {
                kind: "identity_key".to_string(),
                key,
                items,
            });
        }
    }

    let mut history_stmt = conn.prepare(
        "SELECT sha256_hash
         FROM download_history
         WHERE sha256_hash IS NOT NULL AND sha256_hash != ''
         GROUP BY sha256_hash
         HAVING COUNT(*) > 1",
    )?;
    let hash_keys = history_stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|row| row.ok())
        .collect::<Vec<_>>();

    for key in hash_keys {
        let mut items_stmt = conn.prepare(
            "SELECT id, title, url, output_path, date, sha256_hash
             FROM download_history
             WHERE sha256_hash = ?1
             ORDER BY date DESC, updated_at DESC",
        )?;
        let items = items_stmt
            .query_map(params![key], |row| {
                Ok(DuplicateDownload {
                    id: row.get(0)?,
                    filename: row.get(1)?,
                    url: row.get(2)?,
                    provider: "history".to_string(),
                    path: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    status: DownloadStatus::Complete,
                    completed_at: None,
                    identity_key: None,
                    sha256_hash: row.get(5)?,
                })
            })?
            .filter_map(|row| row.ok())
            .collect::<Vec<_>>();
        if items.len() > 1 {
            groups.push(DuplicateGroup {
                kind: "sha256".to_string(),
                key,
                items,
            });
        }
    }

    Ok(groups)
}

fn duplicate_download_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DuplicateDownload> {
    let expected_hash_json: Option<String> = row.get(8)?;
    let sha256_hash = expected_hash_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Option<crate::models::ExpectedHash>>(raw).ok())
        .flatten()
        .and_then(|hash| {
            if matches!(hash.algorithm, crate::models::HashAlgorithm::Sha256) {
                Some(hash.value)
            } else {
                None
            }
        });

    Ok(DuplicateDownload {
        id: row.get(0)?,
        filename: row.get(1)?,
        url: row.get(2)?,
        provider: row.get(3)?,
        path: row.get(4)?,
        status: parse_status(&row.get::<_, String>(5)?),
        completed_at: row.get::<_, Option<i64>>(6)?.map(|value| value as u64),
        identity_key: row.get(7)?,
        sha256_hash,
    })
}

#[derive(Debug, Clone)]
pub struct DirectHttpPartState {
    pub part_index: usize,
    pub bytes_downloaded: u64,
}

pub fn load_direct_http_parts(
    conn: &Connection,
    download_key: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<Vec<DirectHttpPartState>> {
    let mut stmt = conn.prepare(
        "SELECT part_index, bytes_downloaded, etag, last_modified
         FROM direct_http_parts
         WHERE download_key = ?1
         ORDER BY part_index ASC",
    )?;
    let rows = stmt
        .query_map(params![download_key], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
        .filter_map(|row| row.ok())
        .collect::<Vec<_>>();

    if rows.iter().any(|(_, _, stored_etag, stored_last_modified)| {
        stored_etag.as_deref() != etag || stored_last_modified.as_deref() != last_modified
    }) {
        clear_direct_http_parts(conn, download_key)?;
        return Ok(Vec::new());
    }

    Ok(rows
        .into_iter()
        .filter_map(|(part_index, bytes_downloaded, _, _)| {
            Some(DirectHttpPartState {
                part_index: usize::try_from(part_index).ok()?,
                bytes_downloaded: u64::try_from(bytes_downloaded).ok()?,
            })
        })
        .collect())
}

pub fn save_direct_http_part(
    conn: &Connection,
    download_key: &str,
    part_index: usize,
    url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
    start_byte: u64,
    end_byte: u64,
    bytes_downloaded: u64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO direct_http_parts
             (download_key, part_index, url, etag, last_modified, start_byte,
              end_byte, bytes_downloaded, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(download_key, part_index) DO UPDATE SET
             url              = excluded.url,
             etag             = excluded.etag,
             last_modified    = excluded.last_modified,
             start_byte       = excluded.start_byte,
             end_byte         = excluded.end_byte,
             bytes_downloaded = excluded.bytes_downloaded,
             updated_at       = excluded.updated_at",
        params![
            download_key,
            part_index as i64,
            url,
            etag,
            last_modified,
            start_byte as i64,
            end_byte as i64,
            bytes_downloaded as i64,
            now_secs(),
        ],
    )?;
    Ok(())
}

pub fn clear_direct_http_parts(conn: &Connection, download_key: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM direct_http_parts WHERE download_key = ?1",
        params![download_key],
    )?;
    Ok(())
}

pub fn load_cached_file_info(conn: &Connection, url: &str) -> Result<Option<CachedFileInfo>> {
    conn.query_row(
        "SELECT url, provider_id, name, size, mime_type, is_folder, children_json, cached_at, last_checked_at
         FROM file_info_cache
         WHERE url = ?1",
        params![url],
        |row| {
            let children_json: Option<String> = row.get(6)?;
            Ok(CachedFileInfo {
                url: row.get(0)?,
                provider_id: row.get(1)?,
                name: row.get(2)?,
                size: row.get::<_, i64>(3)? as u64,
                mime_type: row.get(4)?,
                is_folder: row.get::<_, i64>(5)? != 0,
                children: children_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Option<Vec<FileChildInfo>>>(raw).ok())
                    .flatten(),
                cached_at: row.get::<_, i64>(7)? as u64,
                last_checked_at: row.get::<_, Option<i64>>(8)?.map(|value| value as u64),
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub fn save_cached_file_info(
    conn: &Connection,
    url: &str,
    provider_id: &str,
    name: &str,
    size: u64,
    mime_type: Option<&str>,
    is_folder: bool,
    children: &Option<Vec<FileChildInfo>>,
) -> Result<()> {
    let now = now_secs();
    conn.execute(
        "INSERT INTO file_info_cache
             (url, provider_id, name, size, mime_type, is_folder, children_json, cached_at, last_checked_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
         ON CONFLICT(url) DO UPDATE SET
             provider_id = excluded.provider_id,
             name = excluded.name,
             size = excluded.size,
             mime_type = excluded.mime_type,
             is_folder = excluded.is_folder,
             children_json = excluded.children_json,
             cached_at = excluded.cached_at,
             last_checked_at = excluded.last_checked_at",
        params![
            url,
            provider_id,
            name,
            size as i64,
            mime_type,
            if is_folder { 1 } else { 0 },
            to_json(children),
            now,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Download, DownloadStatus, FileChildInfo};

    fn temp_db_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("gdownloader-db-test-{name}-{}.sqlite", uuid::Uuid::new_v4()))
    }

    fn sample_download(status: DownloadStatus) -> Download {
        Download {
            id: "download-1".to_string(),
            url: "https://example.com/file".to_string(),
            provider: "Example".to_string(),
            identity_key: "example::https://example.com/file".to_string(),
            filename: "file.bin".to_string(),
            size: 1234,
            dest_path: "/tmp/file.bin".to_string(),
            status,
            bytes_downloaded: 120,
            speed_bps: 0,
            eta_secs: 0,
            is_folder: true,
            children: Some(vec![FileChildInfo {
                filename: "child.bin".to_string(),
                size: 120,
                mime_type: Some("application/octet-stream".to_string()),
                is_folder: false,
                path: Some("/child.bin".to_string()),
                source_url: Some("https://example.com/child".to_string()),
                bytes_downloaded: Some(12),
                speed_bps: Some(0),
                eta_secs: Some(0),
                status: Some(DownloadStatus::WaitingCaptcha),
            }]),
            retry_count: 1,
            max_retries: 3,
            speed_limit_kib: 256,
            parallel_parts: 2,
            selected_children: Some(vec!["https://example.com/child".to_string()]),
            expected_hash: None,
            retry_at: Some(999),
            captcha_type: Some("recaptcha2".to_string()),
            captcha_sitekey: Some("sitekey".to_string()),
            captcha_page_url: Some("https://example.com/captcha".to_string()),
            captcha_token: Some("token".to_string()),
            error: Some("erro".to_string()),
            priority: 5,
            created_at: 111,
            started_at: Some(222),
            completed_at: None,
            last_progress_at: Some(333),
        }
    }

    #[test]
    fn migrations_create_expected_schema_and_indexes() {
        let path = temp_db_path("migrations");
        let conn = init(path.to_string_lossy().as_ref()).unwrap();

        assert!(column_exists(&conn, "downloads", "identity_key").unwrap());
        assert!(column_exists(&conn, "downloads", "priority").unwrap());
        assert!(column_exists(&conn, "downloads", "started_at").unwrap());
        assert!(column_exists(&conn, "downloads", "completed_at").unwrap());
        assert!(column_exists(&conn, "downloads", "last_progress_at").unwrap());

        let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_downloads_status_created_at'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(index_count, 1);

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn persists_complex_download_states_and_clears_terminal_entries() {
        let path = temp_db_path("persistence");
        let conn = init(path.to_string_lossy().as_ref()).unwrap();

        let waiting = sample_download(DownloadStatus::WaitingCaptcha);
        upsert(&conn, &waiting).unwrap();

        let mut rate_limited = sample_download(DownloadStatus::RateLimited);
        rate_limited.id = "download-2".to_string();
        rate_limited.identity_key = "example::https://example.com/file-2".to_string();
        rate_limited.retry_at = Some(4444);
        upsert(&conn, &rate_limited).unwrap();

        let mut complete = sample_download(DownloadStatus::Complete);
        complete.id = "download-3".to_string();
        complete.identity_key = "example::https://example.com/file-3".to_string();
        complete.completed_at = Some(5555);
        upsert(&conn, &complete).unwrap();

        let loaded = load_all_downloads(&conn).unwrap();
        assert_eq!(loaded.len(), 3);
        assert!(loaded.iter().any(|item| item.status == DownloadStatus::WaitingCaptcha && item.children.as_ref().is_some_and(|children| !children.is_empty())));
        assert!(loaded.iter().any(|item| item.status == DownloadStatus::RateLimited && item.retry_at == Some(4444)));
        assert!(loaded.iter().any(|item| item.status == DownloadStatus::Complete && item.completed_at == Some(5555)));

        delete_finished(&conn).unwrap();
        let loaded_after_clear = load_all_downloads(&conn).unwrap();
        assert_eq!(loaded_after_clear.len(), 2);
        assert!(loaded_after_clear.iter().all(|item| item.status != DownloadStatus::Complete));

        std::fs::remove_file(path).ok();
    }
}
