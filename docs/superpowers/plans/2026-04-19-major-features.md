# Major Features Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement 9 major features: settings persistence (accentColor), Terabox BrowserWindow login, SQLite download persistence with resume, 1Fichier folder support + rate limit, smart retry timing, inline captcha system with NoPecha, Rapidgator provider, and README update.

**Architecture:** Backend adds SQLite via rusqlite for download state persistence and startup recovery. New error types (`RateLimit`, `CaptchaRequired`) flow from providers through the API to the frontend. Captcha is shown inline in the download card using an iframe served by the local Rust backend. Terabox login uses a child BrowserWindow with CSS injection instead of brittle RSA flow.

**Tech Stack:** Rust/rusqlite (SQLite, bundled), Electron BrowserWindow (Terabox login), iframe + local HTML endpoint (inline captcha), NoPecha REST API (captcha auto-solve), reqwest, Vue 3, Electron 39

---

## File Map

**Create:**
- `backend/src/db.rs` — SQLite init, CRUD for downloads
- `backend/src/providers/rapidgator.rs` — Rapidgator provider
- `backend/src/providers/1fichier.rs` — renamed from fichier.rs (keep module name `fichier`)

**Modify:**
- `backend/Cargo.toml` — add rusqlite bundled
- `backend/src/lib.rs` — add db to AppState, accept db_path arg
- `backend/src/main.rs` — parse db_path CLI arg, pass to create_app
- `backend/src/ws.rs` — add `db: Arc<Mutex<Connection>>` to AppState
- `backend/src/models.rs` — add `WaitingCaptcha`, `RateLimited` status; add `retry_at`, `captcha_info` fields
- `backend/src/providers/mod.rs` — add `ProviderError::RateLimit`, `ProviderError::CaptchaRequired`; update mod for 1fichier; add rapidgator
- `backend/src/providers/fichier.rs` → deleted (replaced by `1fichier.rs`)
- `backend/src/routes/downloads.rs` — write to DB on status changes, load from DB on startup, serve captcha iframe HTML
- `backend/src/routes/captcha.rs` — new: serve captcha widget HTML page
- `src/shared/types.ts` — add `accentColor`, `nopechaApiKey` to PersistedSettings; add new statuses
- `src/main/index.ts` — pass db_path to backend; replace Terabox RSA with BrowserWindow; add captcha:solve IPC
- `src/preload/index.ts` — expose captcha API
- `src/renderer/src/components/AppSettings.vue` — add accentColor picker, nopechaApiKey field
- `src/renderer/src/components/DownloadList.vue` — add captcha iframe inline, countdown timer
- `src/renderer/src/App.vue` — apply accentColor CSS var on load
- `README.md` — update

---

## Task 1: Commit all pending changes

**Files:** all staged/modified files

- [ ] **Step 1: Stage and commit everything current**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
git add -A
git commit -m "feat: add providers (terabox, 1fichier, sharepoint, drime) and UI improvements

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 2: Settings — accentColor persistence

**Files:**
- Modify: `src/shared/types.ts`
- Modify: `src/renderer/src/components/AppSettings.vue`
- Modify: `src/renderer/src/App.vue`

- [ ] **Step 1: Add `accentColor` to PersistedSettings in `src/shared/types.ts`**

Find the `PersistedSettings` interface and add after `uiZoom`:
```typescript
  accentColor?: string  // hex color, e.g. "#a855f7". undefined = use theme default
```

- [ ] **Step 2: Add color picker row to AppSettings.vue**

In the `<!-- Appearance section -->`, after the theme `<select>` row, add:
```html
<div class="setting-row">
  <div class="setting-info">
    <span class="setting-label">Cor de destaque</span>
    <span class="setting-desc">Personaliza a cor principal do app</span>
  </div>
  <div style="display:flex;gap:8px;align-items:center;">
    <input
      type="color"
      v-model="settings.accentColor"
      style="width:40px;height:32px;border:none;background:none;cursor:pointer;padding:0;"
      @change="onAccentColorChange"
    />
    <button
      v-if="settings.accentColor"
      class="browse-btn"
      style="padding:4px 10px;font-size:12px;"
      @click="resetAccentColor"
    >Resetar</button>
  </div>
</div>
```

- [ ] **Step 3: Add `accentColor` to the reactive settings object and handlers in AppSettings.vue**

In the `interface AppSettings` block, add:
```typescript
  accentColor?: string
```

In the reactive `settings` initialization, add:
```typescript
  accentColor: undefined,
```

After `onLocaleChange`, add:
```typescript
function onAccentColorChange(): void {
  applyAccentColor(settings.accentColor)
  void save()
}

function resetAccentColor(): void {
  settings.accentColor = undefined
  applyAccentColor(undefined)
  void save()
}

function applyAccentColor(color: string | undefined): void {
  const root = document.documentElement
  if (color) {
    root.style.setProperty('--accent-color', color)
  } else {
    root.style.removeProperty('--accent-color')
  }
}
```

Also in `onMounted`, after `Object.assign(settings, saved)`:
```typescript
    if (saved.accentColor) applyAccentColor(saved.accentColor)
```

Add `applyAccentColor` function to imports/scope (it can be local to the script).

- [ ] **Step 4: Apply accent color on app startup in `src/renderer/src/App.vue`**

In the `onMounted` block where settings are loaded, add after applying theme:
```typescript
  if (saved.accentColor) {
    document.documentElement.style.setProperty('--accent-color', saved.accentColor)
  }
```

- [ ] **Step 5: Commit**

```bash
git add src/shared/types.ts src/renderer/src/components/AppSettings.vue src/renderer/src/App.vue
git commit -m "feat: persist accent color in settings.json

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 3: Add rusqlite to backend

**Files:**
- Modify: `backend/Cargo.toml`
- Create: `backend/src/db.rs`

- [ ] **Step 1: Add rusqlite dependency to `backend/Cargo.toml`**

Under `[dependencies]`, add:
```toml
# SQLite embutido para persistência de downloads
rusqlite = { version = "0.31", features = ["bundled"] }
```

- [ ] **Step 2: Create `backend/src/db.rs`**

```rust
use anyhow::Result;
use rusqlite::{Connection, params};
use crate::models::{Download, DownloadStatus};

pub fn init(db_path: &str) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("
        PRAGMA journal_mode=WAL;
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
        );
    ")?;
    Ok(conn)
}

