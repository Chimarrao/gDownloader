# YouTube UI Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add video thumbnail as icon, channel name + avatar, fix skeleton size, and fix YouTube download progress jump.

**Architecture:** Extend `FileInfo` (Rust) and `Download` (Rust) with 3 optional fields (`thumbnail_url`, `channel_name`, `channel_thumbnail_url`). YouTube provider fetches channel info via a second `yt-dlp -J` call in parallel. Preload maps new fields to TS `DownloadItem`. Vue components use `<img>` instead of SVG icon for YouTube, display channel name, fix skeleton HTML/CSS. Note: `.progress-fill` already has `transition: width 0.55s ease-out` in the existing CSS — the progress fix is purely a backend change (Task 3).

**Tech Stack:** Rust (tokio, serde_json), TypeScript, Vue 3 SFC

---

## File Map

| File | Change |
|---|---|
| `backend/src/models.rs` | Add 3 optional fields to `FileInfo` and `Download` structs |
| `backend/src/providers/youtube.rs` | Add `uploader`/`channel_id` to `YtdlpInfo`, fetch channel info in parallel, fix Concluído progress event |
| `backend/src/routes/providers.rs` | Include new fields in `/file-info` JSON response |
| `backend/src/routes/downloads.rs` | Copy new fields from `file_info` to `Download` struct at lines 433–479 |
| `src/shared/types.ts` | Add optional fields to `FileInfo` and `DownloadItem` |
| `src/preload/index.ts` | Map `thumbnail_url`, `channel_name`, `channel_thumbnail_url` in `rustDownloadToItem` |
| `src/renderer/src/components/CapturedResultsPanel.vue` | Thumbnail img icon, channel avatar + name in sub-line |
| `src/renderer/src/components/DownloadList.vue` | Thumbnail img icon, channel line, skeleton structure fix, progress-fill CSS transition |

---

## Task 1: Extend Rust models with thumbnail/channel fields

**Files:**
- Modify: `backend/src/models.rs:156-162` (FileInfo struct)
- Modify: `backend/src/models.rs:34-73` (Download struct)

- [ ] **Step 1: Add fields to FileInfo**

In `backend/src/models.rs`, find the `FileInfo` struct (line ~156) and add 3 optional fields:

```rust
// Before (existing):
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub filename: String,
    pub size: u64,
    pub mime_type: Option<String>,
    pub is_folder: bool,
    pub children: Option<Vec<FileChildInfo>>,
}

// After:
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileInfo {
    pub filename: String,
    pub size: u64,
    pub mime_type: Option<String>,
    pub is_folder: bool,
    pub children: Option<Vec<FileChildInfo>>,
    pub thumbnail_url: Option<String>,
    pub channel_name: Option<String>,
    pub channel_thumbnail_url: Option<String>,
}
```

Note: add `Default` to derive list so callers that use `FileInfo { ..Default::default() }` still work.

- [ ] **Step 2: Add fields to Download struct**

In the same file, find the `Download` struct (line ~34) and add 3 fields after `network_route`:

```rust
    #[serde(default)]
    pub network_route: Option<DownloadNetworkRoute>,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    #[serde(default)]
    pub channel_name: Option<String>,
    #[serde(default)]
    pub channel_thumbnail_url: Option<String>,
}
```

- [ ] **Step 3: Verify it compiles**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader/backend
cargo check 2>&1 | head -40
```

Expected: no errors (warnings about unused fields are OK).

- [ ] **Step 4: Commit**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
git add backend/src/models.rs
git commit -m "feat(backend): add thumbnail_url, channel_name, channel_thumbnail_url to FileInfo and Download"
```

---

## Task 2: YouTube provider — fetch video thumbnail + channel info

**Files:**
- Modify: `backend/src/providers/youtube.rs`

- [ ] **Step 1: Add channel fields to YtdlpInfo and add YtdlpChannelInfo struct**

In `youtube.rs`, extend the existing `YtdlpInfo` struct and add `YtdlpChannelInfo`:

