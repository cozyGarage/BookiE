use tablepro_core::{ConnectOptions, DatabaseDriver, TlsConfig, Value};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::mssql_server::MssqlServer;

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_mssql_csv_import_leaves_server_owned_columns_to_sql_server() {
    let container = MssqlServer::default().with_accept_eula().start().await.unwrap();
    let connection = drivers_mssql::MssqlDriver
        .connect(ConnectOptions {
            host: container.get_host().await.unwrap().to_string(),
            port: container.get_host_port_ipv4(1433).await.unwrap(),
            database: "master".into(),
            username: "sa".into(),
            password: secrecy::SecretString::new(MssqlServer::DEFAULT_SA_PASSWORD.to_string().into()),
            tls: TlsConfig::disabled(),
            ..Default::default()
        })
        .await
        .unwrap();
    connection
        .execute(
            "CREATE TABLE csv_owned (id int IDENTITY(1,1) PRIMARY KEY, note nvarchar(20) NOT NULL DEFAULT N'default', calculated AS (id * 2), version rowversion); INSERT INTO csv_owned (note) VALUES (N'source')",
        )
        .await
        .unwrap();

    let columns = connection.fetch_columns(None, "csv_owned").await.unwrap();
    assert!(columns[0].is_auto_increment);
    assert!(columns[2].is_generated);
    assert!(columns[3].is_generated);
    let options = tablepro_core::import::CsvImportOptions::default();
    let sheet = tablepro_core::import::read_csv(
        b"id,note,calculated,version\n900,first,ignored,ignored\n901,second,ignored,ignored\n",
        &options,
        None,
    )
    .unwrap();
    let mapping = vec![Some(0), Some(1), Some(2), Some(3)];
    let plan = tablepro_core::import::build_insert_plan(
        &tablepro_core::import::ImportTarget {
            driver_id: "mssql",
            schema: None,
            table: "csv_owned",
            columns: &columns,
            mapping: &mapping,
        },
        &sheet,
        &options,
    )
    .unwrap();
    assert_eq!(
        plan.columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["note"]
    );
    assert_eq!(
        plan.rows,
        vec![vec![Value::Text("first".into())], vec![Value::Text("second".into())]]
    );
    assert_eq!(plan.statement, "INSERT INTO [csv_owned] ([note]) VALUES (@P1)");
    for row in &plan.rows {
        connection.execute_params(&plan.statement, row).await.unwrap();
    }

    let result = connection
        .query("SELECT id, note, calculated, DATALENGTH(version) FROM csv_owned ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Text("source".into()),
                Value::Int(2),
                Value::Int(8)
            ],
            vec![Value::Int(2), Value::Text("first".into()), Value::Int(4), Value::Int(8)],
            vec![
                Value::Int(3),
                Value::Text("second".into()),
                Value::Int(6),
                Value::Int(8)
            ],
        ]
    );
}