pub fn upsert(conn: &Connection, d: &Download) -> Result<()> {
    let status = serde_json::to_string(&d.status).unwrap_or_default();
    let status = status.trim_matches('"');
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    conn.execute(
        "INSERT INTO downloads
            (id, url, provider, filename, dest_path, size, bytes_downloaded, status, error, retry_count, retry_at, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
         ON CONFLICT(id) DO UPDATE SET
            bytes_downloaded=excluded.bytes_downloaded,
            status=excluded.status,
            error=excluded.error,
            retry_count=excluded.retry_count,
            retry_at=excluded.retry_at,
            updated_at=excluded.updated_at",
        params![
            d.id,
            d.url,
            d.provider,
            d.filename,
            d.dest_path,
            d.size as i64,
            d.bytes_downloaded as i64,
            status,
            d.error,
            d.retry_count as i64,
            d.retry_at.map(|x| x as i64),
            d.created_at as i64,
            now,
        ],
    )?;
    Ok(())
}

pub fn update_progress(conn: &Connection, id: &str, bytes: u64) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    conn.execute(
        "UPDATE downloads SET bytes_downloaded=?1, updated_at=?2 WHERE id=?3",
        params![bytes as i64, now, id],
    )?;
    Ok(())
}

pub fn update_status(conn: &Connection, id: &str, status: &str, error: Option<&str>) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    conn.execute(
        "UPDATE downloads SET status=?1, error=?2, updated_at=?3 WHERE id=?4",
        params![status, error, now, id],
    )?;
    Ok(())
}

pub fn update_retry_at(conn: &Connection, id: &str, retry_at: u64) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    conn.execute(
        "UPDATE downloads SET retry_at=?1, status='rate_limited', updated_at=?2 WHERE id=?3",
        params![retry_at as i64, now, id],
    )?;
    Ok(())
}

pub fn load_resumable(conn: &Connection) -> Result<Vec<ResumeRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, provider, filename, dest_path, size, bytes_downloaded, retry_count, created_at
         FROM downloads
         WHERE status IN ('downloading','paused','rate_limited')"
    )?;
    let rows = stmt.query_map([], |row| {
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

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM downloads WHERE id=?1", params![id])?;
    Ok(())
}

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
```

- [ ] **Step 3: Verify it compiles**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader/backend
cargo check 2>&1 | head -30
```
Expected: errors only about db module not being declared yet (not Cargo errors).

- [ ] **Step 4: Add `mod db;` to `backend/src/lib.rs`**

At the top of lib.rs, add after the existing `mod` declarations:
```rust
pub mod db;
```

- [ ] **Step 5: Verify compile again**

```bash
cargo check 2>&1 | head -30
```
Expected: clean or only warnings.

- [ ] **Step 6: Commit**

```bash
git add backend/Cargo.toml backend/src/db.rs backend/src/lib.rs
git commit -m "feat: add SQLite persistence layer (rusqlite bundled)

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 4: Wire SQLite into AppState and startup

**Files:**
- Modify: `backend/src/ws.rs` (AppState)
- Modify: `backend/src/main.rs`
- Modify: `backend/src/lib.rs`
- Modify: `src/main/index.ts`

- [ ] **Step 1: Add `db` to `AppState` in `backend/src/ws.rs`**

Add import at top:
```rust
use rusqlite::Connection;
use std::sync::Mutex;
```

In the `AppState` struct, add:
```rust
    pub db: Arc<Mutex<Connection>>,
```

In `AppState::new` (or wherever it's constructed), add `db` parameter:
```rust
pub fn new(
    max_concurrent_downloads: usize,
    db: Connection,
) -> Self {
    // ... existing fields ...
    db: Arc::new(Mutex::new(db)),
}
```

- [ ] **Step 2: Update `backend/src/lib.rs` to accept db_path and pass to AppState**

Change `create_app` signature to:
```rust
pub fn create_app(db_path: String) -> Router {
```

Inside `create_app`, before building the router:
```rust
    let db = crate::db::init(&db_path).expect("Failed to open SQLite database");
    let state = AppState::new(3, db);
```

- [ ] **Step 3: Update `backend/src/main.rs` to parse db_path from args**

```rust
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    let args: Vec<String> = std::env::args().collect();
    let db_path = args.get(1).cloned().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{}/.config/gDownloader/downloads.db", home)
    });
    
    // Ensure parent dir exists
    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    
    let app = gdownloader_backend::create_app(db_path);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    println!("PORT:{}", port);
    axum::serve(listener, app).await.unwrap();
}
```

- [ ] **Step 4: Pass db_path from Electron to the backend in `src/main/index.ts`**

Find where the Rust backend is spawned (search for `spawn` or `child_process`). Add the db_path argument:

```typescript
import { app } from 'electron'
import path from 'path'

// In the spawn call, add db_path as first argument:
const dbPath = path.join(app.getPath('userData'), 'downloads.db')
const child = spawn(backendBin, [dbPath], { ... })
```

- [ ] **Step 5: Compile check**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader/backend
cargo build 2>&1 | tail -20
```
Expected: successful build.

- [ ] **Step 6: Commit**

```bash
git add backend/src/ws.rs backend/src/main.rs backend/src/lib.rs src/main/index.ts
git commit -m "feat: wire SQLite into AppState, pass db_path from Electron

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 5: Persist downloads to SQLite on status changes

**Files:**
- Modify: `backend/src/routes/downloads.rs`

- [ ] **Step 1: In `add_download` handler, insert into SQLite after adding to HashMap**

After `downloads.insert(id.clone(), download.clone());`, add:
```rust
if let Ok(db) = state.db.lock() {
    let _ = crate::db::upsert(&db, &download);
}
```

- [ ] **Step 2: In the download task (progress loop), write to SQLite every 5 seconds**

In the async task that runs the download, add a timer:
```rust
let mut last_db_write = std::time::Instant::now();
// ... inside progress loop:
if last_db_write.elapsed().as_secs() >= 5 {
    if let (Ok(db), Ok(downloads)) = (state.db.lock(), state.downloads.lock()) {
        if let Some(d) = downloads.get(&id) {
            let _ = crate::db::update_progress(&db, &id, d.bytes_downloaded);
        }
    }
    last_db_write = std::time::Instant::now();
}
```

- [ ] **Step 3: In status change handlers (cancel, pause, complete, error), update SQLite**

In each status-changing function, after updating the HashMap status, add:
```rust
if let Ok(db) = state.db.lock() {
    let _ = crate::db::update_status(&db, &id, "cancelled", None); // adjust status string
}
```

- [ ] **Step 4: On `remove` route, delete from SQLite**

After removing from HashMap:
```rust
if let Ok(db) = state.db.lock() {
    let _ = crate::db::delete(&db, &id);
}
```

- [ ] **Step 5: Compile check**

```bash
cargo build 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add backend/src/routes/downloads.rs
git commit -m "feat: persist download state to SQLite on every status change

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 6: Startup recovery from SQLite

**Files:**
- Modify: `backend/src/routes/downloads.rs` or `backend/src/lib.rs`

- [ ] **Step 1: Add startup recovery function to `backend/src/routes/downloads.rs`**