```rust
// Replace existing YtdlpInfo (around line 22):
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct YtdlpInfo {
    id: Option<String>,
    title: Option<String>,
    thumbnail: Option<String>,
    duration: Option<f64>,
    webpage_url: Option<String>,
    entries: Option<Vec<YtdlpEntry>>,
    formats: Option<Vec<YtdlpFormat>>,
    uploader: Option<String>,
    channel_id: Option<String>,
}

// Add new struct after YtdlpInfo:
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct YtdlpChannelInfo {
    thumbnail: Option<String>,
    title: Option<String>,
}
```

- [ ] **Step 2: Add channel info fetch helper method**

Add this method to the `impl YouTubeProvider` block, after the existing `read_info` method:

```rust
async fn read_channel_info(channel_id: &str, context: Option<&DownloadContext>) -> Option<YtdlpChannelInfo> {
    let channel_url = format!("https://www.youtube.com/channel/{channel_id}");
    let mut args = Vec::<String>::new();
    if let Some(context) = context {
        Self::apply_cookies_args(&mut args, context);
        Self::apply_proxy_args(&mut args, context);
    }
    args.extend([
        "-J".to_string(),
        "--flat-playlist".to_string(),
        "--playlist-items".to_string(),
        "0".to_string(),
        "--no-warnings".to_string(),
        channel_url,
    ]);
    let output = tokio::time::timeout(
        tokio::time::Duration::from_secs(10),
        tokio::process::Command::new(Self::ytdlp_bin()).args(args).output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice::<YtdlpChannelInfo>(&output.stdout).ok()
}
```

- [ ] **Step 3: Update `info_for` to populate new FileInfo fields**

In the `info_for` async function, after the existing `Ok(FileInfo { filename, size, ... })` for the single video case (around line 409–416), change it to:

```rust
// After building `children` from formats, before returning:
let thumbnail_url = info.thumbnail.clone();
let channel_name = info.uploader.clone();
let channel_id = info.channel_id.clone();

// Fetch channel info in parallel (non-blocking, 10s timeout)
let channel_thumbnail_url = if let Some(ref cid) = channel_id {
    let cid = cid.clone();
    let ctx_clone = context.cloned();
    // Run the channel fetch as a separate task and await it
    let handle = tokio::spawn(async move {
        Self::read_channel_info(&cid, ctx_clone.as_ref()).await
            .and_then(|ch| ch.thumbnail)
    });
    tokio::time::timeout(tokio::time::Duration::from_secs(12), handle)
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten()
} else {
    None
};

Ok(FileInfo {
    filename,
    size: children.iter().map(|child| child.size).max().unwrap_or(0),
    mime_type: Some("video/*".to_string()),
    is_folder: false,
    children: Some(children),
    thumbnail_url,
    channel_name,
    channel_thumbnail_url,
})
```

Also update the **playlist** branch return at line ~385–392 to include the fields (thumbnail from info, no channel lookup for playlists):

```rust
return Ok(FileInfo {
    filename,
    size: 0,
    mime_type: Some("application/vnd.youtube.playlist".to_string()),
    is_folder: true,
    children: Some(std::mem::take(&mut children)),
    thumbnail_url: info.thumbnail.clone(),
    channel_name: info.uploader.clone(),
    channel_thumbnail_url: None,
});
```

The `info_for` signature accepts `context: Option<&DownloadContext>`. The `context.cloned()` call requires `DownloadContext: Clone`. Verify this is already derived; if not, add `#[derive(Clone)]` to `DownloadContext` in `backend/src/providers/mod.rs`.

- [ ] **Step 4: Verify DownloadContext already has Clone (no change needed)**

`DownloadContext` at `backend/src/providers/mod.rs:428` already derives `Clone`:
```rust
#[derive(Debug, Clone, Default)]
pub struct DownloadContext { ... }
```
No code change needed. The `context.cloned()` call in Step 3 will compile as-is.

