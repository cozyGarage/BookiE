use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde_json::json;

use tablepro_core::{ConnectOptions, Connection, OperationControl, Value};

use super::{connect, start_mariadb, start_mysql};

#[path = "../../../../core/tests/support/value_contract.rs"]
mod value_contract;

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_preserves_scalar_boundaries_through_parameters_and_exports() {
    let (_container, options) = start_mysql().await;
    let connection = connect(options).await;
    value_contract::assert_scalar_contract(connection.as_ref(), "mysql").await;
}

async fn connect_with_permissive_dates(options: ConnectOptions) -> Box<dyn Connection> {
    let admin = connect(options.clone()).await;
    admin.execute("SET GLOBAL sql_mode = ''").await.unwrap();
    admin.close().await.unwrap();
    connect(options).await
}

fn micros_time(hour: u32, minute: u32, second: u32, micro: u32) -> NaiveTime {
    NaiveTime::from_hms_micro_opt(hour, minute, second, micro).unwrap()
}

fn text(value: &str) -> Value {
    Value::Text(value.into())
}

#[tokio::test]
#[ignore = "requires docker"]
async fn native_time_zero_date_and_year_values_survive_reads_parameters_and_exports() {
    let (_container, options) = start_mysql().await;
    let conn = connect_with_permissive_dates(options).await;
    let definition = "(id INT PRIMARY KEY, t TIME(6), d DATE, dt DATETIME(6), ts TIMESTAMP(6) NULL, y YEAR)";
    for table in ["native_source", "native_bound", "native_exported"] {
        conn.execute(&format!("CREATE TABLE {table} {definition}"))
            .await
            .unwrap();
    }
    conn.execute(
        "INSERT INTO native_source VALUES
         (1, '-01:00:00', '0000-00-00', '0000-00-00 00:00:00', '0000-00-00 00:00:00', 0),
         (2, '838:59:59', '2024-00-15', '2024-02-00 12:00:00.5', NULL, 2155),
         (3, '-838:59:59', '9999-12-31', '9999-12-31 23:59:59.999999', '2038-01-19 03:14:07.999999', 1901),
         (4, '-00:00:01.5', '1000-01-01', '1000-01-01 00:00:00', '1970-01-01 00:00:01', 2024),
         (5, '24:00:00', NULL, NULL, NULL, NULL),
         (6, '23:59:59.999999', NULL, NULL, NULL, NULL)",
    )
    .await
    .unwrap();

    let result = conn
        .query("SELECT id, t, d, dt, ts, y FROM native_source ORDER BY id")
        .await
        .unwrap();
    let date = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
    let instant = |naive: NaiveDateTime| Utc.from_utc_datetime(&naive);
    let expected = vec![
        vec![
            Value::Int(1),
            text("-01:00:00"),
            text("0000-00-00"),
            text("0000-00-00 00:00:00"),
            text("0000-00-00 00:00:00"),
            Value::Int(0),
        ],
        vec![
            Value::Int(2),
            text("838:59:59"),
            text("2024-00-15"),
            text("2024-02-00 12:00:00.500000"),
            Value::Null,
            Value::Int(2155),
        ],
        vec![
            Value::Int(3),
            text("-838:59:59"),
            Value::Date(date(9999, 12, 31)),
            Value::DateTime(date(9999, 12, 31).and_time(micros_time(23, 59, 59, 999_999))),
            Value::TimestampTz(instant(date(2038, 1, 19).and_time(micros_time(3, 14, 7, 999_999)))),
            Value::Int(1901),
        ],
        vec![
            Value::Int(4),
            text("-00:00:01.500000"),
            Value::Date(date(1000, 1, 1)),
            Value::DateTime(date(1000, 1, 1).and_time(micros_time(0, 0, 0, 0))),
            Value::TimestampTz(instant(date(1970, 1, 1).and_time(micros_time(0, 0, 1, 0)))),
            Value::Int(2024),
        ],
        vec![
            Value::Int(5),
            text("24:00:00"),
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
        ],
        vec![
            Value::Int(6),
            Value::Time(micros_time(23, 59, 59, 999_999)),
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
        ],
    ];
    assert_eq!(result.rows, expected);

    let columns = conn.fetch_columns(None, "native_exported").await.unwrap();
    for row in &result.rows {
        conn.execute_params("INSERT INTO native_bound VALUES (?, ?, ?, ?, ?, ?)", row)
            .await
            .unwrap();
        let statement =
            tablepro_core::sql_literal::build_insert_literal("mysql", None, "native_exported", &columns, row).unwrap();
        conn.execute(&statement).await.unwrap();
    }
    for copy in ["native_bound", "native_exported"] {
        let matching = conn
            .query(&format!(
                "SELECT COUNT(*) FROM native_source s JOIN {copy} c ON s.id = c.id AND s.t <=> c.t \
                 AND s.d <=> c.d AND s.dt <=> c.dt AND s.ts <=> c.ts AND s.y <=> c.y"
            ))
            .await
            .unwrap();
        assert_eq!(matching.rows, vec![vec![Value::Int(6)]], "{copy}");
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn session_non_utc_time_zone_refuses_mysql_timestamp_instant() {
    let (_container, options) = start_mysql().await;
    let conn = connect(options).await;
    conn.execute("CREATE TABLE timezone_source (id INT PRIMARY KEY, instant TIMESTAMP(6) NOT NULL)")
        .await
        .unwrap();

    let mut session = conn.open_session().await.unwrap();
    let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
    session
        .query_params_controlled("SET time_zone = '+05:45'", &[], &control)
        .await
        .unwrap();
    session
        .query_params_controlled(
            "INSERT INTO timezone_source VALUES (1, '2024-01-02 03:04:05.123456')",
            &[],
            &control,
        )
        .await
        .unwrap();
    let result = session
        .query_params_controlled(
            "SELECT instant, CAST(instant AS CHAR) AS session_local, \
             CAST(UNIX_TIMESTAMP(instant) * 1000000 AS SIGNED) AS epoch_micros \
             FROM timezone_source WHERE id = 1",
            &[],
            &control,
        )
        .await
        .unwrap();

    let expected_instant =
        Utc.with_ymd_and_hms(2024, 1, 1, 21, 19, 5).unwrap() + chrono::Duration::microseconds(123_456);
    assert_eq!(result.rows[0][1], text("2024-01-02 03:04:05.123456"));
    assert_eq!(result.rows[0][2], Value::Int(expected_instant.timestamp_micros()));
    assert_eq!(
        result.rows[0][0],
        Value::Undecodable("TIMESTAMP (session time zone is not UTC)".into())
    );

    conn.execute("SET time_zone = '+05:45'").await.unwrap();
    let pooled_result = conn
        .query("SELECT instant FROM timezone_source WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(pooled_result.rows[0][0], Value::TimestampTz(expected_instant));

    let mut tx = conn.begin().await.unwrap();
    tx.execute("SET time_zone = '+05:45'").await.unwrap();
    let transaction_result = tx
        .query("SELECT instant FROM timezone_source WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(
        transaction_result.rows[0][0],
        Value::Undecodable("TIMESTAMP (session time zone is not UTC)".into())
    );
    tx.rollback().await.unwrap();
}

const BACKSLASH_SENSITIVE_TEXTS: [&str; 7] = [
    "a\\b",
    "\\",
    "x\\' OR 1=1 -- ",
    "line\nbreak\\n",
    "nul\0byte",
    "\\0 \\Z \\% \\_ \\\\",
    "plain 'quoted' text",
];

async fn connect_in_sql_mode(options: &ConnectOptions, mode: &str) -> Box<dyn Connection> {
    let admin = connect(options.clone()).await;
    admin.execute(&format!("SET GLOBAL sql_mode = '{mode}'")).await.unwrap();
    admin.close().await.unwrap();
    let conn = connect(options.clone()).await;
    let active = conn.query("SELECT @@SESSION.sql_mode").await.unwrap().rows[0][0].clone();
    let Value::Text(active) = active else {
        panic!("{active:?}")
    };
    let active_modes = active.split(',').map(str::trim).collect::<Vec<_>>();
    let expected_modes = mode.split(',').map(str::trim).filter(|mode| !mode.is_empty());
    for expected_mode in expected_modes {
        assert!(
            active_modes.contains(&expected_mode),
            "mode {expected_mode:?} missing from {active:?}"
        );
    }
    assert_eq!(
        active_modes.contains(&"NO_BACKSLASH_ESCAPES"),
        mode.split(',').any(|mode| mode.trim() == "NO_BACKSLASH_ESCAPES"),
        "NO_BACKSLASH_ESCAPES mode mismatch: {active}"
    );
    conn
}

async fn assert_text_exports_survive_sql_mode(options: &ConnectOptions, mode: &str, suffix: usize) {
    let conn = connect_in_sql_mode(options, mode).await;
    let (source, copy) = (format!("escaped_source_{suffix}"), format!("escaped_copy_{suffix}"));
    conn.execute(&format!(
        "CREATE TABLE {source} (id INT PRIMARY KEY, label TEXT, doc JSON)"
    ))
    .await
    .unwrap();
    conn.execute(&format!("CREATE TABLE {copy} LIKE {source}"))
        .await
        .unwrap();
    let doc = json!({"k": "a\\b\"c\n\0"});
    for (id, label) in BACKSLASH_SENSITIVE_TEXTS.iter().enumerate() {
        conn.execute_params(
            &format!("INSERT INTO {source} VALUES (?, ?, ?)"),
            &[
                Value::Int(id as i64),
                Value::Text((*label).into()),
                Value::Json(doc.clone()),
            ],
        )
        .await
        .unwrap();
    }
    let rows = conn
        .query(&format!("SELECT id, label, doc FROM {source} ORDER BY id"))
        .await
        .unwrap()
        .rows;
    let labels: Vec<Value> = rows.iter().map(|row| row[1].clone()).collect();
    let expected: Vec<Value> = BACKSLASH_SENSITIVE_TEXTS
        .iter()
        .map(|label| Value::Text((*label).into()))
        .collect();
    assert_eq!(labels, expected, "sql_mode '{mode}'");
    let columns = conn.fetch_columns(None, &copy).await.unwrap();
    for row in &rows {
        let statement = tablepro_core::sql_literal::build_insert_literal("mysql", None, &copy, &columns, row).unwrap();
        conn.execute(&statement)
            .await
            .unwrap_or_else(|error| panic!("sql_mode '{mode}': {statement}: {error}"));
    }
    let matching = conn
        .query(&format!(
            "SELECT COUNT(*) FROM {source} s JOIN {copy} c ON s.id = c.id \
             AND HEX(s.label) = HEX(c.label) AND HEX(s.doc) = HEX(c.doc)"
        ))
        .await
        .unwrap();
    assert_eq!(
        matching.rows,
        vec![vec![Value::Int(BACKSLASH_SENSITIVE_TEXTS.len() as i64)]],
        "sql_mode '{mode}'"
    );
}

async fn assert_text_exports_survive_both_backslash_modes(options: ConnectOptions) {
    for (suffix, mode) in [
        "",
        "NO_BACKSLASH_ESCAPES",
        "ANSI_QUOTES",
        "ANSI_QUOTES,NO_BACKSLASH_ESCAPES",
    ]
    .into_iter()
    .enumerate()
    {
        assert_text_exports_survive_sql_mode(&options, mode, suffix).await;
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_text_exports_survive_with_and_without_backslash_escapes() {
    let (_container, options) = start_mysql().await;
    assert_text_exports_survive_both_backslash_modes(options).await;
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_mariadb_text_exports_survive_with_and_without_backslash_escapes() {
    let (_container, options) = start_mariadb().await;
    assert_text_exports_survive_both_backslash_modes(options).await;
}

const PACKED_DEFINITION: &str = "(id INT PRIMARY KEY, flags BIT(8), wide BIT(64), tiny BIT(1), \
     mood ENUM('happy', 'it''s ok', 'ünï', ''), perms SET('read', 'write', 'ëx'), shape GEOMETRY, \
     pin POINT SRID 4326)";

async fn server_bytes(conn: &dyn Connection, sql: &str) -> Vec<Value> {
    let rows = conn.query(sql).await.unwrap().rows;
    rows.into_iter()
        .map(|row| match &row[0] {
            Value::Null => Value::Null,
            Value::Text(hex) => Value::Bytes(
                (0..hex.len())
                    .step_by(2)
                    .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
                    .collect(),
            ),
            other => panic!("{other:?}"),
        })
        .collect()
}

#[tokio::test]
#[ignore = "requires docker"]
async fn value_contract_bit_enum_set_and_geometry_survive_reads_parameters_and_exports() {
    let (_container, options) = start_mysql().await;
    let conn = connect(options).await;
    for table in ["packed_source", "packed_bound", "packed_exported"] {
        conn.execute(&format!("CREATE TABLE {table} {PACKED_DEFINITION}"))
            .await
            .unwrap();
    }
    conn.execute(
        "INSERT INTO packed_source VALUES \
         (1, b'10101010', 0xFFFFFFFFFFFFFFFF, b'1', 'happy', 'read,ëx', \
          ST_GeomFromText('POLYGON((0 0, 1 0, 1 1, 0 0))'), ST_GeomFromText('POINT(52.5 13.4)', 4326)), \
         (2, b'0', 0x8000000000000000, b'0', 'it''s ok', '', \
          ST_GeomFromText('POINT(-1.5 2.25)', 3857), ST_GeomFromText('POINT(0 0)', 4326)), \
         (3, b'1', 0x7FFFFFFFFFFFFFFF, NULL, 'ünï', 'write', NULL, NULL), \
         (4, NULL, 0, NULL, '', 'read,write,ëx', NULL, NULL), \
         (5, NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
    )
    .await
    .unwrap();
    let rows = conn
        .query("SELECT id, flags, wide, tiny, mood, perms, shape, pin FROM packed_source ORDER BY id")
        .await
        .unwrap()
        .rows;
    let column = |index: usize| rows.iter().map(|row| row[index].clone()).collect::<Vec<_>>();
    let ints = |values: [Option<i64>; 5]| values.map(|value| value.map_or(Value::Null, Value::Int)).to_vec();
    assert_eq!(column(1), ints([Some(170), Some(0), Some(1), None, None]));
    assert_eq!(
        column(2),
        vec![
            Value::Bytes(vec![0xff; 8]),
            Value::Bytes(vec![0x80, 0, 0, 0, 0, 0, 0, 0]),
            Value::Int(i64::MAX),
            Value::Int(0),
            Value::Null,
        ]
    );
    assert_eq!(column(3), ints([Some(1), Some(0), None, None, None]));
    let texts = |values: [Option<&str>; 5]| values.map(|value| value.map_or(Value::Null, text)).to_vec();
    assert_eq!(
        column(4),
        texts([Some("happy"), Some("it's ok"), Some("ünï"), Some(""), None])
    );
    assert_eq!(
        column(5),
        texts([Some("read,ëx"), Some(""), Some("write"), Some("read,write,ëx"), None])
    );
    let shapes = server_bytes(conn.as_ref(), "SELECT HEX(shape) FROM packed_source ORDER BY id").await;
    let pins = server_bytes(conn.as_ref(), "SELECT HEX(pin) FROM packed_source ORDER BY id").await;
    assert_eq!(column(6), shapes);
    assert_eq!(column(7), pins);

    let columns = conn.fetch_columns(None, "packed_exported").await.unwrap();
    for row in &rows {
        conn.execute_params("INSERT INTO packed_bound VALUES (?, ?, ?, ?, ?, ?, ?, ?)", row)
            .await
            .unwrap();
        let statement =
            tablepro_core::sql_literal::build_insert_literal("mysql", None, "packed_exported", &columns, row).unwrap();
        conn.execute(&statement)
            .await
            .unwrap_or_else(|error| panic!("{statement}: {error}"));
    }
    for copy in ["packed_bound", "packed_exported"] {
        let matching = conn
            .query(&format!(
                "SELECT COUNT(*) FROM packed_source s JOIN {copy} c ON s.id = c.id \
                 AND s.flags <=> c.flags AND s.wide <=> c.wide AND s.tiny <=> c.tiny \
                 AND s.mood + 0 <=> c.mood + 0 AND s.perms + 0 <=> c.perms + 0 \
                 AND HEX(s.shape) <=> HEX(c.shape) AND HEX(s.pin) <=> HEX(c.pin)"
            ))
            .await
            .unwrap();
        assert_eq!(matching.rows, vec![vec![Value::Int(5)]], "{copy}");
    }

    conn.execute("CREATE TABLE packed_edit (id INT PRIMARY KEY, flags BIT(8), tiny BIT(1))")
        .await
        .unwrap();
    conn.execute("INSERT INTO packed_edit VALUES (1, b'10101010', b'1')")
        .await
        .unwrap();
    conn.execute_params(
        "UPDATE packed_edit SET flags = ?, tiny = ? WHERE id = ?",
        &[Value::Int(85), Value::Bool(false), Value::Int(1)],
    )
    .await
    .unwrap();
    let edited = conn
        .query("SELECT flags, tiny FROM packed_edit WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(edited.rows, vec![vec![Value::Int(85), Value::Int(0)]]);
}
