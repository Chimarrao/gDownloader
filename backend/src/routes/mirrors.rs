use axum::{
    extract::Query,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{FutureExt, Stream};
use serde::Deserialize;
use std::convert::Infallible;
use std::{future::Future, panic::AssertUnwindSafe, time::Duration};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::mirrors::{
    extract_uploader_tag, is_relevant_result, normalize_filename, score_result,
    searchers::{
        build_client, search_1fichier,
        search_archive_org, search_bing, search_duckduckgo,
        search_filesearching, search_gofile, search_google_hosters,
        search_google_opendir, search_mediafire, search_mega,
        search_pixeldrain, search_rapidgator,
        search_yandex,
    },
    MirrorResult,
};
use std::time::Instant;

#[derive(Deserialize)]
pub struct MirrorSearchParams {
    pub filename: String,
}

// Delay entre cada searcher (ms)
const SEARCHER_DELAY_MS: u64 = 1100;
const SEARCHER_TIMEOUT_SECS: u64 = 45;

struct SearcherDescriptor {
    name: &'static str,
    key: &'static str,
    delay_ms: u64,
    timeout_secs: u64,
}

const SEARCHERS: &[SearcherDescriptor] = &[
    SearcherDescriptor { name: "DuckDuckGo", key: "duckduckgo", delay_ms: SEARCHER_DELAY_MS, timeout_secs: SEARCHER_TIMEOUT_SECS },
    SearcherDescriptor { name: "Bing", key: "bing", delay_ms: SEARCHER_DELAY_MS, timeout_secs: SEARCHER_TIMEOUT_SECS },
    SearcherDescriptor { name: "Yandex", key: "yandex", delay_ms: SEARCHER_DELAY_MS, timeout_secs: SEARCHER_TIMEOUT_SECS },
    SearcherDescriptor { name: "Google (open dir)", key: "google_opendir", delay_ms: SEARCHER_DELAY_MS, timeout_secs: SEARCHER_TIMEOUT_SECS },
    SearcherDescriptor { name: "Google (hosters)", key: "google_hosters", delay_ms: SEARCHER_DELAY_MS, timeout_secs: SEARCHER_TIMEOUT_SECS },
    SearcherDescriptor { name: "Pixeldrain API", key: "pixeldrain", delay_ms: SEARCHER_DELAY_MS, timeout_secs: SEARCHER_TIMEOUT_SECS },
    SearcherDescriptor { name: "Gofile API", key: "gofile", delay_ms: SEARCHER_DELAY_MS, timeout_secs: SEARCHER_TIMEOUT_SECS },
    SearcherDescriptor { name: "Archive.org", key: "archive_org", delay_ms: SEARCHER_DELAY_MS, timeout_secs: SEARCHER_TIMEOUT_SECS },
    SearcherDescriptor { name: "MediaFire", key: "mediafire", delay_ms: SEARCHER_DELAY_MS, timeout_secs: SEARCHER_TIMEOUT_SECS },
    SearcherDescriptor { name: "1Fichier (dork)", key: "1fichier", delay_ms: SEARCHER_DELAY_MS, timeout_secs: SEARCHER_TIMEOUT_SECS },
    SearcherDescriptor { name: "Rapidgator (dork)", key: "rapidgator", delay_ms: SEARCHER_DELAY_MS, timeout_secs: SEARCHER_TIMEOUT_SECS },
    SearcherDescriptor { name: "Mega (dork)", key: "mega", delay_ms: SEARCHER_DELAY_MS, timeout_secs: SEARCHER_TIMEOUT_SECS },
    SearcherDescriptor { name: "FileSearching", key: "filesearching", delay_ms: SEARCHER_DELAY_MS, timeout_secs: SEARCHER_TIMEOUT_SECS },
];

struct SearcherRunSummary {
    results: Vec<MirrorResult>,
    raw_count: usize,
    duration_ms: u64,
}

pub async fn search_mirrors(
    Query(params): Query<MirrorSearchParams>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();

    tokio::spawn(async move {
        let filename = params.filename.clone();
        let (slug, ext) = normalize_filename(&filename);
        let uploader   = extract_uploader_tag(&slug);

        let started_at = Instant::now();

        tracing::info!(
            target: "mirrors",
            "Iniciando busca: filename={:?} slug={:?} uploader={:?}",
            filename, slug, uploader
        );

        let client = build_client();
        let total = SEARCHERS.len();
        let mut all_results: Vec<MirrorResult> = vec![];
        let mut seen_urls: std::collections::HashSet<String> = Default::default();

        // Log inicial
        let _ = tx.send(Ok(event_start(&filename, total)));
        let _ = tx.send(Ok(event_log(&format!(
            "════════════════════════════════════════════════\n  Arquivo: {}\n  Busca:   {}{}\n════════════════════════════════════════════════",
            filename,
            slug,
            uploader.as_deref().map(|t| format!("  [uploader: {}]", t)).unwrap_or_default()
        ))));

        for (idx, descriptor) in SEARCHERS.iter().enumerate() {
            let name = descriptor.name;
            let pad = format!("[{:>2}/{}]", idx + 1, total);
            let _ = tx.send(Ok(event_progress(
                idx + 1,
                total,
                name,
                "running",
                0,
                seen_urls.len(),
                0,
                0,
                0,
                None,
            )));
            let _ = tx.send(Ok(event_log(&format!("{} {}...", pad, name))));

            let raw = match descriptor.key {
                "duckduckgo" => run_searcher(name, descriptor.timeout_secs, search_duckduckgo(&client, &slug, &uploader)).await,
                "bing" => run_searcher(name, descriptor.timeout_secs, search_bing(&client, &slug, &uploader, &ext)).await,
                "yandex" => run_searcher(name, descriptor.timeout_secs, search_yandex(&client, &slug, &uploader)).await,
                "google_opendir" => run_searcher(name, descriptor.timeout_secs, search_google_opendir(&client, &slug, &ext)).await,
                "google_hosters" => run_searcher(name, descriptor.timeout_secs, search_google_hosters(&client, &slug, &uploader)).await,
                "pixeldrain" => run_searcher(name, descriptor.timeout_secs, search_pixeldrain(&client, &slug)).await,
                "gofile" => run_searcher(name, descriptor.timeout_secs, search_gofile(&client, &slug)).await,
                "archive_org" => run_searcher(name, descriptor.timeout_secs, search_archive_org(&client, &slug)).await,
                "mediafire" => run_searcher(name, descriptor.timeout_secs, search_mediafire(&client, &slug)).await,
                "1fichier" => run_searcher(name, descriptor.timeout_secs, search_1fichier(&client, &slug, &uploader)).await,
                "rapidgator" => run_searcher(name, descriptor.timeout_secs, search_rapidgator(&client, &slug, &uploader)).await,
                "mega" => run_searcher(name, descriptor.timeout_secs, search_mega(&client, &slug, &uploader)).await,
                "filesearching" => run_searcher(name, descriptor.timeout_secs, search_filesearching(&client, &slug)).await,
                _ => Ok(SearcherRunSummary {
                    results: vec![],
                    raw_count: 0,
                    duration_ms: 0,
                }),
            };

            let raw = match raw {
                Ok(results) => results,
                Err(reason) => {
                    let _ = tx.send(Ok(event_log(&format!("{} {} — {}", pad, name, reason))));
                    let _ = tx.send(Ok(event_progress(
                        idx + 1,
                        total,
                        name,
                        "completed",
                        0,
                        seen_urls.len(),
                        0,
                        0,
                        0,
                        Some(reason.as_str()),
                    )));
                    tokio::time::sleep(Duration::from_millis(descriptor.delay_ms)).await;
                    continue;
                }
            };

            // Deduplica e filtra resultados pouco relevantes
            let raw_count = raw.raw_count;
            let duration_ms = raw.duration_ms;
            let filtered: Vec<MirrorResult> = raw
                .results
                .into_iter()
                .filter(|r| !is_search_engine_url(&r.url))
                .filter(|r| is_relevant_result(r, &slug, &uploader))
                .map(|mut r| {
                    r.score = score_result(&r, &slug, &uploader);
                    r
                })
                .filter(|r| r.score >= 8)
                .collect();

            let rejected_count = raw_count.saturating_sub(filtered.len());
            let new: Vec<MirrorResult> = filtered
                .into_iter()
                .filter(|r| !seen_urls.contains(&r.url))
                .collect();

            if new.is_empty() {
                let _ = tx.send(Ok(event_log(&format!("{} {} — nada encontrado", pad, name))));
            } else {
                let new_count = new.len();
                let _ = tx.send(Ok(event_log(&format!(
                    "{} {} ✓ {} resultado(s)", pad, name, new_count
                ))));

                for r in &new {
                    seen_urls.insert(r.url.clone());
                    let hoster_tag = r.hoster.as_deref()
                        .map(|h| format!(" [{}]", h))
                        .unwrap_or_default();
                    let mag_tag = if r.url.starts_with("magnet:?") { " [torrent]" } else { "" };

                    let _ = tx.send(Ok(event_log(&format!(
                        "    → {}{}{}", r.url, hoster_tag, mag_tag
                    ))));
                    let _ = tx.send(Ok(event_result(&r.source, &r.url, r.hoster.as_deref(), r.score)));

                    tracing::info!(
                        target: "mirrors",
                        "Encontrado: source={} url={} hoster={:?} score={}",
                        r.source, r.url, r.hoster, r.score
                    );
                }
                all_results.extend(new);

                let _ = tx.send(Ok(event_progress(
                    idx + 1,
                    total,
                    name,
                    "completed",
                    new_count,
                    seen_urls.len(),
                    raw_count,
                    rejected_count,
                    duration_ms,
                    None,
                )));
                tokio::time::sleep(std::time::Duration::from_millis(descriptor.delay_ms)).await;
                continue;
            }

            let _ = tx.send(Ok(event_progress(
                idx + 1,
                total,
                name,
                "completed",
                new.len(),
                seen_urls.len(),
                raw_count,
                rejected_count,
                duration_ms,
                None,
            )));

            tokio::time::sleep(std::time::Duration::from_millis(descriptor.delay_ms)).await;
        }

        // ── Sumário final ─────────────────────────────────────────────────────
        let hosters = all_results.iter().filter(|r| r.hoster.is_some()).count();

        let summary = format!(
            "════════════════════════════════════════════════\n  {} link(s) encontrado(s)  |  hosters: {}  |  duração: {:.1}s\n════════════════════════════════════════════════",
            all_results.len(), hosters, started_at.elapsed().as_secs_f32()
        );
        let _ = tx.send(Ok(event_log(&summary)));

        tracing::info!(
            target: "mirrors",
            "Busca concluída: total={} hosters={}",
            all_results.len(), hosters
        );

        let _ = tx.send(Ok(event_done(
            filename.as_str(),
            total,
            all_results.len(),
            hosters,
            started_at.elapsed().as_millis() as u64,
        )));
    });

    Sse::new(UnboundedReceiverStream::new(rx))
        .keep_alive(KeepAlive::default())
}

// ── Helpers de eventos SSE ────────────────────────────────────────────────────
async fn run_searcher<F>(name: &str, timeout_secs: u64, future: F) -> Result<SearcherRunSummary, String>
where
    F: Future<Output = Vec<MirrorResult>>,
{
    let started_at = Instant::now();
    match tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        AssertUnwindSafe(future).catch_unwind(),
    )
    .await
    {
        Ok(Ok(results)) => Ok(SearcherRunSummary {
            raw_count: results.len(),
            results,
            duration_ms: started_at.elapsed().as_millis() as u64,
        }),
        Ok(Err(_)) => {
            tracing::warn!(target: "mirrors", "Searcher com panic: {}", name);
            Err("falhou internamente, pulando para o próximo".to_string())
        }
        Err(_) => {
            tracing::warn!(target: "mirrors", "Searcher em timeout: {}", name);
            Err("tempo limite excedido, pulando para o próximo".to_string())
        }
    }
}