```rust
pub async fn recover_downloads_from_db(state: Arc<AppState>) {
    let rows = {
        let db = state.db.lock().unwrap();
        crate::db::load_resumable(&db).unwrap_or_default()
    };
    
    for row in rows {
        // Check if URL is still reachable
        let client = crate::providers::http_client();
        let reachable = match client.head(&row.url).send().await {
            Ok(resp) => resp.status().is_success() || resp.status().as_u16() == 206,
            Err(_) => false,
        };
        
        let status = if reachable {
            crate::models::DownloadStatus::Paused  // user can resume
        } else {
            crate::models::DownloadStatus::Error
        };
        
        let error = if !reachable {
            Some("Link expirado ou indisponível. Verifique e tente novamente.".to_string())
        } else {
            None
        };
        
        let download = crate::models::Download {
            id: row.id.clone(),
            url: row.url,
            provider: row.provider,
            filename: row.filename,
            dest_path: row.dest_path,
            size: row.size,
            bytes_downloaded: row.bytes_downloaded,
            status,
            error,
            retry_count: row.retry_count,
            created_at: row.created_at,
            // zero out live fields
            speed_bps: 0,
            eta_secs: 0,
            is_folder: false,
            children: None,
            max_retries: 3,
            speed_limit_kib: 0,
            parallel_parts: 4,
            selected_children: None,
            retry_at: None,
        };
        
        let mut downloads = state.downloads.lock().unwrap();
        downloads.insert(row.id, download);
    }
    tracing::info!("Recovered {} downloads from SQLite", rows.len()); // use pre-computed len
}
```

- [ ] **Step 2: Call recovery on startup in `backend/src/lib.rs`**

After creating `state` in `create_app`, spawn a task:
```rust
let state_clone = state.clone();
tokio::spawn(async move {
    crate::routes::downloads::recover_downloads_from_db(state_clone).await;
});
```

- [ ] **Step 3: Compile and run quick test**

```bash
cargo build 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add backend/src/routes/downloads.rs backend/src/lib.rs
git commit -m "feat: recover downloads from SQLite on backend startup

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 7: Add RateLimit and CaptchaRequired error types to providers

**Files:**
- Modify: `backend/src/providers/mod.rs`
- Modify: `backend/src/models.rs`

- [ ] **Step 1: Add new `ProviderError` variants in `backend/src/providers/mod.rs`**

Find the existing error enum or `anyhow::Error` usage. Add a typed error enum:
```rust
#[derive(Debug)]
pub enum ProviderError {
    RateLimit {
        retry_after_secs: u64,
        message: String,
    },
    CaptchaRequired {
        captcha_type: String,   // "recaptcha2" | "hcaptcha"
        sitekey: String,
        page_url: String,
    },
    Other(anyhow::Error),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimit { message, .. } => write!(f, "Rate limited: {}", message),
            Self::CaptchaRequired { captcha_type, .. } => write!(f, "Captcha required: {}", captcha_type),
            Self::Other(e) => write!(f, "{}", e),
        }
    }
}

impl From<anyhow::Error> for ProviderError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}
```

- [ ] **Step 2: Add new statuses to `DownloadStatus` in `backend/src/models.rs`**

In the `DownloadStatus` enum (or wherever it's defined):
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Pending,
    Downloading,
    Paused,
    Complete,
    Error,
    Cancelled,
    RateLimited,       // new
    WaitingCaptcha,    // new
}
```

- [ ] **Step 3: Add `captcha_info` to `Download` struct in `backend/src/models.rs`**

```rust
pub struct Download {
    // ... existing fields ...
    pub captcha_type: Option<String>,   // "recaptcha2" | "hcaptcha"
    pub captcha_sitekey: Option<String>,
    pub captcha_page_url: Option<String>,
    pub captcha_token: Option<String>,  // set when user solves
}
```

- [ ] **Step 4: Add new status strings to `src/shared/types.ts`**

In the `DownloadStatus` union type, add:
```typescript
  | 'rate_limited'
  | 'waiting_captcha'
```

Also add to `DownloadItem`:
```typescript
  captchaType?: string
  captchaSitekey?: string
  captchaPageUrl?: string
  retryAt?: number  // unix timestamp
```

- [ ] **Step 5: Compile check**

```bash
cargo build 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add backend/src/providers/mod.rs backend/src/models.rs src/shared/types.ts
git commit -m "feat: add RateLimit and CaptchaRequired error types

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 8: Rename fichier.rs → 1fichier.rs and enhance with folder support + rate limit

**Files:**
- Delete: `backend/src/providers/fichier.rs`
- Create: `backend/src/providers/1fichier.rs`
- Modify: `backend/src/providers/mod.rs`

- [ ] **Step 1: Read current `backend/src/providers/fichier.rs` fully**

```bash
cat /Users/lucasreolon/Desktop/Código/gDownloader/backend/src/providers/fichier.rs
```

- [ ] **Step 2: Create `backend/src/providers/1fichier.rs` with full implementation**

Rewrite the file with folder support and rate limit detection:

```rust
//! Provider pour 1Fichier (1fichier.com)
use anyhow::{anyhow, Result};
use regex::Regex;
use scraper::{Html, Selector};
use crate::models::{FileChildInfo, FileInfo};
use crate::providers::{http_client, ProviderError};

pub fn matches(url: &str) -> bool {
    url.contains("1fichier.com") || url.contains("alterupload.com") || url.contains("cjoint.net")
}

pub async fn get_file_info(url: &str) -> Result<FileInfo, ProviderError> {
    let client = http_client();
    let resp = client.get(url).send().await.map_err(|e| ProviderError::Other(e.into()))?;
    let html = resp.text().await.map_err(|e| ProviderError::Other(e.into()))?;
    let doc = Html::parse_document(&html);

    // Check for rate limit
    if let Some(secs) = parse_wait_time(&html) {
        return Err(ProviderError::RateLimit {
            retry_after_secs: secs,
            message: format!("1Fichier: aguarde {} minutos", secs / 60),
        });
    }

    // Detect folder
    if url.contains("dir=") || html.contains("liste des fichiers") || html.contains("file list") {
        return get_folder_info(url, &html).await;
    }

    // Single file
    let name = extract_filename(&doc).unwrap_or_else(|| "arquivo".to_string());
    let size = extract_size(&doc).unwrap_or(0);

    Ok(FileInfo {
        filename: name,
        size,
        mime: None,
        is_folder: false,
        children: None,
    })
}

async fn get_folder_info(url: &str, html: &str) -> Result<FileInfo, ProviderError> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("table.lst tr").unwrap();
    let link_sel = Selector::parse("a").unwrap();

    let mut children = Vec::new();
    for row in doc.select(&row_sel) {
        if let Some(a) = row.select(&link_sel).next() {
            let href = a.value().attr("href").unwrap_or("");
            if href.contains("1fichier.com/?") || href.contains("1fichier.com/!") {
                let filename = a.text().collect::<String>().trim().to_string();
                if filename.is_empty() { continue; }
                children.push(FileChildInfo {
                    url: href.to_string(),
                    filename: filename.clone(),
                    size: 0,
                    mime: None,
                });
            }
        }
    }

    let folder_name = {
        let title_sel = Selector::parse("title").unwrap();
        doc.select(&title_sel)
            .next()
            .map(|t| t.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "pasta".to_string())
    };

    Ok(FileInfo {
        filename: folder_name,
        size: 0,
        mime: None,
        is_folder: true,
        children: Some(children),
    })
}

