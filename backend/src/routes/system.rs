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
    // Caminho alvo (pasta de download). Se ausente, usa o home do usuário.
    let target = query
        .path
        .filter(|path| !path.trim().is_empty())
        .or_else(|| std::env::var("HOME").ok())
        .unwrap_or_else(|| "/".to_string());

    if let Some((total, available)) = statvfs_usage(&target) {
        return Json(DiskUsage {
            total,
            available,
            used: total.saturating_sub(available),
            mount: mount_point_for(&target),
        });
    }

    // Fallback (Windows ou falha do statvfs): sysinfo por ponto de montagem.
    let disks = Disks::new_with_refreshed_list();
    let target_path = std::path::PathBuf::from(&target);
    let best = disks
        .iter()
        .filter(|disk| target_path.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
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

/// Espaço (total, disponível) em bytes do sistema de arquivos que contém `path`,
/// via `statvfs` — o mesmo mecanismo do `df`, então resolve firmlinks do macOS
/// corretamente (ao contrário de casar ponto de montagem). `None` fora de unix.
#[cfg(unix)]
fn statvfs_usage(path: &str) -> Option<(u64, u64)> {
    use std::ffi::CString;
    let c_path = CString::new(path).ok()?;
    // SAFETY: statvfs preenche a struct zerada; só lemos campos numéricos.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) != 0 {
            return None;
        }
        let block_size = if stat.f_frsize != 0 {
            stat.f_frsize as u64
        } else {
            stat.f_bsize as u64
        };
        let total = (stat.f_blocks as u64).saturating_mul(block_size);
        let available = (stat.f_bavail as u64).saturating_mul(block_size);
        Some((total, available))
    }
}

#[cfg(not(unix))]
fn statvfs_usage(_path: &str) -> Option<(u64, u64)> {
    None
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn statvfs_reports_plausible_usage_for_home() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let (total, available) = statvfs_usage(&home).expect("statvfs deve funcionar no home");
        assert!(total > 0, "total deve ser > 0");
        assert!(available > 0, "disponível deve ser > 0");
        assert!(available <= total, "disponível não pode exceder o total");
        // Ajuda a depurar visualmente (cargo test -- --nocapture).
        eprintln!(
            "disco {home}: total={}GB disponível={}GB",
            total / 1_000_000_000,
            available / 1_000_000_000
        );
    }
}

/// Ponto de montagem legível do volume que contém `path` (best-effort, informativo).
fn mount_point_for(path: &str) -> String {
    let target = std::path::PathBuf::from(path);
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .filter(|disk| target.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| disk.mount_point().to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string())
}
