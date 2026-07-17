//! Bulk download facility client.

use std::path::{Path, PathBuf};

use futures::stream::{self, StreamExt};
#[cfg(feature = "bulk")]
use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::AsyncWriteExt;
use tracing::debug;

use crate::client::EurostatClient;
use crate::error::Result;
use crate::model::BulkFile;

/// Client for Eurostat bulk download listings.
pub struct BulkClient<'a> {
    client: &'a EurostatClient,
}

impl<'a> BulkClient<'a> {
    /// Create a bulk client.
    pub fn new(client: &'a EurostatClient) -> Self {
        Self { client }
    }

    /// List bulk files from the modern inventory API, with legacy HTML fallback.
    pub async fn list_files(&self) -> Result<Vec<BulkFile>> {
        let lang = self.client.config().language.to_ascii_lowercase();
        let inventory_url = self
            .client
            .http()
            .dissemination_url(&format!("files/inventory?type=data&lang={lang}"));

        match self.client.http().get_bytes(&inventory_url).await {
            Ok(bytes) => parse_inventory_tsv(&String::from_utf8_lossy(&bytes)),
            Err(_) => {
                let html = self
                    .client
                    .http()
                    .get_bytes(&self.client.config().bulk_index_url)
                    .await?;
                parse_legacy_html_index(&String::from_utf8_lossy(&html))
            }
        }
    }

    /// Download a bulk file with optional resume and progress reporting.
    pub async fn download(
        &self,
        file: &BulkFile,
        destination: &Path,
        resume: bool,
        show_progress: bool,
    ) -> Result<PathBuf> {
        let dest = destination.join(suggested_filename(file));
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if resume && dest.exists() {
            debug!(path = %dest.display(), "skipping existing bulk file");
            return Ok(dest);
        }

        let bytes = self.client.http().get_bytes(&file.url).await?;
        #[cfg(feature = "bulk")]
        if show_progress {
            let pb = ProgressBar::new(bytes.len() as u64);
            let style =
                ProgressStyle::default_bar().template("{msg} [{bar:40}] {bytes}/{total_bytes}");
            if let Ok(style) = style {
                pb.set_style(style);
                pb.set_message(file.dataset_id.clone());
                pb.inc(bytes.len() as u64);
                pb.finish_and_clear();
            }
        }

        let mut out = tokio::fs::File::create(&dest).await?;
        out.write_all(&bytes).await?;
        out.flush().await?;
        Ok(dest)
    }

    /// Download multiple files in parallel.
    pub async fn download_many(
        &self,
        files: &[BulkFile],
        destination: &Path,
        resume: bool,
        parallelism: usize,
        show_progress: bool,
    ) -> Result<Vec<PathBuf>> {
        let client = self.client.clone();
        let results = stream::iter(files.iter().cloned())
            .map(|file| {
                let client = client.clone();
                let destination = destination.to_path_buf();
                async move {
                    client
                        .bulk()
                        .download(&file, &destination, resume, show_progress)
                        .await
                }
            })
            .buffer_unordered(parallelism.max(1))
            .collect::<Vec<_>>()
            .await;

        results.into_iter().collect()
    }
}

fn suggested_filename(file: &BulkFile) -> String {
    file.url
        .rsplit('/')
        .next()
        .unwrap_or(&file.dataset_id)
        .to_string()
}

