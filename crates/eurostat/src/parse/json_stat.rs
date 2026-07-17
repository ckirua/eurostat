//! JSON-stat 2.0 parser.

use crate::error::{Error, Result};
use crate::model::{Dimension, DimensionCode, Observation, ObservationTable};

/// Parse JSON-stat bytes into an [`ObservationTable`].
pub fn parse_json_stat(bytes: &[u8], dataset_id: &str) -> Result<ObservationTable> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let title = value
        .get("label")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let dimension_ids = value
        .get("id")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Parse("JSON-stat missing id array".into()))?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect::<Vec<_>>();

    let mut dimensions = Vec::new();
    let dimension_obj = value
        .get("dimension")
        .and_then(|v| v.as_object())
        .ok_or_else(|| Error::Parse("JSON-stat missing dimension object".into()))?;

    for dim_id in &dimension_ids {
        let dim = dimension_obj
            .get(dim_id)
            .ok_or_else(|| Error::Parse(format!("JSON-stat missing dimension {dim_id}")))?;
        let category = dim
            .get("category")
            .and_then(|v| v.get("index"))
            .and_then(|v| v.as_object())
            .ok_or_else(|| Error::Parse(format!("JSON-stat missing category for {dim_id}")))?;

        let labels = dim
            .get("category")
            .and_then(|v| v.get("label"))
            .and_then(|v| v.as_object());

        let codes = category
            .keys()
            .map(|code| DimensionCode {
                id: code.clone(),
                label: labels
                    .and_then(|map| map.get(code))
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            })
            .collect();

        dimensions.push(Dimension {
            id: dim_id.clone(),
            label: dim
                .get("label")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            codes,
        });
    }

    let sizes = value
        .get("size")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Parse("JSON-stat missing size array".into()))?
        .iter()
        .filter_map(|v| v.as_u64().map(|n| n as usize))
        .collect::<Vec<_>>();

    let values = parse_values(&value)?;

    let status_map = value
        .get("status")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<std::collections::HashMap<_, _>>()
        })
        .unwrap_or_default();

    let mut observations = Vec::new();
    for (index, raw_value) in values {
        if raw_value.is_null() {
            continue;
        }
        let value = match &raw_value {
            serde_json::Value::Number(num) => num.as_f64(),
            _ => None,
        };
        if value.is_none() {
            continue;
        }

        let coords = unravel_index(index, &sizes);
        let mut dim_pairs = Vec::new();
        for (dim_idx, pos) in coords.iter().enumerate() {
            let dim = &dimensions[dim_idx];
            if let Some(code) = dim.codes.get(*pos) {
                dim_pairs.push((dim.id.clone(), code.id.clone()));
            }
        }

        observations.push(Observation {
            dimensions: dim_pairs,
            value,
            status: status_map.get(&index.to_string()).cloned(),
        });
    }

    Ok(ObservationTable {
        dataset_id: dataset_id.to_string(),
        title,
        dimensions,
        observations,
    })
}

/// Parse JSON-stat `value` as either a dense array or a sparse index map.
fn parse_values(root: &serde_json::Value) -> Result<Vec<(usize, serde_json::Value)>> {
    let value = root
        .get("value")
        .ok_or_else(|| Error::Parse("JSON-stat missing value field".into()))?;

    match value {
        serde_json::Value::Array(items) => Ok(items.iter().cloned().enumerate().collect()),
        serde_json::Value::Object(map) => {
            let mut entries = Vec::with_capacity(map.len());
            for (key, raw_value) in map {
                let index = key
                    .parse::<usize>()
                    .map_err(|_| Error::Parse(format!("JSON-stat invalid sparse index: {key}")))?;
                entries.push((index, raw_value.clone()));
            }
            entries.sort_by_key(|(index, _)| *index);
            Ok(entries)
        }
        _ => Err(Error::Parse(
            "JSON-stat value must be an array or sparse object".into(),
        )),
    }
}

fn unravel_index(mut index: usize, sizes: &[usize]) -> Vec<usize> {
    let mut coords = vec![0; sizes.len()];
    for (i, size) in sizes.iter().enumerate().rev() {
        if *size == 0 {
            coords[i] = 0;
            continue;
        }
        coords[i] = index % size;
        index /= size;
    }
    coords
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_json_stat() {
        let json = include_bytes!("../../tests/fixtures/nama_10_gdp.json");
        let table = parse_json_stat(json, "nama_10_gdp").expect("parse");
        assert_eq!(table.dataset_id, "nama_10_gdp");
        assert!(!table.observations.is_empty());
    }

    #[test]
    fn parses_sparse_json_stat_object_values() {
        let json = r#"{
            "version": "2.0",
            "class": "dataset",
            "label": "Sparse GDP",
            "id": ["geo", "time"],
            "size": [1, 2],
            "dimension": {
                "geo": {
                    "category": { "index": { "DE": 0 }, "label": { "DE": "Germany" } }
                },
                "time": {
                    "category": { "index": { "2020": 0, "2021": 1 }, "label": { "2020": "2020", "2021": "2021" } }
                }
            },
            "value": { "0": 100.0, "2": 200.0 }
        }"#;
        let table = parse_json_stat(json.as_bytes(), "nama_10_gdp").expect("parse");
        assert_eq!(table.observations.len(), 2);
    }
}
