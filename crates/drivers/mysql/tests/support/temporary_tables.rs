#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bookie_core::{ConnectOptions, OperationControl};

use crate::{connect, start_mariadb, start_mysql};

fn control() -> OperationControl {
    OperationControl::with_timeout(std::time::Duration::from_secs(10))
}

async fn assert_temporary_table_visibility(options: ConnectOptions, mariadb: bool) {
    let connection = connect(options).await;
    let mut owner = connection.open_session().await.unwrap();
    let operation = control();
    owner
        .query_params_controlled("CREATE TEMPORARY TABLE bookie_temp_catalog (id INT)", &[], &operation)
        .await
        .unwrap();
    let mut other = connection.open_session().await.unwrap();

    let listed = owner.list_temporary_tables_controlled(&operation).await.unwrap();
    let names: Vec<_> = listed
        .iter()
        .map(|table| (table.schema.as_deref(), table.name.as_str()))
        .collect();
    if mariadb {
        assert_eq!(names, [(Some("test"), "bookie_temp_catalog")]);
    } else {
        assert!(names.is_empty(), "MySQL does not expose temporary tables: {names:?}");
    }
    assert!(
        other
            .list_temporary_tables_controlled(&operation)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        other
            .query_params_controlled("SELECT id FROM bookie_temp_catalog", &[], &operation)
            .await
            .is_err()
    );

    owner
        .query_params_controlled("DROP TEMPORARY TABLE bookie_temp_catalog", &[], &operation)
        .await
        .unwrap();
    assert!(
        owner
            .list_temporary_tables_controlled(&operation)
            .await
            .unwrap()
            .is_empty()
    );
    owner.close().await.unwrap();
    other.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires docker"]
async fn temporary_table_catalog_is_session_scoped_on_mysql_and_mariadb() {
    {
        let (_container, options) = start_mysql().await;
        assert_temporary_table_visibility(options, false).await;
    }
    {
        let (_container, options) = start_mariadb().await;
        assert_temporary_table_visibility(options, true).await;
    }
}
