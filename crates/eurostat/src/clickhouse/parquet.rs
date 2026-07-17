//! Read ZSTD parquet bytes from S3 and normalize into observation rows.

use bytes::Bytes;
use arrow::array::{Array, Float64Array, StringArray};
use arrow::datatypes::DataType;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

use crate::error::{Error, Result};
use crate::mirror::{s3_object_key, S3Config};

use super::normalize::normalize_row;
use super::row::{ObservationRow, ParquetDataset};

/// Fetch a dataset parquet object from S3 and parse into normalized rows.
pub async fn fetch_s3_parquet_dataset(
    s3: &S3Config,
    dataset_id: &str,
    source: &str,
) -> Result<ParquetDataset> {
    let key = s3_object_key(dataset_id, source);
    let bucket = s3.bucket()?;
    let response = bucket
        .get_object(&key)
        .await
        .map_err(|e| Error::S3(e.to_string()))?;
    if !(200..300).contains(&response.status_code()) {
        return Err(Error::NotFound(format!(
            "S3 object not found: {key} (HTTP {})",
            response.status_code()
        )));
    }
    parse_parquet_bytes(dataset_id, &response.to_vec())
}

/// Parse ZSTD-compressed parquet bytes into normalized observation rows.
pub fn parse_parquet_bytes(dataset_id: &str, bytes: &[u8]) -> Result<ParquetDataset> {
    let reader = ParquetRecordBatchReaderBuilder::try_new(Bytes::copy_from_slice(bytes))?
        .build()
        .map_err(|e| Error::Parquet(e))?;

    let mut headers: Option<Vec<String>> = None;
    let mut rows = Vec::new();

    for batch_result in reader {
        let batch = batch_result?;
        let batch_headers: Vec<String> = batch
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect();
        if headers.is_none() {
            headers = Some(batch_headers.clone());
        }

        let row_count = batch.num_rows();
        for row_idx in 0..row_count {
            let values = extract_row_values(&batch, &batch_headers, row_idx)?;
            let normalized = normalize_row(&batch_headers, &values);
            rows.push(ObservationRow::from_normalized(dataset_id, normalized));
        }
    }

    Ok(ParquetDataset {
        dataset_id: dataset_id.to_string(),
        headers: headers.unwrap_or_default(),
        rows,
    })
}

fn extract_row_values(
    batch: &arrow::record_batch::RecordBatch,
    headers: &[String],
    row_idx: usize,
) -> Result<Vec<Option<String>>> {
    let mut values = Vec::with_capacity(headers.len());
    for (col_idx, header) in headers.iter().enumerate() {
        let column = batch.column(col_idx);
        let lower = header.to_ascii_lowercase();
        if lower == "value" || lower == "obs_value" {
            values.push(extract_f64_as_string(column, row_idx)?);
        } else {
            values.push(extract_string(column, row_idx)?);
        }
    }
    Ok(values)
}

fn extract_string(column: &dyn Array, row_idx: usize) -> Result<Option<String>> {
    if column.is_null(row_idx) {
        return Ok(None);
    }
    match column.data_type() {
        DataType::Utf8 => {
            let array = column
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| Error::Parse("expected Utf8 column".into()))?;
            Ok(Some(array.value(row_idx).to_string()))
        }
        DataType::LargeUtf8 => {
            let array = column
                .as_any()
                .downcast_ref::<arrow::array::LargeStringArray>()
                .ok_or_else(|| Error::Parse("expected LargeUtf8 column".into()))?;
            Ok(Some(array.value(row_idx).to_string()))
        }
        DataType::Float64 => extract_f64_as_string(column, row_idx),
        DataType::Int64 => {
            if column.is_null(row_idx) {
                Ok(None)
            } else {
                let array = column
                    .as_any()
                    .downcast_ref::<arrow::array::Int64Array>()
                    .ok_or_else(|| Error::Parse("expected Int64 column".into()))?;
                Ok(Some(array.value(row_idx).to_string()))
            }
        }
        other => Err(Error::Parse(format!(
            "unsupported parquet column type for string extraction: {other:?}"
        ))),
    }
}

fn extract_f64_as_string(column: &dyn Array, row_idx: usize) -> Result<Option<String>> {
    if column.is_null(row_idx) {
        return Ok(None);
    }
    match column.data_type() {
        DataType::Float64 => {
            let array = column
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| Error::Parse("expected Float64 column".into()))?;
            Ok(Some(array.value(row_idx).to_string()))
        }
        DataType::Utf8 => extract_string(column, row_idx),
        other => Err(Error::Parse(format!(
            "unsupported parquet column type for numeric extraction: {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::export::export_parquet_zstd_bytes;
    use crate::mirror::dataset_prefix;
    use crate::model::{Observation, ObservationTable};
    use crate::parse::tsv::parse_tsv;

    #[test]
    fn parses_parquet_bytes_into_observation_rows() {
        let tsv = b"freq\tgeo\tvalues\nA\tDE\t100.5\nA\tFR\t200.0\n";
        let table = parse_tsv(tsv, "demo_gdp").expect("parse");
        let bytes = export_parquet_zstd_bytes(&table).expect("export");
        let dataset = parse_parquet_bytes("demo_gdp", &bytes).expect("parquet");
        assert_eq!(dataset.dataset_id, "demo_gdp");
        assert_eq!(dataset.rows.len(), 2);
        assert_eq!(dataset.rows[0].geo.as_deref(), Some("DE"));
        assert_eq!(dataset.rows[0].prefix, "demo");
        assert_eq!(dataset.rows[0].value, Some(100.5));
        assert_eq!(dataset.rows[0].freq.as_deref(), Some("A"));
    }

    #[test]
    fn observation_row_carries_dimensions() {
        let table = ObservationTable {
            dataset_id: "test_ds".into(),
            observations: vec![Observation {
                dimensions: vec![
                    ("geo".into(), "DE".into()),
                    ("time".into(), "2020".into()),
                    ("indic".into(), "GDP".into()),
                ],
                value: Some(1.0),
                status: None,
            }],
            ..Default::default()
        };
        let bytes = export_parquet_zstd_bytes(&table).expect("export");
        let dataset = parse_parquet_bytes("test_ds", &bytes).expect("parquet");
        assert_eq!(dataset.rows[0].dimensions.get("indic").map(String::as_str), Some("GDP"));
    }

    #[test]
    fn prefix_matches_dataset_prefix_helper() {
        assert_eq!(dataset_prefix("nama_10_gdp"), "nama");
    }
}