- [ ] **Step 5: Compile check**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader/backend
cargo check 2>&1 | head -60
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
git add backend/src/providers/youtube.rs backend/src/providers/mod.rs
git commit -m "feat(youtube): fetch video thumbnail and channel avatar via second yt-dlp call"
```

---

## Task 3: Fix YouTube progress jump

**Files:**
- Modify: `backend/src/providers/youtube.rs` (end of `download_with_context`, around line 852–869)

The problem: after download, the code does `grand_downloaded = final_size; grand_total = final_size;` then sends `(final_size, final_size)` as the Concluído event. The frontend's progress bar was using synthetic scale (total=10,000), so receiving total=500MB causes the scale to switch and percent jumps from last synthetic value to 100% without intermediate values.

- [ ] **Step 1: Fix the Concluído send_stage call**

Find the block at the end of the `for item_url in urls` loop (around line 850–870):

```rust
// BEFORE (existing code):
let final_size = Self::output_size(dest_path, started_at).await;
if final_size > 0 {
    grand_downloaded = final_size;
    grand_total = final_size;
}
Self::send_stage(
    &progress_tx,
    &progress_child_key,
    "Concluído",
    grand_total,
    grand_total.max(SYNTHETIC_PROGRESS_TOTAL),
)
.await;

// AFTER:
let final_size = Self::output_size(dest_path, started_at).await;
// Always report completion using the synthetic scale to avoid a sudden
// scale-switch on the frontend (which would look like "100% from nowhere").
// The real file size is returned as the function's Ok() value for backend tracking.
Self::send_stage(
    &progress_tx,
    &progress_child_key,
    "Concluído",
    SYNTHETIC_PROGRESS_TOTAL,
    SYNTHETIC_PROGRESS_TOTAL,
)
.await;
// Update tracking variables for the return value
grand_downloaded = if final_size > 0 { final_size } else {
    grand_downloaded.saturating_add(
        last_synthetic_downloaded
            .max(SYNTHETIC_PROGRESS_TOTAL)
    )
};
grand_total = grand_total.max(grand_downloaded);
```

- [ ] **Step 2: Compile check**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader/backend
cargo check 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
git add backend/src/providers/youtube.rs
git commit -m "fix(youtube): send synthetic progress scale at completion to avoid frontend 100% jump"
```

---

## Task 4: Expose new fields in `/file-info` HTTP response

**Files:**
- Modify: `backend/src/routes/providers.rs:126-133`

- [ ] **Step 1: Update the JSON response**

Find the block at lines ~126–133:

```rust
// BEFORE:
Ok(Json(serde_json::json!({
    "name": info.filename,
    "size": info.size,
    "mimeType": info.mime_type,
    "isFolder": info.is_folder,
    "children": info.children,
})))

// AFTER:
Ok(Json(serde_json::json!({
    "name": info.filename,
    "size": info.size,
    "mimeType": info.mime_type,
    "isFolder": info.is_folder,
    "children": info.children,
    "thumbnailUrl": info.thumbnail_url,
    "channelName": info.channel_name,
    "channelThumbnailUrl": info.channel_thumbnail_url,
})))
```

- [ ] **Step 2: Compile check**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader/backend
cargo check 2>&1 | head -20
```

- [ ] **Step 3: Commit**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
git add backend/src/routes/providers.rs
git commit -m "feat(api): expose thumbnailUrl, channelName, channelThumbnailUrl in /file-info response"
```

---

## Task 5: Copy new fields into Download struct when queuing

**Files:**
- Modify: `backend/src/routes/downloads.rs:433-479`

- [ ] **Step 1: Add the 3 fields to the Download struct literal**

Find the `Download { ... }` construction around lines 433–479 and add the 3 new fields after `network_route: None`:

```rust
    network_route: None,
    thumbnail_url: file_info.thumbnail_url.clone(),
    channel_name: file_info.channel_name.clone(),
    channel_thumbnail_url: file_info.channel_thumbnail_url.clone(),
};
```

- [ ] **Step 2: Compile check**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader/backend
cargo check 2>&1 | head -20
```

- [ ] **Step 3: Commit**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
git add backend/src/routes/downloads.rs
git commit -m "feat(downloads): propagate thumbnail_url/channel fields from file_info to Download"
```

