use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex as StdMutex, OnceLock};
use sysinfo::Disks;

/// Taxas de I/O por ponto de montagem (leitura, escrita) em bytes/s, atualizadas
/// uma vez por segundo pelo sampler. Chave = mount_point.
static DISK_IO: OnceLock<StdMutex<HashMap<String, (u64, u64)>>> = OnceLock::new();

fn disk_io_store() -> &'static StdMutex<HashMap<String, (u64, u64)>> {
    DISK_IO.get_or_init(|| StdMutex::new(HashMap::new()))
}

/// Inicia (uma vez) o amostrador de I/O de disco: uma thread dedicada que reamostra
/// o sysinfo a cada segundo e grava as taxas de leitura/escrita por montagem. Usa
/// thread própria (não tokio) porque o refresh é bloqueante e usa IOKit no macOS.
pub fn spawn_disk_io_sampler() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return; // já iniciado
    }
    std::thread::Builder::new()
        .name("disk-io-sampler".into())
        .spawn(|| {
            let mut disks = Disks::new_with_refreshed_list();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                // Reamostra sem remover discos que sumiram temporariamente.
                disks.refresh(false);
                let mut rates: HashMap<String, (u64, u64)> = HashMap::new();
                for disk in disks.iter() {
                    let mount = disk.mount_point().to_string_lossy().to_string();
                    let usage = disk.usage();
                    // `read_bytes`/`written_bytes` = delta desde o refresh anterior
                    // (intervalo de ~1s), logo já é a taxa por segundo.
                    let entry = rates.entry(mount).or_insert((0, 0));
                    entry.0 = entry.0.saturating_add(usage.read_bytes);
                    entry.1 = entry.1.saturating_add(usage.written_bytes);
                }
                if let Ok(mut store) = disk_io_store().lock() {
                    *store = rates;
                }
            }
        })
        .ok();
}

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

#[derive(Debug, Serialize)]
pub struct DiskEntry {
    pub name: String,
    pub mount: String,
    pub total: u64,
    pub available: u64,
    pub used: u64,
    pub removable: bool,
    pub kind: String,
    /// Taxa de I/O ao vivo (bytes/s) do disco, amostrada a cada segundo.
    #[serde(rename = "readBps")]
    pub read_bps: u64,
    #[serde(rename = "writeBps")]
    pub write_bps: u64,
}

/// Lista todos os discos/volumes montados (HD, SSD, pendrive…) com sua alocação.
/// Alimenta o balão do widget de disco (multi-disco).
pub async fn list_disks() -> Json<Vec<DiskEntry>> {
    let disks = Disks::new_with_refreshed_list();
    let io_rates = disk_io_store().lock().map(|m| m.clone()).unwrap_or_default();
    let mut seen_mount = std::collections::HashSet::new();
    let mut seen_volume = std::collections::HashSet::new();
    let mut entries = Vec::new();

    for disk in disks.iter() {
        let mount = disk.mount_point().to_string_lossy().to_string();
        let total = disk.total_space();
        if total == 0 || !seen_mount.insert(mount.clone()) {
            continue;
        }
        // Filtra volumes internos irrelevantes do macOS. No APFS, `/` e
        // `/System/Volumes/Data` são o mesmo volume físico — ignoramos o /Data.
        if mount.starts_with("/System/Volumes/") {
            continue;
        }
        if mount.starts_with("/private/") || mount == "/dev" {
            continue;
        }

        // Disponível via statvfs (preciso); cai para o sysinfo se falhar.
        let available = statvfs_usage(&mount)
            .map(|(_, avail)| avail)
            .unwrap_or_else(|| disk.available_space());

        // Dedupe por assinatura do volume (mesmo disco montado em 2 pontos).
        let name = disk.name().to_string_lossy().to_string();
        if !seen_volume.insert(format!("{name}|{total}|{available}")) {
            continue;
        }
        let name = {
            let raw = disk.name().to_string_lossy().to_string();
            if raw.trim().is_empty() { mount.clone() } else { raw }
        };
        let kind = match disk.kind() {
            sysinfo::DiskKind::SSD => "SSD",
            sysinfo::DiskKind::HDD => "HDD",
            _ => "Desconhecido",
        };
        let (read_bps, write_bps) = io_rates.get(&mount).copied().unwrap_or((0, 0));
        entries.push(DiskEntry {
            name,
            mount,
            total,
            available,
            used: total.saturating_sub(available),
            removable: disk.is_removable(),
            kind: kind.to_string(),
            read_bps,
            write_bps,
        });
    }

    entries.sort_by(|a, b| b.total.cmp(&a.total));
    Json(entries)
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
