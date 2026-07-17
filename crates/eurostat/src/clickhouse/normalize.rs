//! Promote common Eurostat columns and pack the rest into a dimension map.

use std::collections::HashMap;

use super::time::parse_time_period;

/// Promoted column values extracted from a parquet row.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PromotedColumns {
    pub geo: Option<String>,
    pub time_code: Option<String>,
    pub freq: Option<String>,
    pub unit: Option<String>,
}

/// Fully normalized observation row ready for ClickHouse insert.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedRow {
    pub geo: Option<String>,
    pub time_code: Option<String>,
    pub time_start: Option<chrono::NaiveDate>,
    pub time_end: Option<chrono::NaiveDate>,
    pub freq: Option<String>,
    pub unit: Option<String>,
    pub dimensions: HashMap<String, String>,
    pub value: Option<f64>,
    pub status: Option<String>,
}

/// Map a parquet column name to a promoted field key, if any.
pub fn normalize_column_name(name: &str) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "geo" | "cntry" | "country" | "partner" | "rep_country" => Some("geo"),
        "time" | "time_period" | "period" => Some("time_code"),
        "freq" | "frequency" => Some("freq"),
        "unit" => Some("unit"),
        "value" | "obs_value" => None,
        "status" => None,
        _ => None,
    }
}

/// Whether a column is a fixed fact column (not a dimension).
pub fn is_fact_column(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    matches!(lower.as_str(), "value" | "obs_value" | "status")
}

/// Normalize a single parquet row into promoted columns + dimension map.
pub fn normalize_row(headers: &[String], values: &[Option<String>]) -> NormalizedRow {
    let mut promoted = PromotedColumns::default();
    let mut dimensions = HashMap::new();
    let mut value = None;
    let mut status = None;

    for (header, cell) in headers.iter().zip(values.iter()) {
        let Some(cell) = cell.as_ref().filter(|s| !s.is_empty()) else {
            continue;
        };
        let lower = header.trim().to_ascii_lowercase();
        if lower == "value" || lower == "obs_value" {
            value = cell.parse().ok();
            continue;
        }
        if lower == "status" {
            status = Some(cell.clone());
            continue;
        }
        if let Some(target) = normalize_column_name(header) {
            match target {
                "geo" => promoted.geo = Some(cell.clone()),
                "time_code" => promoted.time_code = Some(cell.clone()),
                "freq" => promoted.freq = Some(cell.to_ascii_uppercase()),
                "unit" => promoted.unit = Some(cell.clone()),
                _ => {}
            }
        } else {
            dimensions.insert(header.clone(), cell.clone());
        }
    }

    let period = parse_time_period(
        promoted.time_code.as_deref().unwrap_or(""),
        promoted.freq.as_deref(),
    );
    let freq = period
        .freq
        .or(promoted.freq)
        .map(|f| f.to_ascii_uppercase());

    NormalizedRow {
        geo: promoted.geo,
        time_code: promoted.time_code,
        time_start: period.time_start,
        time_end: period.time_end,
        freq,
        unit: promoted.unit,
        dimensions,
        value,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(headers: &[&str], values: &[&str]) -> NormalizedRow {
        let headers: Vec<String> = headers.iter().map(|s| (*s).to_string()).collect();
        let values: Vec<Option<String>> = values.iter().map(|s| Some((*s).to_string())).collect();
        normalize_row(&headers, &values)
    }

    #[test]
    fn maps_column_aliases() {
        assert_eq!(normalize_column_name("GEO"), Some("geo"));
        assert_eq!(normalize_column_name("TIME_PERIOD"), Some("time_code"));
        assert_eq!(normalize_column_name("FREQ"), Some("freq"));
        assert_eq!(normalize_column_name("na_item"), None);
    }

    #[test]
    fn promotes_geo_time_freq_unit() {
        let r = row(
            &["geo", "time", "freq", "unit", "na_item", "value", "status"],
            &["DE", "2020", "A", "EUR", "B1GQ", "100.5", "e"],
        );
        assert_eq!(r.geo.as_deref(), Some("DE"));
        assert_eq!(r.time_code.as_deref(), Some("2020"));
        assert_eq!(r.freq.as_deref(), Some("A"));
        assert_eq!(r.unit.as_deref(), Some("EUR"));
        assert_eq!(r.dimensions.get("na_item").map(String::as_str), Some("B1GQ"));
        assert_eq!(r.value, Some(100.5));
        assert_eq!(r.status.as_deref(), Some("e"));
        assert_eq!(
            r.time_start,
            chrono::NaiveDate::from_ymd_opt(2020, 1, 1)
        );
    }

    #[test]
    fn sdmx_csv_style_headers() {
        let r = row(
            &["GEO", "TIME_PERIOD", "OBS_VALUE", "UNIT"],
            &["FR", "2020-Q1", "42.0", "PC_GDP"],
        );
        assert_eq!(r.geo.as_deref(), Some("FR"));
        assert_eq!(r.time_code.as_deref(), Some("2020-Q1"));
        assert_eq!(r.freq.as_deref(), Some("Q"));
        assert_eq!(r.value, Some(42.0));
    }

    #[test]
    fn packs_non_promoted_into_dimensions() {
        let r = row(&["indic", "sex", "value"], &["EMP", "T", "1.0"]);
        assert_eq!(r.dimensions.len(), 2);
        assert!(r.geo.is_none());
    }
}
