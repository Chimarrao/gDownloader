pub mod searchers;

use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;

// ── Hosters conhecidos (usados para detecção e scoring) ──────────────────────
pub static HOSTERS: &[&str] = &[
    // Providers e hosters suportados
    "1fichier.com",
    "rapidgator.net",
    "mega.nz",
    "mediafire.com",
    "pixeldrain.com",
    "gofile.io",
    "katfile.com",
    "katfile.ws",
    "brupload.net",
    "brfiles.com",
    "terabox.com",
    "1024tera.com",
    "akirabox.to",
    // Clouds / arquivos públicos
    "drive.google.com",
    "onedrive.live.com",
    "sharepoint.com",
    "dropbox.com",
    "box.com",
    "archive.org",
    // Transferências genéricas menos controversas
    "wetransfer.com",
    "smash.com",
    "transfernow.net",
    "sendspace.com",
    "filemail.com",
    "multiup.io",
    "multiup.org",
    "workupload.com",
];

static PART_SUFFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\.part\.?\d+$").unwrap());
static FILE_EXT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?ix)^(.+)\.(mkv|avi|mp4|mov|wmv|flv|zip|rar|7z|iso|exe|pdf|epub|ts|m2ts)$")
        .unwrap()
});
static NON_ALNUM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^a-zA-Z0-9]+").unwrap());
static UPLOADER_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9]{2,11}$").unwrap());

// ── Resultado de busca ────────────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize)]
pub struct MirrorResult {
    pub source: String,
    pub url: String,
    pub label: String,
    pub hoster: Option<String>,
    pub score: i32,
}

// ── Normalização de filename ──────────────────────────────────────────────────
/// Remove `.partN`, extensão → devolve (slug_com_espaços, extensão).
/// Ex: "Dirty.Dancing.1987.Bluray.1080p.part5" → ("Dirty Dancing 1987 Bluray 1080p", "")
pub fn normalize_filename(raw: &str) -> (String, String) {
    let name = raw.trim();

    // Remove sufixos .partN / .part.N no final
    let name = PART_SUFFIX_RE.replace(name, "").to_string();

    // Detecta e separa extensão
    let (base, ext) = if let Some(caps) = FILE_EXT_RE.captures(&name) {
        (caps[1].to_string(), caps[2].to_lowercase())
    } else {
        (name.to_string(), String::new())
    };

    let slug = NON_ALNUM_RE.replace_all(&base, " ").trim().to_string();
    (slug, ext)
}

// ── Detecção de hoster a partir de URL ───────────────────────────────────────
pub fn detect_hoster(url: &str) -> Option<String> {
    HOSTERS
        .iter()
        .find(|&&h| url.contains(h))
        .map(|&h| h.to_string())
}

// ── Extração da tag do uploader ───────────────────────────────────────────────
/// Detecta a tag do uploader no nome do arquivo.
/// Padrão: segmento no final que parece um handle (3-12 chars, mix alfanum).
pub fn extract_uploader_tag(slug: &str) -> Option<String> {
    // Palavras que claramente NÃO são tags de uploader
    let noise: &[&str] = &[
        "mkv", "avi", "mp4", "bluray", "bdrip", "brrip", "webrip", "web", "dl",
        "1080p", "720p", "4k", "uhd", "dts", "hd", "5", "1", "2", "ac3", "aac",
        "x264", "x265", "hevc", "remux", "dual", "multi", "pt", "br", "eng",
        "subs", "sub", "dub", "dubbed", "extended", "directors", "cut",
    ];
    let words: Vec<&str> = slug.split_whitespace().collect();
    // Procura de trás para frente o primeiro token que parece uma tag
    for word in words.iter().rev() {
        let w = word.to_lowercase();
        if noise.contains(&w.as_str()) {
            continue;
        }
        // Tag: 3-12 chars, pelo menos uma letra, pode ter dígitos
        if UPLOADER_TAG_RE.is_match(word) {
            return Some(word.to_string());
        }
    }
    None
}

fn haystack_for_result(result: &MirrorResult) -> String {
    format!("{} {} {}", result.url, result.label, result.source).to_lowercase()
}

pub fn core_terms(slug: &str, uploader: &Option<String>) -> Vec<String> {
    let uploader_lower = uploader.as_ref().map(|value| value.to_lowercase());
    let mut terms = Vec::new();

    for word in slug.split_whitespace() {
        let lower = word.to_lowercase();
        if word.len() < 3 || word.parse::<u64>().is_ok() {
            continue;
        }
        if uploader_lower.as_deref() == Some(lower.as_str()) {
            continue;
        }
        if !terms.iter().any(|existing| existing == &lower) {
            terms.push(lower);
        }
        if terms.len() >= 6 {
            break;
        }
    }

    terms
}

pub fn title_hit_count(result: &MirrorResult, slug: &str, uploader: &Option<String>) -> usize {
    let haystack = haystack_for_result(result);
    core_terms(slug, uploader)
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count()
}

pub fn uploader_hit(result: &MirrorResult, uploader: &Option<String>) -> bool {
    let Some(uploader) = uploader else {
        return false;
    };
    haystack_for_result(result).contains(&uploader.to_lowercase())
}

pub fn is_relevant_result(result: &MirrorResult, slug: &str, uploader: &Option<String>) -> bool {
    let haystack = haystack_for_result(result);
    let title_hits = title_hit_count(result, slug, uploader);
    let has_hoster = result.hoster.is_some();
    let has_uploader = uploader_hit(result, uploader);
    let exact_slug = haystack.contains(&slug.to_lowercase());

    if exact_slug {
        return true;
    }

    if uploader.is_some() {
        return (has_uploader && title_hits >= 1)
            || title_hits >= 3
            || (has_hoster && title_hits >= 2);
    }

    title_hits >= 2 || (has_hoster && title_hits >= 1)
}

// ── Scoring de resultados ─────────────────────────────────────────────────────
/// Pontua um resultado com base em quão relevante parece para o arquivo buscado.
pub fn score_result(result: &MirrorResult, slug: &str, uploader: &Option<String>) -> i32 {
    let haystack = haystack_for_result(result);
    let title_hits = title_hit_count(result, slug, uploader) as i32;
    let mut s: i32 = 0;

    if haystack.contains(&slug.to_lowercase()) {
        s += 18;
    }

    if result.hoster.is_some() {
        s += 10;
    }

    s += title_hits * 4;

    if uploader_hit(result, uploader) {
        s += 12;
    }

    if haystack.contains(".mkv")
        || haystack.contains(".mp4")
        || haystack.contains(".zip")
        || haystack.contains(".rar")
    {
        s += 3;
    }

    if title_hits == 0 {
        s -= 8;
    } else if title_hits == 1 {
        s -= 2;
    }

    s
}

// ── Construtor de resultado ───────────────────────────────────────────────────
pub fn make_result(source: &str, url: &str, label: &str) -> MirrorResult {
    let hoster = detect_hoster(url).or_else(|| {
        reqwest::Url::parse(url)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_string))
    });
    let normalized_source = source.trim().replace("  ", " ");
    let normalized_label = if label.trim().is_empty() {
        hoster
            .clone()
            .unwrap_or_else(|| normalized_source.clone())
    } else {
        label.trim().to_string()
    };
    let score = 0; // será recalculado após construção
    MirrorResult {
        source: normalized_source,
        url: url.to_string(),
        label: normalized_label,
        hoster,
        score,
    }
}
