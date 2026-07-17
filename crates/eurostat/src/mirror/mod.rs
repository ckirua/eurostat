//! Bulk mirror pipeline: download Eurostat bulk files and write `.parquet.zstd` datasets.

mod convert;
mod local;
mod manifest;

#[cfg(feature = "s3")]
mod s3;
#[cfg(feature = "s3")]
mod s3_manifest;

use serde::{Deserialize, Serialize};

use crate::model::BulkFile;

pub use convert::{convert_bulk_file_to_parquet, dataset_prefix, s3_object_key};
pub use local::run_mirror;
pub use manifest::{mirror_status, ManifestEntry};

#[cfg(feature = "s3")]
pub use s3::{run_s3_mirror, S3Config};
#[cfg(feature = "s3")]
pub use s3_manifest::{mirror_status_s3, S3_MANIFEST_KEY};

/// Bulk source to mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MirrorSource {
    /// Dissemination `tsv.gz` inventory files.
    Dissemination,
    /// COMEXT `.7z` CSV archives.
    Comext,
    /// Both dissemination and COMEXT sources.
    All,
}

/// Mirror run options.
#[derive(Debug, Clone)]
pub struct MirrorOptions {
    pub source: MirrorSource,
    pub resume: bool,
    pub parallel: usize,
    pub limit: Option<usize>,
}

/// Mirror progress summary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MirrorStatus {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
    pub pending: usize,
}

pub fn select_files(files: &[BulkFile], source: MirrorSource) -> Vec<BulkFile> {
    files
        .iter()
        .filter(|file| match source {
            MirrorSource::Dissemination => is_dissemination_file(file),
            MirrorSource::Comext => is_comext_archive(file),
            MirrorSource::All => is_dissemination_file(file) || is_comext_archive(file),
        })
        .cloned()
        .collect()
}

fn is_dissemination_file(file: &BulkFile) -> bool {
    matches!(file.format.as_str(), "tsv.gz" | "tsv" | "sdmx-csv")
}

pub(crate) fn is_comext_archive(file: &BulkFile) -> bool {
    file.url.ends_with(".7z") || file.format == "archive"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_dissemination_files_only() {
        let files = vec![
            BulkFile {
                dataset_id: "nama".into(),
                url: "https://x/nama.tsv.gz".into(),
                format: "tsv.gz".into(),
                size_bytes: None,
            },
            BulkFile {
                dataset_id: "gdp".into(),
                url: "https://x/gdp?format=SDMX-CSV".into(),
                format: "sdmx-csv".into(),
                size_bytes: None,
            },
            BulkFile {
                dataset_id: "full".into(),
                url: "https://x/full.7z".into(),
                format: "archive".into(),
                size_bytes: None,
            },
        ];
        let selected = select_files(&files, MirrorSource::Dissemination);
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().any(|f| f.format == "tsv.gz"));
        assert!(selected.iter().any(|f| f.format == "sdmx-csv"));
    }
}