pub async fn download(
    url: &str,
    dest_path: &str,
    tx: tokio::sync::mpsc::Sender<crate::providers::ProgressUpdate>,
    speed_limit_kib: u64,
    parallel_parts: u32,
    bytes_already: u64,
) -> Result<(), ProviderError> {
    let client = http_client();

    // Step 1: GET page to find download form / direct link
    let page_resp = client.get(url).send().await.map_err(|e| ProviderError::Other(e.into()))?;
    let html = page_resp.text().await.map_err(|e| ProviderError::Other(e.into()))?;

    // Check rate limit
    if let Some(secs) = parse_wait_time(&html) {
        return Err(ProviderError::RateLimit {
            retry_after_secs: secs,
            message: format!("1Fichier: aguarde {} minutos", secs / 60),
        });
    }

    // Extract direct download link (POST form to get it)
    let doc = Html::parse_document(&html);
    let form_sel = Selector::parse("form[method='POST']").unwrap();
    
    let direct_url = if let Some(form) = doc.select(&form_sel).next() {
        let action = form.value().attr("action").unwrap_or(url);
        // Submit the form
        let post_resp = client
            .post(action)
            .form(&[("dl", ""), ("adz", ""), ("dl2", "download")])
            .send()
            .await
            .map_err(|e| ProviderError::Other(e.into()))?;
        
        let post_html = post_resp.text().await.map_err(|e| ProviderError::Other(e.into()))?;
        
        // Check rate limit again after POST
        if let Some(secs) = parse_wait_time(&post_html) {
            return Err(ProviderError::RateLimit {
                retry_after_secs: secs,
                message: format!("1Fichier: aguarde {} minutos", secs / 60),
            });
        }
        
        // Extract link from response
        extract_direct_link(&post_html).ok_or_else(|| ProviderError::Other(anyhow!("Link de download não encontrado")))?
    } else {
        // Maybe it's already a direct link
        url.to_string()
    };

    // Download with resume support
    crate::providers::try_parallel_download(
        &client,
        &direct_url,
        dest_path,
        bytes_already,
        tx,
        speed_limit_kib,
        parallel_parts,
    )
    .await
    .map_err(|e| ProviderError::Other(e))
}

/// Parse wait time from 1fichier HTML response.
/// Returns seconds to wait, or None if no rate limit found.
pub fn parse_wait_time(html: &str) -> Option<u64> {
    // Patterns: "You must wait X minutes", "Vous devez attendre X minutes"
    // Also: "wait X seconds", "attendre X secondes"
    let minute_re = Regex::new(r"(?i)(?:must wait|attendre)\s+(\d+)\s*(?:minute|minute)").ok()?;
    let hour_re = Regex::new(r"(?i)(?:must wait|attendre)\s+(\d+)\s*(?:hour|heure)").ok()?;
    let sec_re = Regex::new(r"(?i)(?:must wait|attendre)\s+(\d+)\s*(?:second|seconde)").ok()?;
    
    // Check for JS variable: var countdown = X;
    let js_re = Regex::new(r"var\s+countdown\s*=\s*(\d+)").ok()?;

    if let Some(cap) = js_re.captures(html) {
        if let Ok(secs) = cap[1].parse::<u64>() {
            return Some(secs);
        }
    }
    if let Some(cap) = hour_re.captures(html) {
        if let Ok(h) = cap[1].parse::<u64>() {
            return Some(h * 3600);
        }
    }
    if let Some(cap) = minute_re.captures(html) {
        if let Ok(m) = cap[1].parse::<u64>() {
            return Some(m * 60);
        }
    }
    if let Some(cap) = sec_re.captures(html) {
        if let Ok(s) = cap[1].parse::<u64>() {
            return Some(s);
        }
    }
    None
}

fn extract_filename(doc: &Html) -> Option<String> {
    let sel = Selector::parse(".ct_warn span, .fl_bloc .ui-state-default, h1").ok()?;
    doc.select(&sel)
        .find_map(|el| {
            let t = el.text().collect::<String>().trim().to_string();
            if !t.is_empty() && t.len() < 256 { Some(t) } else { None }
        })
}

fn extract_size(doc: &Html) -> Option<u64> {
    let sel = Selector::parse(".dl_count, .ct_warn").ok()?;
    for el in doc.select(&sel) {
        let t = el.text().collect::<String>();
        if let Some(bytes) = parse_human_size(&t) {
            return Some(bytes);
        }
    }
    None
}

