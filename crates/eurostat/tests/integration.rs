use eurostat::parse::json_stat::parse_json_stat;
use eurostat::{Config, EurostatClient};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn catalogue_list_smoke_test() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/catalogue/toc/txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "\"GDP and main components\"\t\"nama_10_gdp\"\t\"dataset\"\t\"08.07.2026\"\n",
        ))
        .mount(&server)
        .await;

    let config = Config {
        base_url: server.uri(),
        ..Default::default()
    };
    let client = EurostatClient::new(config).expect("client");
    let datasets = client.catalogue().list_datasets().await.expect("list");
    assert_eq!(datasets.len(), 1);
    assert_eq!(datasets[0].id, "nama_10_gdp");
}

#[test]
fn json_stat_fixture_roundtrip() {
    let json = include_bytes!("fixtures/nama_10_gdp.json");
    let table = parse_json_stat(json, "nama_10_gdp").expect("parse");
    assert_eq!(table.len(), 4);
}

#[cfg(feature = "export")]
#[test]
fn export_parquet_roundtrip() {
    use eurostat::export::{export_parquet, to_record_batch};
    use eurostat::parse::json_stat::parse_json_stat;

    let json = include_bytes!("fixtures/nama_10_gdp.json");
    let table = parse_json_stat(json, "nama_10_gdp").expect("parse");
    let batch = to_record_batch(&table).expect("batch");
    assert_eq!(batch.num_rows(), 4);

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("out.parquet");
    export_parquet(&table, &path).expect("export");
    assert!(path.exists());
}
