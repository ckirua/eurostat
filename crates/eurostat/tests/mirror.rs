//! Mirror pipeline tests.

use std::io::Write;

use eurostat::export::export_parquet_zstd;
use eurostat::mirror::{select_files, MirrorSource};
use eurostat::model::BulkFile;
use eurostat::parse::comext_csv::parse_comext_csv;
use eurostat::parse::json_stat::parse_json_stat;

#[test]
fn comext_csv_fixture_parses() {
    let csv = include_bytes!("fixtures/comext_sample.csv");
    let table = parse_comext_csv(csv).expect("parse");
    assert_eq!(table.rows.len(), 2);
    assert!(table.headers.contains(&"VALUE_EUR".to_string()));
}

#[cfg(feature = "export")]
#[test]
fn export_parquet_zstd_roundtrip() {
    let json = include_bytes!("fixtures/nama_10_gdp.json");
    let table = parse_json_stat(json, "nama_10_gdp").expect("parse");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("out.parquet.zstd");
    export_parquet_zstd(&table, &path).expect("export");
    assert!(path.exists());
    assert!(path.metadata().expect("meta").len() > 0);
}

#[test]
fn mirror_selects_comext_archives() {
    let files = vec![
        BulkFile {
            dataset_id: "full".into(),
            url: "https://example.com/full_v2_202401.7z".into(),
            format: "archive".into(),
            size_bytes: None,
        },
        BulkFile {
            dataset_id: "nama".into(),
            url: "https://example.com/nama.tsv.gz".into(),
            format: "tsv.gz".into(),
            size_bytes: None,
        },
    ];
    let comext = select_files(&files, MirrorSource::Comext);
    assert_eq!(comext.len(), 1);
    let all = select_files(&files, MirrorSource::All);
    assert_eq!(all.len(), 2);
}

#[cfg(feature = "export")]
#[test]
fn tsv_gz_fixture_converts_to_parquet_zstd() {
    use eurostat::parse::tsv::parse_tsv;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let tsv = b"freq\tgeo\tvalues\nA\tDE\t100.5\n";
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(tsv).expect("write");
    let gz = encoder.finish().expect("finish");
    let mut decoder = flate2::read::GzDecoder::new(&gz[..]);
    let mut raw = Vec::new();
    std::io::Read::read_to_end(&mut decoder, &mut raw).expect("gunzip");
    let table = parse_tsv(&raw, "demo").expect("parse");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("demo.parquet.zstd");
    export_parquet_zstd(&table, &path).expect("export");
    assert!(path.exists());
}