fn extract_direct_link(html: &str) -> Option<String> {
    let re = Regex::new(r#"href="(https://[^"]*\.1fichier\.com/[^"]*)"#).ok()?;
    re.captures(html).map(|c| c[1].to_string())
}

fn parse_human_size(s: &str) -> Option<u64> {
    let re = Regex::new(r"([\d.,]+)\s*(B|KB|MB|GB|TB)").ok()?;
    let cap = re.captures(s)?;
    let n: f64 = cap[1].replace(',', ".").parse().ok()?;
    let mult = match &cap[2] {
        "B" => 1.0,
        "KB" => 1024.0,
        "MB" => 1024.0 * 1024.0,
        "GB" => 1024.0 * 1024.0 * 1024.0,
        "TB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * mult) as u64)
}
```

- [ ] **Step 3: Update `backend/src/providers/mod.rs` to use #[path] for 1fichier**

Find `mod fichier;` and replace with:
```rust
#[path = "1fichier.rs"]
mod fichier;
pub use fichier::parse_wait_time as fichier_parse_wait_time;
```

Also update the provider detection to use `fichier::matches` and add "1Fichier" as the provider name (string "1Fichier").

- [ ] **Step 4: Delete old fichier.rs**

```bash
rm /Users/lucasreolon/Desktop/Código/gDownloader/backend/src/providers/fichier.rs
```

- [ ] **Step 5: Compile check**

```bash
cargo build 2>&1 | tail -30
```

- [ ] **Step 6: Commit**

```bash
git add backend/src/providers/
git commit -m "feat: rename fichier.rs→1fichier.rs, add folder support and rate limit parsing

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 9: Smart retry — handle RateLimit from providers in routes

**Files:**
- Modify: `backend/src/routes/downloads.rs`
- Modify: `backend/src/db.rs`

- [ ] **Step 1: In the download task, catch `ProviderError::RateLimit` and set `retry_at`**

In the download execution block, when the provider returns an error:
```rust
Err(crate::providers::ProviderError::RateLimit { retry_after_secs, message }) => {
    let retry_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() + retry_after_secs;
    
    // Update in-memory
    if let Ok(mut downloads) = state.downloads.lock() {
        if let Some(d) = downloads.get_mut(&id) {
            d.status = crate::models::DownloadStatus::RateLimited;
            d.retry_at = Some(retry_at);
            d.error = Some(message.clone());
        }
    }
    
    // Update SQLite
    if let Ok(db) = state.db.lock() {
        let _ = crate::db::update_retry_at(&db, &id, retry_at);
    }
    
    // Broadcast status update
    let _ = state.tx.send(crate::ws::WsEvent::StatusChanged {
        id: id.clone(),
        status: "rate_limited".to_string(),
        error: Some(message),
        retry_at: Some(retry_at),
    });
    
    // Spawn a task to auto-retry when the timer expires
    let state_clone = state.clone();
    let id_clone = id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(retry_after_secs)).await;
        crate::routes::downloads::auto_retry(&state_clone, &id_clone).await;
    });
}
```

- [ ] **Step 2: Add `auto_retry` function to `backend/src/routes/downloads.rs`**

```rust
pub async fn auto_retry(state: &Arc<AppState>, id: &str) {
    // Reset status to pending and re-queue
    if let Ok(mut downloads) = state.downloads.lock() {
        if let Some(d) = downloads.get_mut(id) {
            if d.status == crate::models::DownloadStatus::RateLimited {
                d.status = crate::models::DownloadStatus::Pending;
                d.retry_at = None;
                d.retry_count += 1;
            }
        }
    }
    // The scheduler loop will pick it up automatically
}
```

- [ ] **Step 3: Add `retry_at` to `WsEvent::StatusChanged` in `backend/src/ws.rs`**

```rust
StatusChanged {
    id: String,
    status: String,
    error: Option<String>,
    retry_at: Option<u64>,  // new field
},
```

- [ ] **Step 4: Add `retryAt` to the WebSocket status event in frontend `src/renderer/src/App.vue`**

Find where WS messages are handled and update:
```typescript
case 'status_changed': {
  const dl = downloads.value.find(d => d.id === msg.id)
  if (dl) {
    dl.status = msg.status
    dl.error = msg.error
    dl.retryAt = msg.retry_at  // new
  }
  break
}
```

- [ ] **Step 5: Compile check**

```bash
cargo build 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add backend/src/routes/downloads.rs backend/src/ws.rs src/renderer/src/App.vue
git commit -m "feat: auto-retry rate-limited downloads after server wait time expires

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 10: Countdown timer UI for rate-limited downloads

**Files:**
- Modify: `src/renderer/src/components/DownloadList.vue`

- [ ] **Step 1: Add countdown display to download card**

In the download card template, after the error/status section, add:
```html
<div v-if="item.status === 'rate_limited' && item.retryAt" class="rate-limit-row">
  <span class="rate-limit-icon">⏱</span>
  <span class="rate-limit-text">Retentando em {{ formatCountdown(item.retryAt) }}</span>
</div>
```

- [ ] **Step 2: Add `formatCountdown` function to DownloadList.vue script**

```typescript
const now = ref(Math.floor(Date.now() / 1000))

// Update every second
setInterval(() => { now.value = Math.floor(Date.now() / 1000) }, 1000)

function formatCountdown(retryAt: number): string {
  const diff = retryAt - now.value
  if (diff <= 0) return 'agora'
  const h = Math.floor(diff / 3600)
  const m = Math.floor((diff % 3600) / 60)
  const s = diff % 60
  if (h > 0) return `${h}h ${m}m`
  if (m > 0) return `${m}m ${s}s`
  return `${s}s`
}
```

- [ ] **Step 3: Add minimal CSS for the rate limit row**

```css
.rate-limit-row {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-muted);
  margin-top: 4px;
}
.rate-limit-icon { font-size: 14px; }
```

- [ ] **Step 4: Commit**

```bash
git add src/renderer/src/components/DownloadList.vue
git commit -m "feat: show rate-limit countdown timer in download card

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 11: Terabox — replace RSA login with BrowserWindow

**Files:**
- Modify: `src/main/index.ts`

- [ ] **Step 1: Read current Terabox login code in `src/main/index.ts` lines 277–448**

```bash
sed -n '270,460p' /Users/lucasreolon/Desktop/Código/gDownloader/src/main/index.ts
```

- [ ] **Step 2: Replace `verifyTeraboxAccount` with BrowserWindow approach**

Delete the old `verifyTeraboxAccount` function and replace with:

```typescript
import { BrowserWindow, session } from 'electron'

async function loginTeraboxWithBrowser(mainWindow: BrowserWindow): Promise<string[]> {
  return new Promise((resolve, reject) => {
    const loginWin = new BrowserWindow({
      parent: mainWindow,
      modal: true,
      width: 480,
      height: 560,
      title: 'Entrar no Terabox',
      autoHideMenuBar: true,
      webPreferences: {
        nodeIntegration: false,
        contextIsolation: true,
        partition: 'persist:terabox',
      },
    })

    // Inject CSS to hide everything except the login form
    loginWin.webContents.on('dom-ready', () => {
      loginWin.webContents.insertCSS(`
        header, footer, .nav, .sidebar, .banner, .ad, [class*="header"],
        [class*="footer"], [class*="nav"], [class*="banner"], [class*="logo"],
        [class*="promotion"], [class*="download-app"] { display: none !important; }
        body { background: #1a1a2e !important; }
        .login-wrap, .login-form, #TANGRAM__PSP_4__wrapper,
        [class*="login"] { 
          margin: 20px auto !important; 
          box-shadow: none !important;
        }
      `)
    })

    // Poll for session cookies
    const cookieSession = session.fromPartition('persist:terabox')
    const pollInterval = setInterval(async () => {
      const cookies = await cookieSession.cookies.get({ domain: '.terabox.com' })
      const sessionCookies = cookies.filter(c =>
        ['ndus', 'ndut', 'BDUSS', 'STOKEN'].includes(c.name)
      )
      if (sessionCookies.length >= 2) {
        clearInterval(pollInterval)
        const cookieHeader = sessionCookies
          .map(c => `${c.name}=${c.value}`)
          .join('; ')
        loginWin.close()
        resolve([cookieHeader])
      }
    }, 500)

    loginWin.on('closed', () => {
      clearInterval(pollInterval)
      reject(new Error('Login cancelado pelo usuário'))
    })

    loginWin.loadURL('https://www.terabox.com/login')
  })
}
```

- [ ] **Step 3: Update the `auth:login` IPC handler to use the new function**

Find `ipcMain.handle('auth:login', ...)` and replace the body:
```typescript
ipcMain.handle('auth:login', async (_event, moduleId: string) => {
  if (moduleId !== 'terabox') throw new Error('Módulo desconhecido')
  
  const cookies = await loginTeraboxWithBrowser(mainWindow)
  
  const settings = readSettingsFromDisk()
  if (!settings.accounts) settings.accounts = {}
  settings.accounts.terabox = {
    email: '',
    password: '',
    cookies,
    verifiedAt: new Date().toISOString(),
  }
  writeSettingsAndSync(settings)
  return { success: true }
})
```

- [ ] **Step 4: Update `AccountSettings.vue` — remove email/password fields, show only "Conectar" button**

Replace the Terabox login form with:
```html
<div v-if="!isLoggedIn" class="connect-section">
  <p class="connect-desc">Conecte sua conta Terabox para desbloquear downloads de links privados.</p>
  <button class="connect-btn" :disabled="loading" @click="login">
    {{ loading ? 'Abrindo login...' : 'Conectar Terabox' }}
  </button>
</div>
<div v-else class="connected-section">
  <span class="connected-badge">✓ Conectado</span>
  <button class="disconnect-btn" @click="logout">Desconectar</button>
</div>
```

And update the script `login()` to just call:
```typescript
async function login(): Promise<void> {
  loading.value = true
  try {
    await window.api.auth.login('terabox', {})
    isLoggedIn.value = true
  } catch (e: any) {
    errorMsg.value = e.message || 'Falha no login'
  } finally {
    loading.value = false
  }
}
```

- [ ] **Step 5: Compile check (TypeScript)**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
npx tsc --noEmit 2>&1 | head -30
```

- [ ] **Step 6: Commit**

```bash
git add src/main/index.ts src/renderer/src/components/AccountSettings.vue
git commit -m "feat: replace Terabox RSA login with BrowserWindow cookie capture

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 12: Captcha system — backend endpoint + NoPecha

**Files:**
- Create: `backend/src/routes/captcha.rs`
- Modify: `backend/src/lib.rs`
- Modify: `backend/src/routes/downloads.rs`
- Modify: `src/main/index.ts`
- Modify: `src/preload/index.ts`
- Modify: `src/shared/types.ts`
- Modify: `src/renderer/src/components/AppSettings.vue`

- [ ] **Step 1: Create `backend/src/routes/captcha.rs` — serve captcha widget HTML**

```rust
use axum::{extract::Query, response::Html};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CaptchaParams {
    pub r#type: String,    // "recaptcha2" | "hcaptcha"
    pub sitekey: String,
    pub pageurl: String,
}

pub async fn captcha_page(Query(params): Query<CaptchaParams>) -> Html<String> {
    let (script_url, widget_div, callback_name) = match params.r#type.as_str() {
        "hcaptcha" => (
            format!("https://js.hcaptcha.com/1/api.js"),
            format!(r#"<div class="h-captcha" data-sitekey="{}" data-callback="onSolved"></div>"#, params.sitekey),
            "hcaptcha".to_string(),
        ),
        _ => ( // recaptcha2 default
            "https://www.google.com/recaptcha/api.js".to_string(),
            format!(r#"<div class="g-recaptcha" data-sitekey="{}" data-callback="onSolved"></div>"#, params.sitekey),
            "recaptcha".to_string(),
        ),
    };

    Html(format!(r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ 
    background: transparent; 
    display: flex; 
    justify-content: center; 
    align-items: flex-start;
    padding: 8px;
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
  }}
</style>
</head>
<body>
  {}
  <script>
    function onSolved(token) {{
      window.parent.postMessage({{ type: 'captcha-token', token }}, '*');
    }}
  </script>
  <script src="{}" async defer></script>
</body>
</html>"#, widget_div, script_url))
}

pub async fn submit_captcha(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    axum::Json(body): axum::Json<SubmitCaptchaBody>,
) -> axum::Json<serde_json::Value> {
    // Store token in download, re-queue for download attempt
    if let Ok(mut downloads) = state.downloads.lock() {
        if let Some(d) = downloads.get_mut(&body.download_id) {
            d.captcha_token = Some(body.token);
            d.status = crate::models::DownloadStatus::Pending;
        }
    }
    axum::Json(serde_json::json!({ "ok": true }))
}

#[derive(serde::Deserialize)]
pub struct SubmitCaptchaBody {
    pub download_id: String,
    pub token: String,
}
```

- [ ] **Step 2: Register captcha routes in `backend/src/lib.rs`**

```rust
mod routes {
    pub mod captcha;
    // ... existing
}
// In router:
.route("/captcha", axum::routing::get(routes::captcha::captcha_page))
.route("/captcha/submit", axum::routing::post(routes::captcha::submit_captcha))
```

- [ ] **Step 3: Add `nopechaApiKey` to `PersistedSettings` in `src/shared/types.ts`**

```typescript
  nopechaApiKey?: string
```

- [ ] **Step 4: Add NoPecha auto-solve IPC handler in `src/main/index.ts`**

```typescript
ipcMain.handle('captcha:nopecha-solve', async (_event, params: {
  type: string
  sitekey: string
  pageurl: string
}) => {
  const settings = readSettingsFromDisk()
  const apiKey = settings.nopechaApiKey
  if (!apiKey) return null

  // Submit task
  const submitRes = await fetch('https://api.nopecha.com/', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      type: params.type,
      sitekey: params.sitekey,
      url: params.pageurl,
      key: apiKey,
    }),
  }).then(r => r.json()).catch(() => null)

  if (!submitRes?.data) return null
  const taskId = submitRes.data

  // Poll up to 120s
  for (let i = 0; i < 60; i++) {
    await new Promise(r => setTimeout(r, 2000))
    const res = await fetch(`https://api.nopecha.com/?id=${taskId}&key=${apiKey}`)
      .then(r => r.json()).catch(() => null)
    if (res?.data?.[0]) return res.data[0]
  }
  return null
})
```

- [ ] **Step 5: Expose in `src/preload/index.ts`**

```typescript
captcha: {
  nopechaSolve: (params: { type: string; sitekey: string; pageurl: string }) =>
    ipcRenderer.invoke('captcha:nopecha-solve', params),
},
```

- [ ] **Step 6: Add NoPecha key field to `AppSettings.vue` (Appearance section)**

```html
<div class="setting-row">
  <div class="setting-info">
    <span class="setting-label">NoPecha API Key</span>
    <span class="setting-desc">Para resolver captchas automaticamente</span>
  </div>
  <input
    v-model="settings.nopechaApiKey"
    type="password"
    class="setting-input setting-input-wide"
    placeholder="nopecha_xxxxxxxxx"
    @change="save"
  />
