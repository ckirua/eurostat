//! SDMX-CSV parser for Eurostat dissemination bulk downloads.

use crate::error::{Error, Result};
use crate::model::{Observation, ObservationTable};
use crate::parse::comext_csv::ComextTable;

const METADATA_COLUMNS: &[&str] = &["DATAFLOW", "LAST UPDATE"];

/// Parse SDMX-CSV bytes into a columnar table (all columns preserved).
pub fn parse_sdmx_csv_table(bytes: &[u8]) -> Result<ComextTable> {
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
        return Err(Error::Parse("SDMX-CSV missing header".into()));
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
        return Err(Error::Parse("SDMX-CSV contained no data rows".into()));
    }

    Ok(ComextTable { headers, rows })
}

/// Parse SDMX-CSV bytes into an [`ObservationTable`].
pub fn parse_sdmx_csv(bytes: &[u8], dataset_id: &str) -> Result<ObservationTable> {
    let table = parse_sdmx_csv_table(bytes)?;
    let value_idx = table
        .headers
        .iter()
        .position(|h| h.eq_ignore_ascii_case("OBS_VALUE"));
    let status_idx = table
        .headers
        .iter()
        .position(|h| h.eq_ignore_ascii_case("OBS_FLAG") || h.eq_ignore_ascii_case("CONF_STATUS"));

    let dimension_cols = table
        .headers
        .iter()
        .enumerate()
        .filter(|(idx, name)| {
            !METADATA_COLUMNS
                .iter()
                .any(|meta| name.eq_ignore_ascii_case(meta))
                && value_idx != Some(*idx)
                && status_idx != Some(*idx)
        })
        .map(|(_, name)| name.clone())
        .collect::<Vec<_>>();

    let mut observations = Vec::with_capacity(table.rows.len());
    for row in &table.rows {
        let dimensions = table
            .headers
            .iter()
            .zip(row.iter())
            .filter(|(name, _)| dimension_cols.iter().any(|dim| dim == *name))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        let value = value_idx
            .and_then(|idx| row.get(idx))
            .and_then(|v| v.trim().parse::<f64>().ok());
        let status = status_idx
            .and_then(|idx| row.get(idx))
            .filter(|s| !s.is_empty())
            .cloned();
        observations.push(Observation {
            dimensions,
            value,
            status,
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
    fn parses_sdmx_csv_header_and_rows() {
        let csv = b"DATAFLOW,LAST UPDATE,freq,unit,geo,TIME_PERIOD,OBS_VALUE\nESTAT:DEMO,now,A,EUR,DE,2020,100.5\n";
        let table = parse_sdmx_csv_table(csv).expect("parse");
        assert_eq!(table.headers.len(), 7);
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0][6], "100.5");
    }

    #[test]
    fn parses_sdmx_csv_observations() {
        let csv = b"DATAFLOW,LAST UPDATE,freq,unit,geo,TIME_PERIOD,OBS_VALUE\nESTAT:DEMO,now,A,EUR,DE,2020,100.5\n";
        let table = parse_sdmx_csv(csv, "demo").expect("parse");
        assert_eq!(table.observations.len(), 1);
        assert_eq!(table.observations[0].value, Some(100.5));
    }
}
