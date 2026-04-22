use regex::Regex;
use reqwest::Client;
use std::sync::LazyLock;

use crate::providers::{DEFAULT_ACCEPT_LANGUAGE, DEFAULT_USER_AGENT};
use super::{make_result, MirrorResult, HOSTERS};

static URL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"https?://[^\s"'<>]+"#).unwrap());
static DDG_RESULT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"uddg=(https?%3A[^&]+)").unwrap());
static BING_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<a[^>]+href="(https?://[^"]+)""#).unwrap());
static YANDEX_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""url":"(https?://[^"]+)""#).unwrap());
static YANDEX_DATA_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"data-url="(https?://[^"]+)""#).unwrap());
static GOOGLE_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/url\?q=(https?://[^&]+)").unwrap());
static MEDIAFIRE_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"href="(https://www\.mediafire\.com/file/[^"]+)""#).unwrap()
});
static FILESEARCHING_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"href="((ftp|https?)://[^"]+)""#).unwrap());

// ── HTTP client compartilhado ─────────────────────────────────────────────────
pub fn build_client() -> Client {
    Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(14))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .unwrap_or_default()
}

// ── Helper: GET com retry ─────────────────────────────────────────────────────
async fn get(client: &Client, url: &str, referer: Option<&str>) -> Option<String> {
    for attempt in 0..2u8 {
        let mut req = client.get(url)
            .header("Accept", "text/html,*/*;q=0.9")
            .header("Accept-Language", DEFAULT_ACCEPT_LANGUAGE);
        if let Some(r) = referer {
            req = req.header("Referer", r);
        }
        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                return resp.text().await.ok();
            }
            Ok(resp) => {
                tracing::debug!("mirrors: HTTP {} → {}", resp.status(), &url[..url.len().min(80)]);
                if attempt == 0 { tokio::time::sleep(std::time::Duration::from_secs(1)).await; }
            }
            Err(e) => {
                tracing::debug!("mirrors: req err → {}: {}", &url[..url.len().min(60)], e);
                if attempt == 0 { tokio::time::sleep(std::time::Duration::from_secs(1)).await; }
            }
        }
    }
    None
}

async fn get_json(client: &Client, url: &str) -> Option<serde_json::Value> {
    for attempt in 0..2u8 {
        match client.get(url)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                return resp.json::<serde_json::Value>().await.ok();
            }
            Ok(resp) => {
                tracing::debug!("mirrors: JSON HTTP {} → {}", resp.status(), &url[..url.len().min(80)]);
                if attempt == 0 { tokio::time::sleep(std::time::Duration::from_secs(1)).await; }
            }
            Err(e) => {
                tracing::debug!("mirrors: JSON err → {}", e);
                if attempt == 0 { tokio::time::sleep(std::time::Duration::from_secs(1)).await; }
            }
        }
    }
    None
}

// ── Helper: extrai URLs de hoster do HTML ─────────────────────────────────────
#[allow(dead_code)]
fn extract_hoster_urls(html: &str, slug_words: &[&str]) -> Vec<String> {
    URL_RE.find_iter(html)
        .map(|m| m.as_str().to_string())
        .filter(|url| {
            let ul = url.to_lowercase();
            // Deve ser de um hoster conhecido
            let is_hoster = HOSTERS.iter().any(|&h| ul.contains(h));
            if !is_hoster { return false; }
            // Pelo menos 1 palavra-chave do slug na URL (filtra resultados não relacionados)
            slug_words.iter().any(|w| ul.contains(&w.to_lowercase()))
        })
        .collect()
}

// ── Helper: URL encode ────────────────────────────────────────────────────────
fn q(s: &str) -> String {
    urlencoding::encode(s).to_string()
}