</div>
```

Also add `nopechaApiKey?: string` to `AppSettings` interface and `nopechaApiKey: undefined` to reactive defaults.

- [ ] **Step 7: Commit**

```bash
git add backend/src/routes/captcha.rs backend/src/lib.rs src/main/index.ts src/preload/index.ts src/shared/types.ts src/renderer/src/components/AppSettings.vue
git commit -m "feat: captcha system — backend HTML endpoint, NoPecha auto-solve IPC

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 13: Captcha inline UI in download card

**Files:**
- Modify: `src/renderer/src/components/DownloadList.vue`

- [ ] **Step 1: Add captcha iframe to download card template**

In the download card, add below the progress/error section:
```html
<div v-if="item.status === 'waiting_captcha'" class="captcha-row">
  <div class="captcha-header">
    <span>🔒 Verificação necessária</span>
    <span class="captcha-auto-note" v-if="nopechaConfigured">Tentando resolver automaticamente...</span>
  </div>
  <iframe
    v-if="!captchaSolving[item.id]"
    :src="captchaUrl(item)"
    class="captcha-iframe"
    frameborder="0"
    scrolling="no"
    @load="onCaptchaIframeLoad(item)"
  />
</div>
```

- [ ] **Step 2: Add captcha logic to DownloadList.vue script**

