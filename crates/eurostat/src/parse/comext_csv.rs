//! COMEXT bulk CSV parser (comma-separated, dot decimals per Eurostat bulk PDF).

use crate::error::{Error, Result};

/// Parsed COMEXT CSV table with string columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComextTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Parse COMEXT bulk CSV bytes into a columnar table.
pub fn parse_comext_csv(bytes: &[u8]) -> Result<ComextTable> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b',')
        .has_headers(true)
        .flexible(true)
        .from_reader(bytes);

    let headers = reader
        .headers()
        .map_err(|e| Error::Parse(e.to_string()))?
        .iter()
        .map(str::trim)
        .map(str::to_string)
        .collect::<Vec<_>>();

    if headers.is_empty() {
        return Err(Error::Parse("COMEXT CSV missing header".into()));
    }

    let mut rows = Vec::new();
    for result in reader.records() {
        let record = result.map_err(|e| Error::Parse(e.to_string()))?;
        let row = record
            .iter()
            .map(str::trim)
            .map(str::to_string)
            .collect::<Vec<_>>();
        if !row.iter().all(|cell| cell.is_empty()) {
            rows.push(row);
        }
    }

    if rows.is_empty() {
        return Err(Error::Parse("COMEXT CSV contained no data rows".into()));
    }

    Ok(ComextTable { headers, rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comext_csv_fixture() {
        let csv = include_bytes!("../../tests/fixtures/comext_sample.csv");
        let table = parse_comext_csv(csv).expect("parse");
        assert_eq!(table.headers.len(), 7);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0][0], "DE");
    }
}