fn is_search_engine_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("google.com")
        || lower.contains("bing.com")
        || lower.contains("duckduckgo.com")
        || lower.contains("search.yahoo.com")
        || lower.contains("yandex.")
}

fn event_start(filename: &str, total: usize) -> Event {
    Event::default().data(
        serde_json::json!({
            "type": "start",
            "filename": filename,
            "total": total
        })
        .to_string()
    )
}

fn event_progress(
    current: usize,
    total: usize,
    searcher: &str,
    phase: &str,
    new_results: usize,
    total_results: usize,
    raw_results: usize,
    rejected_results: usize,
    duration_ms: u64,
    error: Option<&str>,
) -> Event {
    Event::default().data(
        serde_json::json!({
            "type": "progress",
            "current": current,
            "total": total,
            "searcher": searcher,
            "phase": phase,
            "newResults": new_results,
            "totalResults": total_results,
            "rawResults": raw_results,
            "rejectedResults": rejected_results,
            "durationMs": duration_ms,
            "error": error,
        })
        .to_string()
    )
}

fn event_log(msg: &str) -> Event {
    Event::default().data(
        serde_json::json!({ "type": "log", "payload": msg }).to_string()
    )
}

fn event_result(source: &str, url: &str, hoster: Option<&str>, score: i32) -> Event {
    Event::default().data(
        serde_json::json!({
            "type": "result",
            "url": url,
            "source": source,
            "hoster": hoster,
            "score": score
        })
        .to_string()
    )
}

fn event_done(filename: &str, searchers: usize, total: usize, hosters: usize, duration_ms: u64) -> Event {
    Event::default().data(
        serde_json::json!({
            "type": "done",
            "filename": filename,
            "searchers": searchers,
            "total": total,
            "hosters": hosters,
            "durationMs": duration_ms
        })
        .to_string()
    )
}