---

## Task 6: TypeScript types + preload mapping

**Files:**
- Modify: `src/shared/types.ts`
- Modify: `src/preload/index.ts`

- [ ] **Step 1: Extend FileInfo in types.ts**

Find `export interface FileInfo` in `src/shared/types.ts` (line ~15) and add 3 optional fields:

```typescript
export interface FileInfo {
  name: string
  size: number
  mimeType?: string
  isFolder?: boolean
  children?: DownloadChild[]
  thumbnailUrl?: string
  channelName?: string
  channelThumbnailUrl?: string
}
```

- [ ] **Step 2: Extend DownloadItem in types.ts**

Find `export interface DownloadItem` (line ~60) and add 3 optional fields after `lastProgressAt`:

```typescript
  lastProgressAt?: number
  thumbnailUrl?: string
  channelName?: string
  channelThumbnailUrl?: string
}
```

- [ ] **Step 3: Map new fields in rustDownloadToItem in preload/index.ts**

Find the `rustDownloadToItem` function (line ~990) and add 3 mappings at the end of the returned object, before the closing `}`:

```typescript
    lastProgressAt: d.last_progress_at ? (d.last_progress_at as number) * 1000 : undefined,
    thumbnailUrl: typeof d.thumbnail_url === 'string' ? d.thumbnail_url : undefined,
    channelName: typeof d.channel_name === 'string' ? d.channel_name : undefined,
    channelThumbnailUrl: typeof d.channel_thumbnail_url === 'string' ? d.channel_thumbnail_url : undefined,
  }
```

- [ ] **Step 4: Type check**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
npx tsc --noEmit 2>&1 | head -30
```

Expected: no new errors.

- [ ] **Step 5: Commit**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
git add src/shared/types.ts src/preload/index.ts
git commit -m "feat(types): add thumbnailUrl, channelName, channelThumbnailUrl to FileInfo and DownloadItem"
```

---

## Task 7: CapturedResultsPanel — thumbnail icon + channel sub-line

**Files:**
- Modify: `src/renderer/src/components/CapturedResultsPanel.vue`

- [ ] **Step 1: Replace YouTube SVG icon with thumbnail img**

In the template (around lines 128–141), find the two `<span>` icon elements. Replace the `v-if="row.module?.id === 'youtube'"` branch with an `<img>` when thumbnail is available, falling back to SVG:

```html
<!-- Replace the existing YouTube icon span (lines 128-134) with: -->
<img
  v-if="row.module?.id === 'youtube' && (row.info?.thumbnailUrl || row.loading)"
  :src="row.info?.thumbnailUrl ?? ''"
  class="row-thumb"
  :class="{ 'row-thumb-loading': !row.info?.thumbnailUrl }"
  alt="Thumbnail"
/>
<span
  v-else-if="row.module?.id === 'youtube'"
  class="row-icon provider-row-icon"
  :style="{ color: getProviderIcon(row.module.id).color }"
  aria-label="YouTube"
  role="img"
  v-html="getProviderIcon(row.module.id).svg"
></span>
<span
  v-else
  class="row-icon"
  :class="getFileIcon(row.info?.name ?? row.displayName, row.info?.mimeType, row.info?.isFolder).className"
  :aria-label="getFileIcon(row.info?.name ?? row.displayName, row.info?.mimeType, row.info?.isFolder).alt"
  role="img"
></span>
```

- [ ] **Step 2: Add channel name + avatar to sub-line**

In the `row-sub` div (around lines 158–178), add a channel section after the `row.module?.name` span for YouTube rows:

```html
<!-- Inside .row-sub, after the provider name span: -->
<template v-if="row.module?.id === 'youtube' && row.info?.channelName">
  <span>·</span>
  <span class="row-channel">
    <img
      v-if="row.info?.channelThumbnailUrl"
      :src="row.info.channelThumbnailUrl"
      class="row-channel-avatar"
      alt=""
    />
    <span>{{ row.info.channelName }}</span>
  </span>
</template>
```

