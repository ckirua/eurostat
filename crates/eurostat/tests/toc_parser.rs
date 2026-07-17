//! Integration tests for catalogue TOC parsing.

use eurostat::parse::toc_xml::parse_toc_txt;

#[test]
fn parses_real_toc_fixture_excludes_folders() {
    let data = include_bytes!("fixtures/catalogue_toc.txt");
    let datasets = parse_toc_txt(data).expect("parse fixture");

    assert_eq!(datasets.len(), 2);
    assert_eq!(datasets[0].id, "nama_10_gdp");
    assert!(datasets[0].title.contains("Gross domestic product"));
    assert_eq!(datasets[1].id, "ei_bssi_m_r2");
}

#[test]
fn finds_nama_10_gdp_by_id() {
    let data = include_bytes!("fixtures/catalogue_toc.txt");
    let datasets = parse_toc_txt(data).expect("parse fixture");
    let nama = datasets
        .iter()
        .find(|d| d.id == "nama_10_gdp")
        .expect("nama_10_gdp present");
    assert_eq!(nama.updated_at.as_deref(), Some("08.07.2026"));
}
