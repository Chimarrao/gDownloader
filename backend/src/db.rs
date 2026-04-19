use anyhow::Result;
use rusqlite::{params, Connection};

use crate::models::{Download, DownloadStatus};

/// Inicializa o banco SQLite e cria a tabela de downloads se não existir.
pub fn init(db_path: &str) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS downloads (
             id                TEXT PRIMARY KEY,
             url               TEXT NOT NULL,
             provider          TEXT NOT NULL DEFAULT '',
             filename          TEXT NOT NULL DEFAULT '',
             dest_path         TEXT NOT NULL DEFAULT '',
             size              INTEGER NOT NULL DEFAULT 0,
             bytes_downloaded  INTEGER NOT NULL DEFAULT 0,
             status            TEXT NOT NULL DEFAULT 'pending',
             error             TEXT,
             retry_count       INTEGER NOT NULL DEFAULT 0,
             retry_at          INTEGER,
             created_at        INTEGER NOT NULL,
             updated_at        INTEGER NOT NULL
         );",
    )?;
    Ok(conn)
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
        DownloadStatus::Paused => "paused",
        DownloadStatus::Complete => "complete",
        DownloadStatus::Error => "error",
        DownloadStatus::Cancelled => "cancelled",
        DownloadStatus::RateLimited => "rate_limited",
        DownloadStatus::WaitingCaptcha => "waiting_captcha",
    }
}

/// Insere ou atualiza um download no banco.
pub fn upsert(conn: &Connection, d: &Download) -> Result<()> {
    conn.execute(
        "INSERT INTO downloads
             (id, url, provider, filename, dest_path, size, bytes_downloaded,
              status, error, retry_count, retry_at, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
         ON CONFLICT(id) DO UPDATE SET
             bytes_downloaded = excluded.bytes_downloaded,
             status           = excluded.status,
             error            = excluded.error,
             retry_count      = excluded.retry_count,
             retry_at         = excluded.retry_at,
             updated_at       = excluded.updated_at",
        params![
            d.id,
            d.url,
            d.provider,
            d.filename,
            d.dest_path,
            d.size as i64,
            d.bytes_downloaded as i64,
            status_str(&d.status),
            d.error,
            d.retry_count as i64,
            d.retry_at.map(|x| x as i64),
            d.created_at as i64,
            now_secs(),
        ],
    )?;
    Ok(())
}

/// Atualiza só o progresso (bytes) — chamado a cada 5 segundos durante o download.
pub fn update_progress(conn: &Connection, id: &str, bytes: u64) -> Result<()> {
    conn.execute(
        "UPDATE downloads SET bytes_downloaded=?1, updated_at=?2 WHERE id=?3",
        params![bytes as i64, now_secs(), id],
    )?;
    Ok(())
}

/// Atualiza status e mensagem de erro.
pub fn update_status(conn: &Connection, id: &str, status: &str, error: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE downloads SET status=?1, error=?2, updated_at=?3 WHERE id=?4",
        params![status, error, now_secs(), id],
    )?;
    Ok(())
}

/// Marca como rate_limited e salva o timestamp de retry.
pub fn update_retry_at(conn: &Connection, id: &str, retry_at: u64) -> Result<()> {
    conn.execute(
        "UPDATE downloads SET status='rate_limited', retry_at=?1, updated_at=?2 WHERE id=?3",
        params![retry_at as i64, now_secs(), id],
    )?;
    Ok(())
}

/// Remove um download do banco (usado no remove_download).
pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM downloads WHERE id=?1", params![id])?;
    Ok(())
}

/// Remove todos os downloads com status complete ou cancelled.
pub fn delete_finished(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM downloads WHERE status IN ('complete','cancelled')",
        [],
    )?;
    Ok(())
}

/// Dados mínimos para restaurar um download após restart.
#[derive(Debug)]
pub struct ResumeRow {
    pub id: String,
    pub url: String,
    pub provider: String,
    pub filename: String,
    pub dest_path: String,
    pub size: u64,
    pub bytes_downloaded: u64,
    pub retry_count: u32,
    pub created_at: u64,
}

/// Retorna downloads que estavam em andamento ou pausados ao encerrar.
pub fn load_resumable(conn: &Connection) -> Result<Vec<ResumeRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, provider, filename, dest_path, size, bytes_downloaded, retry_count, created_at
         FROM downloads
         WHERE status IN ('downloading','paused','rate_limited')",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ResumeRow {
                id: row.get(0)?,
                url: row.get(1)?,
                provider: row.get(2)?,
                filename: row.get(3)?,
                dest_path: row.get(4)?,
                size: row.get::<_, i64>(5)? as u64,
                bytes_downloaded: row.get::<_, i64>(6)? as u64,
                retry_count: row.get::<_, i64>(7)? as u32,
                created_at: row.get::<_, i64>(8)? as u64,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}