```typescript
import { ref, computed } from 'vue'

const captchaSolving = ref<Record<string, boolean>>({})

const backendPort = ref(0)
window.api.getBackendPort?.().then((p: number) => { backendPort.value = p })

function captchaUrl(item: DownloadItem): string {
  if (!item.captchaSitekey || !item.captchaType) return ''
  const base = `http://127.0.0.1:${backendPort.value}/captcha`
  return `${base}?type=${item.captchaType}&sitekey=${encodeURIComponent(item.captchaSitekey)}&pageurl=${encodeURIComponent(item.captchaPageUrl ?? item.url)}`
}

function onCaptchaIframeLoad(item: DownloadItem): void {
  // Try NoPecha first
  if (window.api.captcha?.nopechaSolve && item.captchaSitekey) {
    window.api.captcha.nopechaSolve({
      type: item.captchaType ?? 'recaptcha2',
      sitekey: item.captchaSitekey,
      pageurl: item.captchaPageUrl ?? item.url,
    }).then((token: string | null) => {
      if (token) submitCaptchaToken(item.id, token)
    })
  }
}

async function submitCaptchaToken(downloadId: string, token: string): Promise<void> {
  captchaSolving.value[downloadId] = true
  await fetch(`http://127.0.0.1:${backendPort.value}/captcha/submit`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ download_id: downloadId, token }),
  })
}

// Listen for postMessage from captcha iframe
window.addEventListener('message', (ev) => {
  if (ev.data?.type === 'captcha-token' && ev.data.token) {
    // Find which download is waiting for captcha
    const item = downloads.value.find(d => d.status === 'waiting_captcha')
    if (item) submitCaptchaToken(item.id, ev.data.token)
  }
})
```

- [ ] **Step 3: Add captcha CSS**

```css
.captcha-row {
  margin-top: 10px;
  border: 1px solid var(--border-color);
  border-radius: 8px;
  overflow: hidden;
  background: var(--card-bg);
}
.captcha-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  font-size: 13px;
  border-bottom: 1px solid var(--border-color);
}
.captcha-iframe {
  width: 100%;
  height: 100px;
  border: none;
  background: transparent;
}
.captcha-auto-note {
  font-size: 11px;
  color: var(--text-muted);
}
```

- [ ] **Step 4: Commit**

```bash
git add src/renderer/src/components/DownloadList.vue
git commit -m "feat: inline captcha widget in download card with NoPecha auto-resolve

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 14: Rapidgator provider

**Files:**
- Create: `backend/src/providers/rapidgator.rs`
- Modify: `backend/src/providers/mod.rs`

- [ ] **Step 1: Create `backend/src/providers/rapidgator.rs`**

```rust
//! Provider para Rapidgator (rapidgator.net)
use anyhow::{anyhow, Result};
use regex::Regex;
use scraper::{Html, Selector};
use crate::models::FileInfo;
use crate::providers::{http_client, ProviderError, ProgressUpdate};

pub fn matches(url: &str) -> bool {
    url.contains("rapidgator.net/file/")
}

pub async fn get_file_info(url: &str) -> Result<FileInfo, ProviderError> {
    let client = http_client();
    let resp = client.get(url).send().await.map_err(|e| ProviderError::Other(e.into()))?;
    let html = resp.text().await.map_err(|e| ProviderError::Other(e.into()))?;

    if let Some(secs) = parse_wait_time(&html) {
        return Err(ProviderError::RateLimit {
            retry_after_secs: secs,
            message: format!("Rapidgator: aguarde {} horas", secs / 3600),
        });
    }

    if let Some((sitekey, page_url)) = detect_captcha(url, &html) {
        return Err(ProviderError::CaptchaRequired {
            captcha_type: "recaptcha2".to_string(),
            sitekey,
            page_url,
        });
    }

    let doc = Html::parse_document(&html);
    let name = extract_filename(&doc).unwrap_or_else(|| "arquivo".to_string());
    let size = extract_size(&doc).unwrap_or(0);

    Ok(FileInfo {
        filename: name,
        size,
        mime: None,
        is_folder: false,
        children: None,
    })
}

pub async fn download(
    url: &str,
    dest_path: &str,
    tx: tokio::sync::mpsc::Sender<ProgressUpdate>,
    speed_limit_kib: u64,
    parallel_parts: u32,
    bytes_already: u64,
    captcha_token: Option<&str>,
) -> Result<(), ProviderError> {
    let client = http_client();

    let page = client.get(url).send().await.map_err(|e| ProviderError::Other(e.into()))?;
    let html = page.text().await.map_err(|e| ProviderError::Other(e.into()))?;

    // Check rate limit
    if let Some(secs) = parse_wait_time(&html) {
        return Err(ProviderError::RateLimit {
            retry_after_secs: secs,
            message: format!("Rapidgator: aguarde {} horas", secs / 3600),
        });
    }

    // Detect captcha if no token provided
    if let Some((sitekey, page_url)) = detect_captcha(url, &html) {
        if captcha_token.is_none() {
            return Err(ProviderError::CaptchaRequired {
                captcha_type: "recaptcha2".to_string(),
                sitekey,
                page_url,
            });
        }
    }

    // Extract file ID from URL
    let file_id = extract_file_id(url)
        .ok_or_else(|| ProviderError::Other(anyhow!("ID do arquivo não encontrado")))?;

    // POST to get download link (with captcha token if available)
    let mut form = vec![
        ("DownloadMd5Form[code]", file_id.as_str()),
    ];
    let token_str = captcha_token.unwrap_or("");
    if !token_str.is_empty() {
        form.push(("DownloadMd5Form[captcha]", token_str));
    }

    let api_url = format!("https://rapidgator.net/download/AjaxStartTimer/{}", file_id);
    let start_resp = client
        .post(&api_url)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Referer", url)
        .form(&form)
        .send()
        .await
        .map_err(|e| ProviderError::Other(e.into()))?;

    let start_json: serde_json::Value = start_resp.json().await.map_err(|e| ProviderError::Other(e.into()))?;

    // Check for wait time in JSON
    if let Some(delay) = start_json["delay"].as_i64() {
        if delay > 0 {
            return Err(ProviderError::RateLimit {
                retry_after_secs: delay as u64,
                message: format!("Rapidgator: aguarde {}s", delay),
            });
        }
    }

    let sid = start_json["sid"].as_str().unwrap_or("").to_string();

    // Wait for delay (free users)
    if let Some(wait) = start_json["next"].as_i64() {
        if wait > 0 {
            tokio::time::sleep(tokio::time::Duration::from_secs(wait as u64)).await;
        }
    }

    // Get download URL
    let dl_url_api = format!(
        "https://rapidgator.net/download/AjaxGetDownloadLink?sid={}",
        sid
    );
    let dl_resp: serde_json::Value = client
        .get(&dl_url_api)
        .header("X-Requested-With", "XMLHttpRequest")
        .send()
        .await
        .map_err(|e| ProviderError::Other(e.into()))?
        .json()
        .await
        .map_err(|e| ProviderError::Other(e.into()))?;

    let direct_url = dl_resp["url"].as_str()
        .ok_or_else(|| ProviderError::Other(anyhow!("URL de download não retornada pelo Rapidgator")))?
        .to_string();

    crate::providers::try_parallel_download(
        &client,
        &direct_url,
        dest_path,
        bytes_already,
        tx,
        speed_limit_kib,
        parallel_parts,
    )
    .await
    .map_err(|e| ProviderError::Other(e))
}

pub fn parse_wait_time(html: &str) -> Option<u64> {
    // "Please wait X hours" / "wait X minutes"
    let re_h = Regex::new(r"(?i)wait\s+(\d+)\s+hour").ok()?;
    let re_m = Regex::new(r"(?i)wait\s+(\d+)\s+minute").ok()?;
    let re_s = Regex::new(r"(?i)wait\s+(\d+)\s+second").ok()?;

    if let Some(c) = re_h.captures(html) {
        return Some(c[1].parse::<u64>().unwrap_or(1) * 3600);
    }
    if let Some(c) = re_m.captures(html) {
        return Some(c[1].parse::<u64>().unwrap_or(1) * 60);
    }
    if let Some(c) = re_s.captures(html) {
        return Some(c[1].parse::<u64>().unwrap_or(30));
    }
    None
}

fn detect_captcha(page_url: &str, html: &str) -> Option<(String, String)> {
    // reCaptcha v2 sitekey
    let re = Regex::new(r#"data-sitekey="([^"]+)""#).ok()?;
    let cap = re.captures(html)?;
    Some((cap[1].to_string(), page_url.to_string()))
}

fn extract_file_id(url: &str) -> Option<String> {
    let re = Regex::new(r"rapidgator\.net/file/([a-f0-9]+)").ok()?;
    re.captures(url).map(|c| c[1].to_string())
}

fn extract_filename(doc: &Html) -> Option<String> {
    let sel = Selector::parse("h1, .file-info h1, title").ok()?;
    doc.select(&sel)
        .find_map(|el| {
            let t = el.text().collect::<String>().trim().to_string();
            if !t.is_empty() && !t.contains("Rapidgator") { Some(t) } else { None }
        })
}

fn extract_size(doc: &Html) -> Option<u64> {
    let sel = Selector::parse(".file-info li, .file-size").ok()?;
    for el in doc.select(&sel) {
        let t = el.text().collect::<String>();
        if t.contains("B") || t.contains("KB") || t.contains("MB") || t.contains("GB") {
            if let Some(bytes) = parse_human_size(&t) {
                return Some(bytes);
            }
        }
    }
    None
}

fn parse_human_size(s: &str) -> Option<u64> {
    let re = Regex::new(r"([\d.,]+)\s*(B|KB|MB|GB|TB)").ok()?;
    let cap = re.captures(s)?;
    let n: f64 = cap[1].replace(',', ".").parse().ok()?;
    let mult = match &cap[2] {
        "B" => 1.0,
        "KB" => 1024.0,
        "MB" => 1024.0 * 1024.0,
        "GB" => 1024.0 * 1024.0 * 1024.0,
        "TB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * mult) as u64)
}
```

