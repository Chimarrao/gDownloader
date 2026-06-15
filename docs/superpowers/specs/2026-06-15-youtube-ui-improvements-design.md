# YouTube UI Improvements — Design Spec
**Date:** 2026-06-15  
**Status:** Approved

## Scope

Four improvements to the YouTube download experience:
1. Channel name + avatar in the captured-links panel and download list
2. Video thumbnail as icon (during search and during download)
3. Fix skeleton card proportions in DownloadList
4. Fix YouTube download progress (jumps to 100% without smooth progression)

---

## 1. Data Model Extensions

### Rust — `FileInfo` (backend model)
Add three optional fields:
- `thumbnail_url: Option<String>` — video thumbnail URL (from yt-dlp `thumbnail` field)
- `channel_name: Option<String>` — channel name (from yt-dlp `uploader`)
- `channel_thumbnail_url: Option<String>` — channel avatar URL (from second yt-dlp call)

### Rust — `Download` / persisted model
Same three fields added as `Option<String>` columns (or just in-memory on the `Download` struct passed to the frontend).

### TypeScript — `FileInfo` (`src/shared/types.ts`)
```typescript
thumbnailUrl?: string
channelName?: string
channelThumbnailUrl?: string
```

### TypeScript — `DownloadItem` (`src/shared/types.ts`)
Same three fields added as optional.

---

## 2. Channel Avatar Fetch (Backend)

In `YouTubeProvider::info_for`, after parsing video JSON:
1. Extract `channel_id` from `YtdlpInfo`
2. Spawn a tokio task (timeout 10s, non-fatal on error) running:
   ```
   yt-dlp -J --flat-playlist --playlist-items 0 --no-warnings
     "https://www.youtube.com/channel/{channel_id}"
   ```
3. Parse `thumbnail` from the resulting JSON → `channel_thumbnail_url`
4. Join the task result before returning `FileInfo`

`YtdlpInfo` additions:
- `uploader: Option<String>`
- `channel_id: Option<String>`

`YtdlpChannelInfo` new struct:
- `thumbnail: Option<String>`
- `title: Option<String>`

---

## 3. CapturedResultsPanel — Thumbnail as Icon

**Loading state:** show a square skeleton placeholder (26×26px) instead of SVG icon while `row.loading`.

**After info loads (YouTube):** replace the provider SVG with `<img src="row.info.thumbnailUrl">` styled as a 40×28px rounded thumbnail (16:9 ratio).

**Sub-line:** add channel avatar `<img>` (16×16px rounded-full) + `row.info.channelName` text after the existing provider name.

---

## 4. DownloadList — Thumbnail as Icon

**Provider icon area (`provider-icon` div):** for `item.moduleId === 'youtube'` and `item.thumbnailUrl`, render `<img>` instead of SVG. Same 40×28px rounded style.

**Below title:** add a `.item-channel` line with avatar img (14×14px) + channel name. Only shown when `item.channelName` is set.

---

## 5. Skeleton Fix

Current skeleton has 3 thin lines that don't match card height. Fix:
- `.skeleton-card` gets `display: flex; align-items: center; gap: 12px; padding: 12px 14px`
- Left: square skeleton block (40×28px) for icon placeholder
- Right column: `.skeleton-line.skeleton-title` (height: 14px, width: 60%), `.skeleton-line.skeleton-progress` (height: 8px, width: 100%), `.skeleton-line.skeleton-meta` (height: 11px, width: 40%)
- Result matches actual download card height

---

## 6. YouTube Progress Fix

**Root cause:** `send_stage("Concluído", final_size, final_size)` at download completion replaces the synthetic scale (total=10,000) with the real file size in bytes, causing the frontend percent calculation to jump from ~85% to 100% without intermediate values.

**Backend fix:** at completion, send `(SYNTHETIC_PROGRESS_TOTAL, SYNTHETIC_PROGRESS_TOTAL)` instead of `(final_size, final_size)`. The actual bytes downloaded is already returned as the function's `Ok(grand_downloaded)` return value — it does not need to be in the progress event.

**Frontend fix:** add `transition: width 0.4s ease` to `.progress-fill` in DownloadList.vue for smooth animation.

---

## Files to Change

### Backend (Rust)
- `backend/src/providers/youtube.rs` — `YtdlpInfo`, `info_for`, channel fetch, progress fix
- `backend/src/models.rs` (or wherever `FileInfo` and `Download` are defined) — add 3 optional fields

### Frontend (TypeScript/Vue)
- `src/shared/types.ts` — extend `FileInfo` and `DownloadItem`
- `src/renderer/src/components/CapturedResultsPanel.vue` — thumbnail icon, channel sub-line, skeleton icon
- `src/renderer/src/components/DownloadList.vue` — thumbnail icon, channel line, skeleton fix, progress transition

---

## Non-Goals
- No YouTube API key integration
- No channel subscriber count or video stats
- No thumbnail caching to disk (URLs used directly)
- No changes to other providers
