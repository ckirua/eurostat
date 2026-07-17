//! TOC and catalogue text/XML parsers.

use crate::error::{Error, Result};
use crate::model::DatasetInfo;

/// Parse Eurostat catalogue TOC text format (`catalogue/toc/txt`).
pub fn parse_toc_txt(bytes: &[u8]) -> Result<Vec<DatasetInfo>> {
    let text = std::str::from_utf8(bytes).map_err(|e| Error::Parse(e.to_string()))?;
    let mut datasets = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let fields = line.split('\t').map(strip_quotes).collect::<Vec<_>>();
        if fields.len() < 3 {
            continue;
        }

        // Live API: title, code, type, last update, ...
        if fields[0].eq_ignore_ascii_case("title") && fields[1].eq_ignore_ascii_case("code") {
            continue;
        }

        if fields[2] != "dataset" {
            continue;
        }

        let title = fields[0].trim();
        let id = fields[1].trim();
        if id.is_empty() || title.is_empty() {
            continue;
        }

        datasets.push(DatasetInfo {
            id: id.to_string(),
            title: title.to_string(),
            description: None,
            updated_at: fields.get(3).filter(|v| !v.trim().is_empty()).cloned(),
            url: None,
        });
    }

    if datasets.is_empty() {
        return Err(Error::Parse("catalogue TOC contained no datasets".into()));
    }
    Ok(datasets)
}

fn strip_quotes(field: &str) -> String {
    let trimmed = field.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(trimmed)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_code_first_lines_are_ignored() {
        let data = b"nama_10_gdp\tGDP and main components\tNational accounts\n";
        let err = parse_toc_txt(data).unwrap_err();
        assert!(err.to_string().contains("no datasets"));
    }

    #[test]
    fn parses_live_toc_format() {
        let data = br#""title"	"code"	"type"	"last update of data"
"Database by themes"	"data"	"folder"	" "
"                Gross domestic product (GDP) and main components (output, expenditure and income) - annual data"	"nama_10_gdp"	"dataset"	"08.07.2026"
"#;
        let datasets = parse_toc_txt(data).expect("parse");
        assert_eq!(datasets.len(), 1);
        assert_eq!(datasets[0].id, "nama_10_gdp");
        assert!(datasets[0].title.contains("Gross domestic product"));
        assert_eq!(datasets[0].updated_at.as_deref(), Some("08.07.2026"));
    }
}
