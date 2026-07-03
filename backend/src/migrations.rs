use anyhow::Result;
use rusqlite::Connection;

pub(crate) struct Migration {
    pub(crate) version: i64,
    pub(crate) name: &'static str,
    pub(crate) apply: fn(&Connection) -> Result<()>,
}

pub(crate) const MIGRATIONS: &[Migration] = &[
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
    Migration {
        version: 13,
        name: "create_history_fts",
        apply: migration_create_history_fts,
    },
    Migration {
        version: 14,
        name: "create_intercept_history_table",
        apply: migration_create_intercept_history_table,
    },
    Migration {
        version: 15,
        name: "add_download_network_route",
        apply: migration_add_download_network_route,
    },
    Migration {
        version: 16,
        name: "add_file_cache_media_columns",
        apply: migration_add_file_cache_media_columns,
    },
    Migration {
        version: 17,
        name: "add_download_auto_tor_on_limit",
        apply: migration_add_download_auto_tor_on_limit,
    },
];

pub(crate) fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
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
             network_route_json     TEXT,
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
        (
            "network_route_json",
            "ALTER TABLE downloads ADD COLUMN network_route_json TEXT",
        ),
    ];

    for (column, ddl) in columns {
        add_column_if_missing(conn, "downloads", column, ddl)?;
    }

    Ok(())
}

fn migration_add_download_network_route(conn: &Connection) -> Result<()> {
    add_column_if_missing(
        conn,
        "downloads",
        "network_route_json",
        "ALTER TABLE downloads ADD COLUMN network_route_json TEXT",
    )
}

fn migration_add_download_auto_tor_on_limit(conn: &Connection) -> Result<()> {
    add_column_if_missing(
        conn,
        "downloads",
        "auto_tor_on_limit",
        "ALTER TABLE downloads ADD COLUMN auto_tor_on_limit INTEGER NOT NULL DEFAULT 0",
    )
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

fn migration_add_file_cache_media_columns(conn: &Connection) -> Result<()> {
    add_column_if_missing(
        conn,
        "file_info_cache",
        "thumbnail_url",
        "ALTER TABLE file_info_cache ADD COLUMN thumbnail_url TEXT",
    )?;
    add_column_if_missing(
        conn,
        "file_info_cache",
        "channel_name",
        "ALTER TABLE file_info_cache ADD COLUMN channel_name TEXT",
    )?;
    add_column_if_missing(
        conn,
        "file_info_cache",
        "channel_thumbnail_url",
        "ALTER TABLE file_info_cache ADD COLUMN channel_thumbnail_url TEXT",
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

fn migration_create_history_fts(conn: &Connection) -> Result<()> {
    add_column_if_missing(
        conn,
        "download_history",
        "host",
        "ALTER TABLE download_history ADD COLUMN host TEXT NOT NULL DEFAULT ''",
    )?;
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS download_history_ai;
         DROP TRIGGER IF EXISTS download_history_ad;
         DROP TRIGGER IF EXISTS download_history_au;
         DROP TABLE IF EXISTS history_fts;
         CREATE INDEX IF NOT EXISTS idx_download_history_host
             ON download_history(host);
         CREATE VIRTUAL TABLE IF NOT EXISTS history_fts
             USING fts5(filename, url, host);
         CREATE TRIGGER IF NOT EXISTS download_history_ai AFTER INSERT ON download_history BEGIN
             INSERT INTO history_fts(rowid, filename, url, host)
             VALUES (new.rowid, new.title, new.url, new.host);
         END;
         CREATE TRIGGER IF NOT EXISTS download_history_ad AFTER DELETE ON download_history BEGIN
             DELETE FROM history_fts WHERE rowid = old.rowid;
         END;
         CREATE TRIGGER IF NOT EXISTS download_history_au AFTER UPDATE ON download_history BEGIN
             DELETE FROM history_fts WHERE rowid = old.rowid;
             INSERT INTO history_fts(rowid, filename, url, host)
             VALUES (new.rowid, new.title, new.url, new.host);
         END;",
    )?;
    Ok(())
}

fn migration_create_intercept_history_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS intercept_history (
             id         TEXT PRIMARY KEY,
             url        TEXT NOT NULL DEFAULT '',
             filename   TEXT NOT NULL DEFAULT '',
             mime_type  TEXT NOT NULL DEFAULT '',
             size       INTEGER NOT NULL DEFAULT 0,
             status     TEXT NOT NULL DEFAULT 'queued',
             created_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_intercept_history_created
             ON intercept_history(created_at DESC);",
    )?;
    Ok(())
}
