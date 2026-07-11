use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};
use sysinfo::Disks;

#[derive(Debug, Deserialize)]
pub struct DiskQuery {
    /// Caminho (pasta de download) cujo volume será medido. Se ausente, usa o maior
    /// disco disponível (provável volume principal).
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DiskUsage {
    pub total: u64,
    pub available: u64,
    pub used: u64,
    pub mount: String,
}

/// Retorna espaço total/usado/livre do volume que contém `path`.
/// Alimenta o widget de disco no topo da tela (informativo, item 11).
pub async fn disk_usage(Query(query): Query<DiskQuery>) -> Json<DiskUsage> {
    let disks = Disks::new_with_refreshed_list();
    let target = query.path.as_deref().map(std::path::PathBuf::from);

    // Escolhe o disco cujo ponto de montagem é o prefixo mais longo do caminho alvo
    // (ex.: /Volumes/Externo vence / quando o download vai para o disco externo).
    let best = target
        .as_ref()
        .and_then(|path| {
            disks
                .iter()
                .filter(|disk| path.starts_with(disk.mount_point()))
                .max_by_key(|disk| disk.mount_point().as_os_str().len())
        })
        .or_else(|| disks.iter().max_by_key(|disk| disk.total_space()));

    let (total, available, mount) = best
        .map(|disk| {
            (
                disk.total_space(),
                disk.available_space(),
                disk.mount_point().to_string_lossy().to_string(),
            )
        })
        .unwrap_or((0, 0, String::new()));

    Json(DiskUsage {
        total,
        available,
        used: total.saturating_sub(available),
        mount,
    })
}
