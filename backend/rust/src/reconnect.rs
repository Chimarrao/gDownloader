use anyhow::Result;
use tokio::time::{sleep, Duration, Instant};
use tracing::{info, warn};

pub struct ReconnectConfig {
    pub method: String,   // "none" | "router_script" | "curl_command"
    pub command: String,  // command with optional {router_ip} placeholder
    pub router_ip: String,
}

/// Try to get a new IP by running the user-configured reconnect command.
/// Returns Ok(true) if reconnect succeeded and internet is back, Ok(false) on skip/failure.
pub async fn attempt_reconnect(config: &ReconnectConfig) -> Result<bool> {
    if config.method == "none" || config.command.is_empty() {
        return Ok(false);
    }

    let cmd = config.command.replace("{router_ip}", &config.router_ip);
    info!(target: "gdownloader_backend::reconnect", "Executing reconnect command: {}", cmd);

    // Split into program + args (simple shell-style split)
    let parts: Vec<String> = shlex_split(&cmd);
    if parts.is_empty() {
        return Ok(false);
    }

    let status = tokio::process::Command::new(&parts[0])
        .args(&parts[1..])
        .status()
        .await;

    match status {
        Ok(s) if !s.success() => {
            warn!(target: "gdownloader_backend::reconnect", "Reconnect command exited with {:?}", s.code());
            return Ok(false);
        }
        Err(e) => {
            warn!(target: "gdownloader_backend::reconnect", "Failed to run reconnect command: {}", e);
            return Ok(false);
        }
        _ => {}
    }

    // Wait up to 15s for internet to drop (optional — some methods are instant)
    let drop_deadline = Instant::now();
    loop {
        if drop_deadline.elapsed() > Duration::from_secs(15) {
            break; // didn't drop, proceed anyway
        }
        if !internet_up().await {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    info!(target: "gdownloader_backend::reconnect", "Waiting for internet to come back...");

    // Wait up to 90s for internet to return
    let reconnect_deadline = Instant::now();
    loop {
        if reconnect_deadline.elapsed() > Duration::from_secs(90) {
            warn!(target: "gdownloader_backend::reconnect", "Internet did not return after 90s");
            return Ok(false);
        }
        if internet_up().await {
            break;
        }
        sleep(Duration::from_secs(1)).await;
    }

    info!(target: "gdownloader_backend::reconnect", "Internet is back, reconnect succeeded");
    Ok(true)
}

/// Returns true if we can reach 8.8.8.8
async fn internet_up() -> bool {
    #[cfg(target_os = "windows")]
    let args = ["-n", "1", "-w", "1000", "8.8.8.8"];
    #[cfg(not(target_os = "windows"))]
    let args = ["-c", "1", "-W", "1", "8.8.8.8"];

    tokio::process::Command::new("ping")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

fn shlex_split(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = ' ';

    for ch in s.chars() {
        match ch {
            '"' | '\'' if !in_quote => {
                in_quote = true;
                quote_char = ch;
            }
            c if in_quote && c == quote_char => {
                in_quote = false;
            }
            ' ' if !in_quote => {
                if !current.is_empty() {
                    parts.push(current.drain(..).collect());
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}