// ── Multi-query: gera variantes de busca focadas ─────────────────────────────
/// Retorna até 3 queries priorizando uploader + título curto.
pub fn build_queries(slug: &str, uploader: &Option<String>, ext: &str) -> Vec<String> {
    let mut queries = vec![];
    let words: Vec<&str> = slug.split_whitespace().collect();
    let title_words: Vec<&str> = words
        .iter()
        .filter(|w| w.len() >= 3 && w.parse::<u64>().is_err())
        .take(4)
        .copied()
        .collect();
    let short_title = title_words.join(" ");

    if let Some(tag) = uploader {
        queries.push(format!("\"{}\" \"{}\"", slug, tag));
        if !short_title.is_empty() && short_title.to_lowercase() != slug.to_lowercase() {
            queries.push(format!("\"{}\" \"{}\"", short_title, tag));
        }
        if !ext.is_empty() {
            queries.push(format!("\"{}\" \"{}\" filetype:{}", short_title, tag, ext));
        }
    } else {
        queries.push(format!("\"{}\"", slug));
        if !short_title.is_empty() && short_title.to_lowercase() != slug.to_lowercase() {
            queries.push(format!("\"{}\"", short_title));
        }
        if !ext.is_empty() {
            queries.push(format!("\"{}\" filetype:{}", short_title, ext));
        }
    }

    queries
}

// ══════════════════════════════════════════════════════════════════════════════
// SEARCH ENGINES
// ══════════════════════════════════════════════════════════════════════════════

