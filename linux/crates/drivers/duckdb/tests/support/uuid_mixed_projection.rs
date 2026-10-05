use tablepro_core::Value;

#[tokio::test]
async fn value_contract_query_preserves_uuid_kinds_in_mixed_multirow_projection() {
    let connection = super::native_connection().await;
    let result = connection
        .query(
            "SELECT 'first'::VARCHAR AS label, '12345678-9abc-def0-1122-334455667788'::UUID AS id, \
             NULL::UUID AS missing, '12345678-9abc-def0-1122-334455667788'::VARCHAR AS uuid_text \
             UNION ALL \
             SELECT 'second', '00000000-0000-0000-0000-000000000001'::UUID, NULL::UUID, \
             '00000000-0000-0000-0000-000000000001'::VARCHAR",
        )
        .await
        .unwrap();

    assert_eq!(
        result.rows,
        vec![
            vec![
                Value::Text("first".into()),
                Value::Uuid("12345678-9abc-def0-1122-334455667788".parse().unwrap()),
                Value::Null,
                Value::Text("12345678-9abc-def0-1122-334455667788".into()),
            ],
            vec![
                Value::Text("second".into()),
                Value::Uuid("00000000-0000-0000-0000-000000000001".parse().unwrap()),
                Value::Null,
                Value::Text("00000000-0000-0000-0000-000000000001".into()),
            ],
        ]
    );
}
