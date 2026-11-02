use bookie_core::{DatabaseDriver, Value};
use chrono::{TimeZone, Utc};
use drivers_postgres::PgDriver;

use super::shared_container::start_pg;

#[tokio::test]
#[ignore = "requires docker"]
async fn a_role_time_zone_applies_to_new_sessions_and_timestamptz_values_stay_exact_instants() {
    let (_fixture, options) = start_pg().await;
    let admin = PgDriver.connect(options.clone()).await.unwrap();
    admin
        .execute("ALTER ROLE postgres SET TimeZone TO 'Europe/Vienna'")
        .await
        .unwrap();

    let connection = PgDriver.connect(options).await.unwrap();
    let zone = connection.query("SHOW TimeZone").await.unwrap();
    let near_midnight = connection
        .query("SELECT '2026-10-26 23:30:00+00'::timestamptz::date::text, '2026-10-27 00:30:00+02'::timestamptz")
        .await
        .unwrap();

    assert_eq!(zone.rows, vec![vec![Value::Text("Europe/Vienna".into())]]);
    assert_eq!(near_midnight.rows[0][0], Value::Text("2026-10-27".into()));
    assert_eq!(
        near_midnight.rows[0][1],
        Value::TimestampTz(Utc.with_ymd_and_hms(2026, 10, 26, 22, 30, 0).unwrap())
    );
}
