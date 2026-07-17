//! TSV parser for Eurostat tab-separated responses.

use crate::error::{Error, Result};
use crate::model::{Observation, ObservationTable};

/// Parse Eurostat TSV bytes into an [`ObservationTable`].
pub fn parse_tsv(bytes: &[u8], dataset_id: &str) -> Result<ObservationTable> {
    let text = std::str::from_utf8(bytes).map_err(|e| Error::Parse(e.to_string()))?;
    let mut lines = text.lines().filter(|line| !line.is_empty());
    let header = lines
        .next()
        .ok_or_else(|| Error::Parse("TSV missing header".into()))?;

    let columns = header.split('\t').map(str::trim).collect::<Vec<_>>();
    if columns.len() < 2 {
        return Err(Error::Parse("TSV header must include value column".into()));
    }

    let dimension_cols = &columns[..columns.len() - 1];
    let mut observations = Vec::new();

    for line in lines {
        if line.starts_with("DATAFLOW") || line.starts_with("LAST UPDATE") {
            continue;
        }
        let parts = line.split('\t').collect::<Vec<_>>();
        if parts.len() != columns.len() {
            continue;
        }
        let value = parts.last().and_then(|v| v.trim().parse::<f64>().ok());
        let dims = dimension_cols
            .iter()
            .zip(parts.iter())
            .map(|(dim, code)| ((*dim).to_string(), code.trim().to_string()))
            .collect();
        observations.push(Observation {
            dimensions: dims,
            value,
            status: None,
        });
    }

    Ok(ObservationTable {
        dataset_id: dataset_id.to_string(),
        title: None,
        dimensions: Vec::new(),
        observations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tsv_rows() {
        let data = b"freq\tgeo\tvalues\nA\tDE\t100.5\n";
        let table = parse_tsv(data, "demo").expect("parse");
        assert_eq!(table.observations.len(), 1);
        assert_eq!(table.observations[0].value, Some(100.5));
    }
}