fn parse_inventory_tsv(text: &str) -> Result<Vec<BulkFile>> {
    let mut lines = text.lines();
    let Some(header_line) = lines.next() else {
        return Ok(Vec::new());
    };
    let header_parts = header_line.split('\t').map(str::trim).collect::<Vec<_>>();
    let modern = header_parts
        .first()
        .is_some_and(|h| h.eq_ignore_ascii_case("code"))
        && header_parts.len() > 2;

    let code_idx = header_parts
        .iter()
        .position(|h| h.eq_ignore_ascii_case("code"))
        .unwrap_or(0);
    let type_idx = header_parts
        .iter()
        .position(|h| h.eq_ignore_ascii_case("type"));
    let tsv_idx = header_parts
        .iter()
        .position(|h| h.to_ascii_lowercase().contains("download url (tsv)"));
    let csv_idx = header_parts
        .iter()
        .position(|h| h.to_ascii_lowercase().contains("download url (csv)"));

    let mut files = Vec::new();
    for line in lines {
        let parts = line.split('\t').collect::<Vec<_>>();
        if parts.len() < 2 {
            continue;
        }

        if modern {
            let dataset_id = parts
                .get(code_idx)
                .copied()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .unwrap_or_default()
                .to_string();
            if dataset_id.is_empty() {
                continue;
            }
            if let Some(idx) = type_idx {
                let row_type = parts.get(idx).copied().map(str::trim).unwrap_or_default();
                if !row_type.eq_ignore_ascii_case("dataset") {
                    continue;
                }
            }
            let url = csv_idx
                .and_then(|idx| parts.get(idx).copied())
                .or_else(|| tsv_idx.and_then(|idx| parts.get(idx).copied()))
                .map(str::trim)
                .filter(|url| url.starts_with("http"))
                .unwrap_or_default()
                .to_string();
            if url.is_empty() {
                continue;
            }
            let format = classify_bulk_url(&url);
            files.push(BulkFile {
                dataset_id,
                url,
                format,
                size_bytes: None,
            });
        } else {
            let dataset_id = parts[0].trim().to_string();
            let url = parts[1].trim().to_string();
            if url.is_empty() {
                continue;
            }
            let format = classify_bulk_url(&url);
            files.push(BulkFile {
                dataset_id,
                url,
                format,
                size_bytes: None,
            });
        }
    }
    Ok(files)
}

fn classify_bulk_url(url: &str) -> String {
    if url.contains("format=SDMX-CSV") || url.contains("format=sdmx-csv") {
        "sdmx-csv".to_string()
    } else if url.ends_with(".tsv.gz") {
        "tsv.gz".to_string()
    } else if url.ends_with(".csv.gz") || url.ends_with(".7z") {
        "archive".to_string()
    } else if url.ends_with(".xml.gz") {
        "xml.gz".to_string()
    } else if url.contains("format=TSV") || url.ends_with(".tsv") {
        "tsv".to_string()
    } else {
        "unknown".to_string()
    }
}

fn parse_legacy_html_index(html: &str) -> Result<Vec<BulkFile>> {
    let mut files = Vec::new();
    for line in html.lines() {
        if !line.contains("href=") {
            continue;
        }
        let Some(href) = extract_href(line) else {
            continue;
        };
        if !(href.ends_with(".tsv.gz") || href.ends_with(".csv.gz") || href.ends_with(".xml.gz")) {
            continue;
        }
        let dataset_id = href.rsplit('/').nth(1).unwrap_or("unknown").to_string();
        let format = if href.ends_with(".tsv.gz") {
            "tsv.gz"
        } else if href.ends_with(".csv.gz") {
            "csv.gz"
        } else {
            "xml.gz"
        }
        .to_string();
        files.push(BulkFile {
            dataset_id,
            url: href.to_string(),
            format,
            size_bytes: None,
        });
    }
    Ok(files)
}

fn extract_href(line: &str) -> Option<&str> {
    let start = line.find("href=\"")? + 6;
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inventory_tsv() {
        let tsv = "code\turl\nnama_10_gdp\thttps://example.com/nama_10_gdp.tsv.gz\n";
        let files = parse_inventory_tsv(tsv).expect("parse");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].format, "tsv.gz");
    }

    #[test]
    fn parses_modern_inventory_tsv() {
        let tsv = "Code\tType\tSource dataset\tLast data change\tLast structural change\tData download url (tsv)\tData download url (csv)\n\
nama_10_gdp\tDATASET\t-\tnow\tnow\thttps://example.com/nama?format=TSV\thttps://example.com/nama?format=SDMX-CSV\n\
FOLDER\tFOLDER\t-\t-\t-\t-\t-\n";
        let files = parse_inventory_tsv(tsv).expect("parse");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].dataset_id, "nama_10_gdp");
        assert_eq!(files[0].format, "sdmx-csv");
    }

    #[test]
    fn parses_legacy_html_links() {
        let html = r#"<a href="https://example.com/data/nama/nama_10_gdp.tsv.gz">GDP</a>"#;
        let files = parse_legacy_html_index(html).expect("parse");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].dataset_id, "nama");
    }
}
