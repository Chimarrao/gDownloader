use axum::{
    extract::Multipart,
    http::StatusCode,
    Json,
};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use tracing::{info, warn};

use crate::models::ApiError;

const MAX_CONTAINER_BYTES: usize = 8 * 1024 * 1024;
const REMOTE_ENDPOINTS: &[&str] = &[
    "https://dlc.piratejd.io/decrypt",
    "https://dlc.piratejd.io/api/decrypt",
    "https://dlc.piratejd.io/api",
    "https://dlc.piratejd.io/",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedContainerLink {
    pub url: String,
    pub filename: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportContainerResponse {
    pub links: Vec<ImportedContainerLink>,
    pub source: String,
}

struct UploadedContainer {
    filename: String,
    bytes: Vec<u8>,
}

pub async fn import_container(
    multipart: Multipart,
) -> Result<Json<ImportContainerResponse>, (StatusCode, Json<ApiError>)> {
    let uploaded = read_container_upload(multipart).await?;
    validate_container_filename(&uploaded.filename)?;

    info!(
        target: "gdownloader_backend::links",
        "importando container filename={} bytes={}",
        uploaded.filename,
        uploaded.bytes.len()
    );

    if let Some(plain_links) = extract_plain_links(&uploaded.bytes) {
        return Ok(Json(ImportContainerResponse {
            links: plain_links,
            source: "plain-text".to_string(),
        }));
    }

    match decrypt_with_remote_service(&uploaded).await {
        Ok(links) if !links.is_empty() => Ok(Json(ImportContainerResponse {
            links,
            source: "dlc.piratejd.io".to_string(),
        })),
        Ok(_) => Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiError::new(
                "O container foi decifrado, mas nenhum link foi encontrado na resposta.",
            )),
        )),
        Err(error) => Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiError::new(format!(
                "Falha ao decifrar o container. O serviço remoto dlc.piratejd.io não respondeu ou rejeitou o arquivo: {error}"
            ))),
        )),
    }
}

async fn read_container_upload(
    mut multipart: Multipart,
) -> Result<UploadedContainer, (StatusCode, Json<ApiError>)> {
    while let Some(field) = multipart.next_field().await.map_err(api_bad_request)? {
        let filename = field
            .file_name()
            .map(str::to_string)
            .unwrap_or_else(|| "container.dlc".to_string());
        let bytes = field.bytes().await.map_err(api_bad_request)?.to_vec();

        if bytes.is_empty() {
            continue;
        }
        if bytes.len() > MAX_CONTAINER_BYTES {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(ApiError::new("Container muito grande. O limite atual é 8 MB.")),
            ));
        }

        return Ok(UploadedContainer { filename, bytes });
    }

    Err((
        StatusCode::BAD_REQUEST,
        Json(ApiError::new("Envie um arquivo .dlc, .ccf ou .rsdf no campo multipart.")),
    ))
}

fn validate_container_filename(filename: &str) -> Result<(), (StatusCode, Json<ApiError>)> {
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".dlc") || lower.ends_with(".ccf") || lower.ends_with(".rsdf") {
        return Ok(());
    }

    Err((
        StatusCode::BAD_REQUEST,
        Json(ApiError::new("Formato não suportado. Envie um arquivo .dlc, .ccf ou .rsdf.")),
    ))
}

async fn decrypt_with_remote_service(uploaded: &UploadedContainer) -> anyhow::Result<Vec<ImportedContainerLink>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(35))
        .build()?;
    let mut last_error = String::from("nenhum endpoint testado");

    for endpoint in REMOTE_ENDPOINTS {
        let part = reqwest::multipart::Part::bytes(uploaded.bytes.clone())
            .file_name(uploaded.filename.clone())
            .mime_str("application/octet-stream")?;
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("filename", uploaded.filename.clone());

        let response = match client.post(*endpoint).multipart(form).send().await {
            Ok(response) => response,
            Err(error) => {
                last_error = error.to_string();
                warn!(
                    target: "gdownloader_backend::links",
                    "falha ao chamar decodificador remoto endpoint={} error={}",
                    endpoint,
                    last_error
                );
                continue;
            }
        };

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            last_error = format!("{} retornou HTTP {}", endpoint, status);
            warn!(
                target: "gdownloader_backend::links",
                "decodificador remoto rejeitou endpoint={} status={}",
                endpoint,
                status
            );
            continue;
        }

        let links = normalize_remote_response(&text);
        if !links.is_empty() {
            info!(
                target: "gdownloader_backend::links",
                "container decifrado endpoint={} links={}",
                endpoint,
                links.len()
            );
            return Ok(links);
        }

        last_error = format!("{} não retornou links reconhecíveis", endpoint);
    }

    anyhow::bail!(last_error)
}