- [ ] **Step 3: Add CSS for new elements**

In the `<style scoped>` section, add after `.provider-row-icon :deep(svg)` block:

```css
.row-thumb {
  width: 48px;
  height: 27px;
  border-radius: 4px;
  object-fit: cover;
  flex-shrink: 0;
  background: rgba(126, 139, 164, 0.15);
}

.row-thumb-loading {
  /* shimmer while thumbnail URL is not yet loaded */
  background: linear-gradient(
    90deg,
    rgba(126, 139, 164, 0.12) 25%,
    rgba(126, 139, 164, 0.22) 50%,
    rgba(126, 139, 164, 0.12) 75%
  );
  background-size: 200% 100%;
  animation: shimmer-panel 1.4s infinite;
}

@keyframes shimmer-panel {
  0%   { background-position: -200% 0; }
  100% { background-position:  200% 0; }
}

.row-channel {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--text-muted);
  font-size: 11px;
}

.row-channel-avatar {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  object-fit: cover;
  flex-shrink: 0;
}
```

- [ ] **Step 4: Visual verify — run the app and add a YouTube URL**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
npm run dev 2>&1 | head -5 &
```

Open the link grabber, paste a YouTube URL (e.g. `https://www.youtube.com/watch?v=dQw4w9WgXcQ`), wait for info to load. Verify:
- Thumbnail img (16:9) appears as icon on the left
- Channel avatar (small circle) + channel name appear in the sub-line

- [ ] **Step 5: Commit**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
git add src/renderer/src/components/CapturedResultsPanel.vue
git commit -m "feat(captured-panel): show video thumbnail as icon and channel avatar+name for YouTube rows"
```

---

## Task 8: DownloadList — thumbnail icon + channel line

**Files:**
- Modify: `src/renderer/src/components/DownloadList.vue`

- [ ] **Step 1: Replace provider-icon SVG with thumbnail img for YouTube**

Find the `provider-icon` div in the template (around lines 134–138):

```html
<!-- Replace: -->
<div
  v-if="hasColumn('host')"
  class="provider-icon"
  v-html="getIcon(item.moduleId).svg"
  :style="providerIconStyle(item.moduleId)"
  :title="moduleLabel(item.moduleId)"
></div>

<!-- With: -->
<div
  v-if="hasColumn('host')"
  class="provider-icon"
  :style="item.moduleId !== 'youtube' || !item.thumbnailUrl ? providerIconStyle(item.moduleId) : undefined"
  :title="moduleLabel(item.moduleId)"
>
  <img
    v-if="item.moduleId === 'youtube' && item.thumbnailUrl"
    :src="item.thumbnailUrl"
    class="provider-thumb"
    alt="Thumbnail"
  />
  <span
    v-else
    v-html="getIcon(item.moduleId).svg"
  ></span>
</div>
```

- [ ] **Step 2: Add channel line below the item title**

Find the `item-header` div (around line 144). After the closing `</div>` of `item-header`, and before the `progress-track` div, add:

```html
<!-- Channel line — only shown for YouTube with channelName -->
<div v-if="item.moduleId === 'youtube' && item.channelName" class="item-channel">
  <img
    v-if="item.channelThumbnailUrl"
    :src="item.channelThumbnailUrl"
    class="item-channel-avatar"
    alt=""
  />
  <span class="item-channel-name">{{ item.channelName }}</span>
</div>
```

- [ ] **Step 3: Add CSS for thumbnail provider icon and channel line**

In the `<style scoped>` section, add after the `.provider-icon :deep(svg)` block:

```css
.provider-thumb {
  width: 100%;
  height: 100%;
  object-fit: cover;
  border-radius: 6px;
}

.item-channel {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  color: var(--text-muted);
}

.item-channel-avatar {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  object-fit: cover;
  flex-shrink: 0;
}

.item-channel-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
```

- [ ] **Step 4: Commit**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
git add src/renderer/src/components/DownloadList.vue
git commit -m "feat(download-list): show video thumbnail and channel info for YouTube downloads, add progress transition"
```

---

