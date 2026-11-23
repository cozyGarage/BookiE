use super::*;

struct Pages(Vec<Vec<Vec<Value>>>);

#[async_trait]
impl RowSource for Pages {
    async fn next_page(&mut self) -> Result<Option<Vec<Vec<Value>>>, DataCompareError> {
        if self.0.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.0.remove(0)))
    }
}

fn column(name: &str, data_type: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: data_type.into(),
        nullable: !primary_key,
        primary_key,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn people() -> Vec<ColumnInfo> {
    vec![column("id", "integer", true), column("name", "text", false)]
}

fn layout() -> Layout {
    plan_layout(&people(), &people(), None).unwrap()
}

fn row(id: i64, name: Option<&str>) -> Vec<Value> {
    vec![
        Value::Int(id),
        name.map_or(Value::Null, |name| Value::Text(name.into())),
    ]
}

async fn run(source: Vec<Vec<Value>>, target: Vec<Vec<Value>>) -> Result<DataCompare, DataCompareError> {
    merge(layout(), Pages(vec![source]), Pages(vec![target])).await
}

#[tokio::test]
async fn classifies_inserts_updates_deletes_and_same_rows() {
    let result = run(
        vec![row(1, Some("a")), row(2, Some("b")), row(4, Some("d"))],
        vec![row(1, Some("a")), row(2, Some("x")), row(3, Some("c"))],
    )
    .await
    .unwrap();
    assert_eq!(
        result.counts,
        Counts {
            same: 1,
            insert: 1,
            update: 1,
            delete: 1
        }
    );
    let changes: Vec<_> = result
        .sample
        .iter()
        .map(|sample| (sample.change, sample.key.clone()))
        .collect();
    assert_eq!(
        changes,
        [
            (Change::Update, vec![Value::Int(2)]),
            (Change::Delete, vec![Value::Int(3)]),
            (Change::Insert, vec![Value::Int(4)])
        ]
    );
    assert_eq!(result.sample[0].columns, ["name"]);
    assert_eq!(
        result.sample_lines("postgres"),
        ["UPDATE (2): name", "DELETE (3)", "INSERT (4)"]
    );
}

#[tokio::test]
async fn a_duplicate_key_on_either_side_is_an_error() {
    let duplicate = vec![row(1, Some("a")), row(1, Some("b"))];
    assert!(matches!(
        run(duplicate.clone(), Vec::new()).await,
        Err(DataCompareError::DuplicateKey(Side::Source))
    ));
    assert!(matches!(
        run(Vec::new(), duplicate).await,
        Err(DataCompareError::DuplicateKey(Side::Target))
    ));
}

#[tokio::test]
async fn a_key_out_of_order_across_pages_is_an_error() {
    let source = Pages(vec![vec![row(2, None)], vec![row(1, None)]]);
    let result = merge(layout(), source, Pages(Vec::new())).await;
    assert!(matches!(result, Err(DataCompareError::KeyOrder(Side::Source))));
}

#[tokio::test]
async fn an_empty_side_makes_every_row_an_insert_or_a_delete() {
    let inserts = run(vec![row(1, None), row(2, None)], Vec::new()).await.unwrap();
    assert_eq!((inserts.counts.insert, inserts.counts.delete), (2, 0));
    let deletes = run(Vec::new(), vec![row(1, None)]).await.unwrap();
    assert_eq!((deletes.counts.insert, deletes.counts.delete), (0, 1));
    assert!(run(Vec::new(), Vec::new()).await.unwrap().is_identical());
}

#[tokio::test]
async fn null_outside_the_key_equals_null_and_differs_from_a_value() {
    let result = run(vec![row(1, None), row(2, None)], vec![row(1, None), row(2, Some(""))])
        .await
        .unwrap();
    assert_eq!((result.counts.same, result.counts.update), (1, 1));
}

#[tokio::test]
async fn typed_equality_keeps_signed_zero_and_nan_and_skips_undecodable_cells() {
    let floats = vec![column("id", "integer", true), column("x", "double precision", false)];
    let layout = plan_layout(&floats, &floats, None).unwrap();
    let source = vec![
        vec![Value::Int(1), Value::Float(f64::NAN)],
        vec![Value::Int(2), Value::Float(-0.0)],
        vec![Value::Int(3), Value::Undecodable("numeric".into())],
    ];
    let target = vec![
        vec![Value::Int(1), Value::Float(f64::NAN)],
        vec![Value::Int(2), Value::Float(0.0)],
        vec![Value::Int(3), Value::Float(1.0)],
    ];
    let result = merge(layout, Pages(vec![source]), Pages(vec![target])).await.unwrap();
    assert_eq!(
        (result.counts.same, result.counts.update, result.unknown_cells),
        (2, 1, 1)
    );
}

#[test]
fn key_choice_uses_the_common_primary_key_or_refuses() {
    assert_eq!(layout().key, ["id"]);
    let keyless = vec![column("id", "integer", false)];
    assert!(matches!(
        plan_layout(&keyless, &keyless, None),
        Err(DataCompareError::NoKey)
    ));
    let nullable = [String::from("name")];
    assert!(matches!(
        plan_layout(&people(), &people(), Some(&nullable)),
        Err(DataCompareError::BadKey(_))
    ));
}

#[test]
fn columns_on_one_side_or_with_another_type_are_not_compared() {
    let source = vec![
        column("id", "integer", true),
        column("name", "text", false),
        column("extra", "text", false),
    ];
    let target = vec![column("id", "integer", true), column("name", "varchar(10)", false)];
    let layout = plan_layout(&source, &target, None).unwrap();
    assert!(layout.compared.is_empty());
    assert_eq!(layout.not_compared, ["name", "extra"]);
}

#[tokio::test]
async fn the_script_leaves_deletes_commented_until_opted_in() {
    let result = run(
        vec![row(1, Some("it's")), row(2, Some("b"))],
        vec![row(2, Some("x")), row(3, Some("c"))],
    )
    .await
    .unwrap();
    let script = result.script("postgres", Some("public"), "people", false).unwrap();
    assert!(script.contains("INSERT INTO \"public\".\"people\" (\"id\", \"name\") VALUES (1, 'it''s');"));
    assert!(script.contains("UPDATE \"public\".\"people\" SET \"name\" = 'b' WHERE \"id\" = 2;"));
    assert!(script.contains("-- DELETE FROM \"public\".\"people\" WHERE \"id\" = 3;"));
    let opted_in = result.script("postgres", Some("public"), "people", true).unwrap();
    assert!(opted_in.contains("\nDELETE FROM \"public\".\"people\" WHERE \"id\" = 3;"));
}

#[test]
fn page_queries_order_text_keys_by_bytes_and_resume_after_the_last_key() {
    let columns = vec![
        column("code", "varchar(10)", true),
        column("n", "integer", true),
        column("v", "text", false),
    ];
    let after = [Value::Text("b".into()), Value::Int(7)];
    let (sql, params) = page_query("postgres", None, "t", &columns, 2, Some(&after));
    assert_eq!(
        sql,
        "SELECT \"code\", \"n\", \"v\" FROM \"t\" WHERE (\"code\" COLLATE \"C\" > $1) OR (\"code\" COLLATE \"C\" = $2 AND \"n\" > $3) ORDER BY \"code\" COLLATE \"C\", \"n\" LIMIT 1000"
    );
    assert_eq!(params, [after[0].clone(), after[0].clone(), after[1].clone()]);
    let (mysql, _) = page_query("mysql", None, "t", &columns, 2, None);
    assert!(mysql.ends_with("ORDER BY CAST(`code` AS BINARY), `n` LIMIT 1000"));
    let (mssql, _) = page_query("mssql", None, "t", &columns, 2, None);
    assert!(
        mssql.ends_with("ORDER BY [code] COLLATE Latin1_General_BIN2, [n] OFFSET 0 ROWS FETCH NEXT 1000 ROWS ONLY")
    );
}

#[tokio::test]
async fn a_batched_script_commits_every_n_statements_and_leaves_commented_deletes_out_of_the_count() {
    let result = run(
        vec![row(1, Some("a")), row(2, Some("b")), row(3, Some("c"))],
        vec![row(4, Some("d"))],
    )
    .await
    .unwrap();
    let script = result
        .batched_script("postgres", None, "people", false, Some(2))
        .unwrap();
    let control: Vec<&str> = script
        .lines()
        .filter(|line| {
            matches!(*line, "BEGIN;" | "COMMIT;") || line.starts_with("INSERT") || line.starts_with("-- DELETE")
        })
        .map(|line| line.split(' ').next().unwrap_or_default())
        .collect();
    assert_eq!(
        control,
        [
            "BEGIN;", "INSERT", "INSERT", "COMMIT;", "BEGIN;", "INSERT", "--", "COMMIT;"
        ]
    );
    let mysql = result.batched_script("mysql", None, "people", true, Some(10)).unwrap();
    assert!(mysql.contains("START TRANSACTION;\n") && mysql.trim_end().ends_with("COMMIT;"));
    assert!(
        !result
            .script("postgres", None, "people", true)
            .unwrap()
            .contains("BEGIN;")
    );
}
