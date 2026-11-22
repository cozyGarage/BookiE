use bookie_core::DriverError;

#[tokio::test]
#[ignore = "requires docker"]
async fn postgres_query_hint_reaches_the_driver_error() {
    let (_container, options) = super::start_pg().await;
    let connection = super::connect(options).await;
    connection.query("CREATE TABLE hint_fixture (name text)").await.unwrap();
    let error = connection.query("SELECT nme FROM hint_fixture").await.unwrap_err();
    assert!(
        matches!(error, DriverError::Query { position: Some(8), hint: Some(hint), .. }
        if hint.contains("hint_fixture.name"))
    );
}