fn normalize_remote_response(text: &str) -> Vec<ImportedContainerLink> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut links = Vec::new();
    if let Ok(json) = serde_json::from_str::<Value>(trimmed) {
        collect_json_links(&json, &mut links);
    }

    links.extend(extract_links_from_text(trimmed));
    dedupe_links(links)
}

fn collect_json_links(value: &Value, links: &mut Vec<ImportedContainerLink>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_json_links(item, links);
            }
        }
        Value::Object(map) => {
            if let Some(url) = map
                .get("url")
                .or_else(|| map.get("link"))
                .or_else(|| map.get("downloadUrl"))
                .and_then(Value::as_str)
                .filter(|url| is_supported_url(url))
            {
                let filename = map
                    .get("filename")
                    .or_else(|| map.get("name"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| filename_from_url(url));
                let size = map
                    .get("size")
                    .or_else(|| map.get("bytes"))
                    .and_then(json_size);
                links.push(ImportedContainerLink {
                    url: url.to_string(),
                    filename,
                    size: size.unwrap_or(0),
                });
            }

            for nested in map.values() {
                collect_json_links(nested, links);
            }
        }
        Value::String(value) if is_supported_url(value) => {
            links.push(link_from_url(value));
        }
        _ => {}
    }
}

fn json_size(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|raw| raw.trim().parse::<u64>().ok()))
}

fn extract_plain_links(bytes: &[u8]) -> Option<Vec<ImportedContainerLink>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let links = extract_links_from_text(text);
    if links.is_empty() {
        None
    } else {
        Some(links)
    }
}

fn extract_links_from_text(text: &str) -> Vec<ImportedContainerLink> {
    let Some(re) = Regex::new(r#"https?://[^\s"'<>\\]+"#).ok() else {
        return Vec::new();
    };

    dedupe_links(
        re.find_iter(text)
            .map(|match_| {
                let url = match_
                    .as_str()
                    .trim_end_matches(|ch: char| matches!(ch, ')' | ']' | ',' | ';' | '.'))
                    .to_string();
                link_from_url(&url)
            })
            .filter(|item| is_supported_url(&item.url))
            .collect(),
    )
}

fn dedupe_links(links: Vec<ImportedContainerLink>) -> Vec<ImportedContainerLink> {
    let mut seen = std::collections::HashSet::new();
    links
        .into_iter()
        .filter(|link| seen.insert(link.url.clone()))
        .collect()
}

fn is_supported_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn link_from_url(url: &str) -> ImportedContainerLink {
    ImportedContainerLink {
        url: url.to_string(),
        filename: filename_from_url(url),
        size: 0,
    }
}

fn filename_from_url(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .path_segments()
                .and_then(|segments| segments.filter(|segment| !segment.is_empty()).last())
                .map(str::to_string)
        })
        .and_then(|raw| urlencoding::decode(&raw).ok().map(|decoded| decoded.into_owned()))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "arquivo".to_string())
}

fn api_bad_request(error: impl std::fmt::Display) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError::new(format!("Falha ao ler upload multipart: {error}"))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_plain_urls() {
        let links = extract_links_from_text(
            "https://example.com/a.bin\nhttps://host.test/path/File%20Name.mkv",
        );

        assert_eq!(links.len(), 2);
        assert_eq!(links[1].filename, "File Name.mkv");
    }

    #[test]
    fn normalizes_json_response_shapes() {
        let links = normalize_remote_response(
            r#"{"links":[{"url":"https://example.com/movie.mkv","filename":"movie.mkv","size":123}]}"#,
        );

        assert_eq!(
            links,
            vec![ImportedContainerLink {
                url: "https://example.com/movie.mkv".to_string(),
                filename: "movie.mkv".to_string(),
                size: 123,
            }]
        );
    }

    #[test]
    fn rejects_unknown_extensions() {
        assert!(validate_container_filename("links.dlc").is_ok());
        assert!(validate_container_filename("links.txt").is_err());
    }
}