pub async fn search_duckduckgo(
    client: &Client, slug: &str, uploader: &Option<String>,
) -> Vec<MirrorResult> {
    let mut results = vec![];

    for q_str in build_queries(slug, uploader, "").iter().take(2) {
        let url = format!("https://html.duckduckgo.com/html/?q={}", q(q_str));
        let Some(html) = get(client, &url, Some("https://duckduckgo.com/")).await else {
            continue;
        };
        for cap in DDG_RESULT_RE.captures_iter(&html) {
            let href = urlencoding::decode(&cap[1]).unwrap_or_default().to_string();
            if !href.contains("duckduckgo") {
                results.push(make_result("duckduckgo", &href, slug));
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(900)).await;
    }
    results
}

pub async fn search_bing(
    client: &Client, slug: &str, uploader: &Option<String>, ext: &str,
) -> Vec<MirrorResult> {
    let mut results = vec![];

    for q_str in build_queries(slug, uploader, ext).iter().take(2) {
        let url = format!("https://www.bing.com/search?q={}&count=30&mkt=pt-BR", q(q_str));
        let Some(html) = get(client, &url, None).await else { continue; };
        for cap in BING_LINK_RE.captures_iter(&html) {
            let href = cap[1].to_string();
            let href_lower = href.to_lowercase();
            if href_lower.contains("bing.com")
                || href_lower.contains("microsoft.com")
                || href_lower.contains("msn.com")
                || href_lower.contains("live.com")
            {
                continue;
            }
            let h = super::detect_hoster(&href);
            let slug_words: Vec<&str> = slug.split_whitespace().take(3).collect();
            if h.is_some() || slug_words.iter().any(|w| href_lower.contains(&w.to_lowercase())) {
                results.push(make_result("bing", &href, slug));
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(900)).await;
    }
    results
}

pub async fn search_yandex(
    client: &Client, slug: &str, uploader: &Option<String>,
) -> Vec<MirrorResult> {
    let mut results = vec![];

    for q_str in build_queries(slug, uploader, "").iter().take(2) {
        let url = format!(
            "https://yandex.com/search/?text={}&lr=10393",
            q(q_str)
        );
        let Some(html) = get(client, &url, None).await else { continue; };
        for re in [&*YANDEX_URL_RE, &*YANDEX_DATA_URL_RE] {
            for cap in re.captures_iter(&html) {
                let href = cap[1].to_string();
                let href_lower = href.to_lowercase();
                if href_lower.contains("yandex.") {
                    continue;
                }
                if super::detect_hoster(&href).is_some()
                    || slug.split_whitespace().take(2).any(|w| href_lower.contains(&w.to_lowercase()))
                {
                    results.push(make_result("yandex", &href, slug));
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(900)).await;
    }
    results
}

// ── Google dorks (open dir + hosters) ────────────────────────────────────────
async fn google_dork(client: &Client, dork: &str, tag: &str) -> Vec<MirrorResult> {
    let mut results = vec![];
    let url = format!(
        "https://www.google.com/search?q={}&num=20&hl=pt-BR",
        q(dork)
    );
    let Some(html) = get(client, &url, Some("https://www.google.com/")).await else {
        return results;
    };
    for cap in GOOGLE_URL_RE.captures_iter(&html) {
        let href = urlencoding::decode(&cap[1]).unwrap_or_default().to_string();
        if !href.contains("google") {
            results.push(make_result(tag, &href, dork));
        }
    }
    tokio::time::sleep(std::time::Duration::from_millis(2200)).await;
    results
}

pub async fn search_google_opendir(
    client: &Client, slug: &str, ext: &str,
) -> Vec<MirrorResult> {
    let mut results = vec![];
    let dorks = [
        format!("intitle:\"index of\" \"{}\"", slug),
        format!("\"parent directory\" \"{}\"", slug),
        if !ext.is_empty() { format!("intitle:\"index of\" \"{}\" .{}", slug, ext) }
        else { format!("intitle:\"index of\" \"{}\"", slug) },
    ];
    for dork in &dorks {
        results.extend(google_dork(client, dork, "google-opendir").await);
    }
    results
}

pub async fn search_google_hosters(
    client: &Client, slug: &str, uploader: &Option<String>,
) -> Vec<MirrorResult> {
    let mut results = vec![];
    let hoster_q: String = HOSTERS[..14.min(HOSTERS.len())]
        .iter()
        .map(|h| format!("site:{}", h))
        .collect::<Vec<_>>()
        .join(" OR ");

    let title_short = if let Some(tag) = uploader {
        format!("\"{}\" \"{}\"", slug.split_whitespace().take(3).collect::<Vec<_>>().join(" "), tag)
    } else {
        format!("\"{}\"", slug)
    };

    for dork in [
        format!("{} ({})", title_short, hoster_q),
        format!("\"{}\" download link", slug),
    ] {
        results.extend(google_dork(client, &dork, "google-hosters").await);
    }
    results
}

// ══════════════════════════════════════════════════════════════════════════════
// FILE HOST APIs
// ══════════════════════════════════════════════════════════════════════════════

pub async fn search_pixeldrain(client: &Client, slug: &str) -> Vec<MirrorResult> {
    let mut results = vec![];
    let url = format!("https://pixeldrain.com/api/search?q={}&count=20", q(slug));
    let Some(data) = get_json(client, &url).await else { return results; };

    for arr in [data.get("files"), data.get("lists")] {
        let Some(items) = arr.and_then(|v| v.as_array()) else { continue; };
        for item in items {
            let name = item.get("name").or_else(|| item.get("title"))
                .and_then(|v| v.as_str()).unwrap_or("");
            let slug_words: Vec<&str> = slug.split_whitespace().take(3).collect();
            if !slug_words.iter().any(|w| name.to_lowercase().contains(&w.to_lowercase())) {
                continue;
            }
            if let Some(fid) = item.get("id").and_then(|v| v.as_str()) {
                let link = format!("https://pixeldrain.com/u/{}", fid);
                results.push(make_result("pixeldrain.com", &link, name));
            }
        }
    }
    results
}

pub async fn search_gofile(client: &Client, slug: &str) -> Vec<MirrorResult> {
    let mut results = vec![];
    let url = format!("https://api.gofile.io/search?query={}", q(slug));
    let Some(data) = get_json(client, &url).await else { return results; };

    let items = data
        .get("data").and_then(|d| d.get("items"))
        .and_then(|v| v.as_array());
    let Some(items) = items else { return results; };

    for item in items {
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let link = item.get("link").and_then(|v| v.as_str()).unwrap_or("");
        let slug_words: Vec<&str> = slug.split_whitespace().take(3).collect();
        if !link.is_empty() && slug_words.iter().any(|w| name.to_lowercase().contains(&w.to_lowercase())) {
            results.push(make_result("gofile.io", link, name));
        }
    }
    results
}

pub async fn search_archive_org(client: &Client, slug: &str) -> Vec<MirrorResult> {
    let mut results = vec![];
    let url = format!(
        "https://archive.org/advancedsearch.php?q={}&fl[]=identifier&fl[]=title&rows=10&output=json",
        q(slug)
    );
    let Some(data) = get_json(client, &url).await else { return results; };

    let docs = data
        .get("response").and_then(|r| r.get("docs"))
        .and_then(|v| v.as_array());
    let Some(docs) = docs else { return results; };

    let slug_words: Vec<&str> = slug.split_whitespace().take(3).collect();
    for doc in docs {
        let id    = doc.get("identifier").and_then(|v| v.as_str()).unwrap_or("");
        let title = doc.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let id_l  = id.to_lowercase();
        let tl    = title.to_lowercase();
        if slug_words.iter().any(|w| id_l.contains(&w.to_lowercase()) || tl.contains(&w.to_lowercase())) {
            let link = format!("https://archive.org/download/{}", id);
            results.push(make_result("archive.org", &link, title));
        }
    }
    results
}

pub async fn search_mediafire(client: &Client, slug: &str) -> Vec<MirrorResult> {
    let mut results = vec![];
    let url = format!(
        "https://www.mediafire.com/search/?q={}&type=file",
        q(slug)
    );
    let Some(html) = get(client, &url, Some("https://www.mediafire.com/")).await else {
        return results;
    };
    for cap in MEDIAFIRE_LINK_RE.captures_iter(&html) {
        results.push(make_result("mediafire.com", &cap[1], slug));
    }
    results
}

// ── Dork-based hosters ────────────────────────────────────────────────────────
async fn dork_hoster(
    client: &Client, slug: &str, uploader: &Option<String>,
    site: &str, source: &str,
) -> Vec<MirrorResult> {
    let mut results = vec![];
    let title = if let Some(tag) = uploader {
        format!("\"{}\" \"{}\"", slug.split_whitespace().take(3).collect::<Vec<_>>().join(" "), tag)
    } else {
        format!("\"{}\"", slug)
    };
    let dork = format!("site:{} {}", site, title);

    // DDG
    let url = format!("https://html.duckduckgo.com/html/?q={}", q(&dork));
    if let Some(html) = get(client, &url, Some("https://duckduckgo.com/")).await {
        let re = Regex::new(r"uddg=(https?%3A[^&]+)").unwrap();
        for cap in re.captures_iter(&html) {
            let href = urlencoding::decode(&cap[1]).unwrap_or_default().to_string();
            if href.contains(site) {
                results.push(make_result(source, &href, slug));
            }
        }
    }

    // Google backup
    results.extend(google_dork(client, &dork, source).await);

    results
}

pub async fn search_1fichier(client: &Client, slug: &str, uploader: &Option<String>) -> Vec<MirrorResult> {
    dork_hoster(client, slug, uploader, "1fichier.com", "1fichier.com").await
}

pub async fn search_rapidgator(client: &Client, slug: &str, uploader: &Option<String>) -> Vec<MirrorResult> {
    dork_hoster(client, slug, uploader, "rapidgator.net", "rapidgator.net").await
}

pub async fn search_mega(client: &Client, slug: &str, uploader: &Option<String>) -> Vec<MirrorResult> {
    dork_hoster(client, slug, uploader, "mega.nz", "mega.nz").await
}

// ══════════════════════════════════════════════════════════════════════════════
// FILE SEARCH ENGINES
// ══════════════════════════════════════════════════════════════════════════════

pub async fn search_filesearching(client: &Client, slug: &str) -> Vec<MirrorResult> {
    let mut results = vec![];
    let url = format!("https://filesearching.com/search.php?q={}", q(slug));
    let Some(html) = get(client, &url, None).await else { return results; };

    let slug_words: Vec<&str> = slug.split_whitespace().take(3).collect();
    for cap in FILESEARCHING_LINK_RE.captures_iter(&html) {
        let href = &cap[1];
        let href_lower = href.to_lowercase();
        if href_lower.contains("filesearching.com") {
            continue;
        }
        if slug_words.iter().any(|w| href_lower.contains(&w.to_lowercase())) {
            results.push(make_result("filesearching.com", href, slug));
        }
    }
    results
}