- [ ] **Step 2: Register Rapidgator in `backend/src/providers/mod.rs`**

Add `mod rapidgator;` and add to the provider detection list:
```rust
mod rapidgator;

// In detect_provider():
if rapidgator::matches(url) {
    return Some(("Rapidgator", ...));
}
```

Also add Rapidgator to `get_file_info` and `download` dispatch.

- [ ] **Step 3: Compile and test**

```bash
cargo build 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add backend/src/providers/rapidgator.rs backend/src/providers/mod.rs
git commit -m "feat: add Rapidgator provider with captcha and rate limit support

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Task 15: Update README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update providers table in README**

Add Rapidgator and update 1Fichier entry:
```markdown
| Provider    | Pastas | Auth       | Captcha | Rate Limit |
|-------------|--------|------------|---------|------------|
| Mega        | ✅     | Opcional   | ❌      | ✅ auto    |
| MediaFire   | ❌     | ❌         | ❌      | ✅ auto    |
| Google Drive| ✅     | Opcional   | ❌      | ❌         |
| PixelDrain  | ❌     | ❌         | ❌      | ❌         |
| 1Fichier    | ✅     | Opcional   | ❌      | ✅ auto    |
| Terabox     | ✅     | ✅ OAuth   | ✅      | ❌         |
| SharePoint  | ✅     | Público    | ❌      | ❌         |
| Drime       | ✅     | ❌         | ❌      | ❌         |
| Rapidgator  | ❌     | ❌ (free)  | ✅ auto | ✅ auto    |
```

- [ ] **Step 2: Add SQLite and captcha sections to README**

Add to the features list:
```markdown
- **Persistência SQLite** — Downloads salvos em banco local; retoma de onde parou mesmo após reiniciar
- **Sistema de Captcha** — Resolve reCaptcha/hCaptcha inline na interface; integração com NoPecha API para auto-resolução
- **Rate Limit Inteligente** — Detecta tempo de espera de cada servidor e agenda retry automático com countdown
- **Login Terabox** — Login seguro via browser integrado (sem armazenar senha)
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: update README with new providers, SQLite, captcha, and smart retry

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

## Self-Review Checklist

- [x] Settings accentColor — persists in JSON ✅
- [x] Terabox BrowserWindow — replaces RSA ✅
- [x] SQLite schema — all required fields ✅
- [x] SQLite startup recovery — HEAD check per URL ✅
- [x] 1fichier renamed to 1fichier.rs — via #[path] ✅
- [x] 1fichier folder support — HTML scraping ✅
- [x] 1fichier rate limit — parse_wait_time ✅
- [x] Rapidgator provider — captcha + rate limit + download ✅
- [x] Smart retry timing — auto-retry task spawned after wait ✅
- [x] Countdown timer UI — setInterval in Vue ✅
- [x] Captcha backend HTML endpoint — /captcha route ✅
- [x] NoPecha IPC handler — polls up to 120s ✅
- [x] Captcha inline iframe — postMessage token flow ✅
- [x] README updated ✅

**Potential gaps:**
- Mega rate limit: the `parse_wait_time` for Mega (`EOVERQUOTA` = error -18) should be handled in `mega.rs`. Add to existing Mega error handling: return `ProviderError::RateLimit { retry_after_secs: 6 * 3600, message: "Mega: quota excedida, aguarde 6 horas".to_string() }` when API returns -18.
- The `captcha_token` field needs to be passed through to the download call for Rapidgator — ensure the download dispatch in `routes/downloads.rs` reads `d.captcha_token` and passes it.
