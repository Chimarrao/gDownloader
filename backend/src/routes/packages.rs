use axum::{extract::{Path, State}, http::StatusCode, Json};
use uuid::Uuid;

use crate::{
    models::{ApiError, CreatePackageRequest, Package},
    ws::AppState,
};

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub async fn list_packages(
    State(state): State<AppState>,
) -> Json<Vec<Package>> {
    let packages = (|| -> anyhow::Result<Vec<Package>> {
        let db = state.db.lock().map_err(|_| anyhow::anyhow!("lock failed"))?;
        let mut stmt = db.prepare(
            "SELECT id, name, color, comment, dest_dir_override, priority, created_at
             FROM packages ORDER BY priority DESC, created_at DESC"
        )?;
        let packages = stmt.query_map([], |row| {
            Ok(Package {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                comment: row.get(3)?,
                dest_dir_override: row.get(4)?,
                priority: row.get(5)?,
                created_at: row.get::<_, i64>(6)? as u64,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
        Ok(packages)
    })().unwrap_or_default();
    Json(packages)
}

pub async fn create_package(
    State(state): State<AppState>,
    Json(req): Json<CreatePackageRequest>,
) -> Result<Json<Package>, (StatusCode, Json<ApiError>)> {
    let id = Uuid::new_v4().to_string();
    let now = now_secs();
    let color = req.color.clone().unwrap_or_else(|| "#7c6fff".to_string());
    let priority = req.priority.unwrap_or(0);

    {
        let db = state.db.lock().map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("DB lock failed")))
        })?;
        db.execute(
            "INSERT INTO packages (id, name, color, comment, dest_dir_override, priority, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, req.name, color, req.comment, req.dest_dir_override, priority, now as i64],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(e.to_string()))))?;
    }

    Ok(Json(Package {
        id,
        name: req.name,
        color,
        comment: req.comment,
        dest_dir_override: req.dest_dir_override,
        priority,
        created_at: now,
    }))
}

pub async fn delete_package(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    {
        let db = state.db.lock().map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("DB lock failed")))
        })?;
        let _ = db.execute("UPDATE downloads SET package_id = NULL WHERE package_id = ?1", rusqlite::params![id]);
        db.execute("DELETE FROM packages WHERE id = ?1", rusqlite::params![id])
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(e.to_string()))))?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn assign_download_to_package(
    State(state): State<AppState>,
    Path((package_id, download_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    {
        let db = state.db.lock().map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("DB lock failed")))
        })?;
        db.execute(
            "UPDATE downloads SET package_id = ?1 WHERE id = ?2",
            rusqlite::params![package_id, download_id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(e.to_string()))))?;
    }

    let mut map = state.downloads.lock().await;
    if let Some(dl) = map.get_mut(&download_id) {
        dl.package_id = Some(package_id);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unassign_download_from_package(
    State(state): State<AppState>,
    Path(download_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    {
        let db = state.db.lock().map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("DB lock failed")))
        })?;
        db.execute(
            "UPDATE downloads SET package_id = NULL WHERE id = ?1",
            rusqlite::params![download_id],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new(e.to_string()))))?;
    }

    let mut map = state.downloads.lock().await;
    if let Some(dl) = map.get_mut(&download_id) {
        dl.package_id = None;
    }
    Ok(StatusCode::NO_CONTENT)
}