## Task 9: Fix skeleton card proportions

**Files:**
- Modify: `src/renderer/src/components/DownloadList.vue` (template + CSS)

The current skeleton has 3 thin lines stacked vertically. The actual download card is `display: flex; align-items: flex-start; gap: 12px; padding: 14px` with a left icon (36×36px) and a body column. The skeleton must match this structure.

- [ ] **Step 1: Update skeleton template**

Find the skeleton template block (lines ~115–122):

```html
<!-- BEFORE: -->
<div
  v-for="n in skeletonCount"
  :key="`skeleton-${n}`"
  class="skeleton-card"
>
  <div class="skeleton-line skeleton-title"></div>
  <div class="skeleton-line skeleton-progress"></div>
  <div class="skeleton-line skeleton-meta"></div>
</div>

<!-- AFTER: -->
<div
  v-for="n in skeletonCount"
  :key="`skeleton-${n}`"
  class="skeleton-card"
>
  <div class="skeleton-icon"></div>
  <div class="skeleton-body">
    <div class="skeleton-line skeleton-title"></div>
    <div class="skeleton-line skeleton-progress"></div>
    <div class="skeleton-line skeleton-meta"></div>
  </div>
</div>
```

- [ ] **Step 2: Update skeleton CSS**

Find the `.skeleton-card` CSS block (around line 3606) and replace the entire skeleton section:

```css
/* ── Skeleton cards ─────────────────────────────────────────── */
@keyframes shimmer-skeleton {
  0%   { background-position: -200% 0; }
  100% { background-position:  200% 0; }
}

.skeleton-card {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 14px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  pointer-events: none;
}

.skeleton-icon {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  flex-shrink: 0;
  background: linear-gradient(
    90deg,
    var(--bg-card) 25%,
    color-mix(in srgb, var(--bg-card) 70%, var(--text-muted)) 50%,
    var(--bg-card) 75%
  );
  background-size: 200% 100%;
  animation: shimmer-skeleton 1.4s infinite;
}

.skeleton-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 2px;
}

.skeleton-line {
  border-radius: 6px;
  background: linear-gradient(
    90deg,
    var(--bg-card) 25%,
    color-mix(in srgb, var(--bg-card) 70%, var(--text-muted)) 50%,
    var(--bg-card) 75%
  );
  background-size: 200% 100%;
  animation: shimmer-skeleton 1.4s infinite;
}

.skeleton-title    { height: 14px; width: 55%; }
.skeleton-progress { height: 8px;  width: 100%; }
.skeleton-meta     { height: 10px; width: 35%; }
```

- [ ] **Step 3: Verify skeleton matches card height**

Start the app, add a URL to download queue (so a skeleton appears briefly), confirm the skeleton card is the same height as a regular download card.

- [ ] **Step 4: Commit**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
git add src/renderer/src/components/DownloadList.vue
git commit -m "fix(download-list): skeleton card now matches download card layout with icon placeholder"
```

---

## Task 10: Full build verification

- [ ] **Step 1: Run backend tests**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader/backend
cargo test 2>&1 | tail -20
```

Expected: all existing tests pass. The `parses_ytdlp_progress_with_real_total` and `split_media_phase_progress_does_not_overlap_ranges` tests should still pass since we didn't change `parse_progress` or `phase_progress`.

- [ ] **Step 2: TypeScript check**

```bash
cd /Users/lucasreolon/Desktop/Código/gDownloader
npx tsc --noEmit 2>&1
```

Expected: no errors.

- [ ] **Step 3: Confirm end-to-end with a real YouTube URL**

1. Start app in dev mode
2. Paste `https://www.youtube.com/watch?v=dQw4w9WgXcQ` in Link Grabber
3. Verify: thumbnail shown as icon, channel name shown in sub-line, channel avatar shown (may take a few seconds)
4. Click "Adicionar" to queue the download
5. Verify in DownloadList: thumbnail shown as icon, channel name shown below title
6. Verify progress bar moves smoothly from 0% to 100% without a sudden jump
7. Verify skeleton cards match download card height before the item loads
