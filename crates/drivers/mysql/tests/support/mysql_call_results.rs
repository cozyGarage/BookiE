use super::{connect, start_mysql};
use tablepro_core::{OperationControl, Value};

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_call_preserves_each_result_set_and_its_columns() {
    let (container, options) = start_mysql().await;
    container.create_multi_result_procedure().await;
    let connection = connect(options).await;

    let batch = connection
        .query_result_sets_controlled(
            "CALL two_selects()",
            &[],
            &OperationControl::with_timeout(std::time::Duration::from_secs(30)),
        )
        .await
        .expect("CALL result sets");

    assert_eq!(batch.result_sets.len(), 2);
    assert_eq!(batch.result_sets[0].columns[0].name, "first_result");
    assert_eq!(batch.result_sets[0].rows, vec![vec![Value::Int(11)]]);
    assert_eq!(batch.result_sets[1].columns[0].name, "second_result");
    assert_eq!(batch.result_sets[1].rows, vec![vec![Value::Text("second".into())]]);
}

#[tokio::test]
#[ignore = "requires docker"]
async fn mysql_text_protocol_retry_returns_execute_rows() {
    let (_container, options) = start_mysql().await;
    let connection = connect(options).await;

    let mut session = connection.open_session().await.expect("open MySQL session");
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
    session
        .query_params_controlled("PREPARE bookie_statement FROM 'SELECT 1 AS value'", &[], &control)
        .await
        .expect("prepare through text protocol");
    let result = session
        .query_params_controlled("EXECUTE bookie_statement", &[], &control)
        .await
        .expect("execute through text protocol");

    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0].name, "value");
    assert_eq!(result.rows, vec![vec![Value::Int(1)]]);
}
