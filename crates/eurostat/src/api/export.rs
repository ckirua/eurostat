//! Export observation tables to common formats.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use arrow::array::{Float64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use crate::error::Result;
use crate::model::{ObservationTable, OutputFormat};

/// Export an observation table to the requested format.
pub fn export_table(table: &ObservationTable, format: OutputFormat, path: &Path) -> Result<()> {
    match format {
        OutputFormat::Csv => export_csv(table, path),
        OutputFormat::Json => export_json(table, path),
        OutputFormat::Parquet => export_parquet(table, path),
    }
}

/// Export observations to CSV.
pub fn export_csv(table: &ObservationTable, path: &Path) -> Result<()> {
    let mut writer = csv::Writer::from_path(path)?;
    let dimension_names = collect_dimension_names(table);
    let mut headers = dimension_names.clone();
    headers.push("value".to_string());
    headers.push("status".to_string());
    writer.write_record(&headers)?;

    for obs in &table.observations {
        let mut row: Vec<String> = Vec::with_capacity(headers.len());
        for dim in &dimension_names {
            let value = obs
                .dimensions
                .iter()
                .find(|(name, _)| name == dim)
                .map(|(_, code)| code.clone())
                .unwrap_or_default();
            row.push(value);
        }
        row.push(obs.value.map(|v| v.to_string()).unwrap_or_default());
        row.push(obs.status.clone().unwrap_or_default());
        writer.write_record(&row)?;
    }
    writer.flush()?;
    Ok(())
}

/// Export observations to JSON.
pub fn export_json(table: &ObservationTable, path: &Path) -> Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, table)?;
    writer.flush()?;
    Ok(())
}

/// Export observations to Parquet via Arrow.
pub fn export_parquet(table: &ObservationTable, path: &Path) -> Result<()> {
    let batch = to_record_batch(table)?;
    write_record_batch(&batch, path, false)
}

/// Export observations to ZSTD-compressed Parquet (`.parquet.zstd`).
pub fn export_parquet_zstd(table: &ObservationTable, path: &Path) -> Result<()> {
    let bytes = export_parquet_zstd_bytes(table)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Export observations to ZSTD-compressed Parquet bytes.
pub fn export_parquet_zstd_bytes(table: &ObservationTable) -> Result<Vec<u8>> {
    let batch = to_record_batch(table)?;
    write_record_batch_to_vec(&batch, true)
}

/// Export columnar string data to ZSTD-compressed Parquet.
pub fn export_columns_parquet_zstd(
    headers: &[String],
    rows: &[Vec<String>],
    path: &Path,
) -> Result<()> {
    let bytes = export_columns_parquet_zstd_bytes(headers, rows)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Export columnar string data to ZSTD-compressed Parquet bytes.
pub fn export_columns_parquet_zstd_bytes(
    headers: &[String],
    rows: &[Vec<String>],
) -> Result<Vec<u8>> {
    let fields = headers
        .iter()
        .map(|name| Field::new(name, DataType::Utf8, true))
        .collect::<Vec<_>>();
    let schema = Schema::new(fields);
    let mut columns: Vec<arrow::array::ArrayRef> = Vec::with_capacity(headers.len());

    for col_idx in 0..headers.len() {
        let values = rows
            .iter()
            .map(|row| row.get(col_idx).map(|s| s.as_str()))
            .collect::<Vec<_>>();
        columns.push(std::sync::Arc::new(StringArray::from(values)));
    }

    let batch = RecordBatch::try_new(std::sync::Arc::new(schema), columns)?;
    write_record_batch_to_vec(&batch, true)
}

fn write_record_batch(batch: &RecordBatch, path: &Path, zstd: bool) -> Result<()> {
    let bytes = write_record_batch_to_vec(batch, zstd)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

fn write_record_batch_to_vec(batch: &RecordBatch, zstd: bool) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let props = if zstd {
        Some(
            WriterProperties::builder()
                .set_compression(Compression::ZSTD(Default::default()))
                .build(),
        )
    } else {
        None
    };
    let mut writer = ArrowWriter::try_new(&mut buffer, batch.schema(), props)?;
    writer.write(batch)?;
    writer.close()?;
    Ok(buffer)
}

/// Convert an observation table into an Arrow record batch.
pub fn to_record_batch(table: &ObservationTable) -> Result<RecordBatch> {
    let dimension_names = collect_dimension_names(table);
    let mut fields = dimension_names
        .iter()
        .map(|name| Field::new(name, DataType::Utf8, true))
        .collect::<Vec<_>>();
    fields.push(Field::new("value", DataType::Float64, true));
    fields.push(Field::new("status", DataType::Utf8, true));
    let schema = Schema::new(fields);

    let mut columns: Vec<arrow::array::ArrayRef> = Vec::new();
    for dim in &dimension_names {
        let values = table
            .observations
            .iter()
            .map(|obs| {
                obs.dimensions
                    .iter()
                    .find(|(name, _)| name == dim)
                    .map(|(_, code)| code.as_str())
            })
            .collect::<Vec<_>>();
        columns.push(std::sync::Arc::new(StringArray::from(values)));
    }

    let values = table
        .observations
        .iter()
        .map(|obs| obs.value)
        .collect::<Vec<_>>();
    columns.push(std::sync::Arc::new(Float64Array::from(values)));

    let statuses = table
        .observations
        .iter()
        .map(|obs| obs.status.as_deref())
        .collect::<Vec<_>>();
    columns.push(std::sync::Arc::new(StringArray::from(statuses)));

    Ok(RecordBatch::try_new(std::sync::Arc::new(schema), columns)?)
}

fn collect_dimension_names(table: &ObservationTable) -> Vec<String> {
    if !table.dimensions.is_empty() {
        return table.dimensions.iter().map(|d| d.id.clone()).collect();
    }

    let mut names = Vec::new();
    for obs in &table.observations {
        for (name, _) in &obs.dimensions {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    }
    names
}

#[cfg(feature = "polars-export")]
/// Convert to a Polars DataFrame when the feature is enabled.
pub fn to_polars_dataframe(table: &ObservationTable) -> Result<polars::prelude::DataFrame> {
    use polars::prelude::*;

    let dimension_names = collect_dimension_names(table);
    let mut columns: Vec<Column> = Vec::new();

    for dim in dimension_names {
        let values = table
            .observations
            .iter()
            .map(|obs| {
                obs.dimensions
                    .iter()
                    .find(|(name, _)| *name == dim)
                    .map(|(_, code)| code.clone())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        columns.push(Column::new(PlSmallStr::from_str(&dim), values));
    }

    let values = table
        .observations
        .iter()
        .map(|obs| obs.value)
        .collect::<Vec<_>>();
    columns.push(Column::new(PlSmallStr::from_str("value"), values));

    DataFrame::new(columns).map_err(|e| crate::error::Error::Parse(e.to_string()))
}
