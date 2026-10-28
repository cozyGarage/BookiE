use super::{ColumnInfo, MysqlConnection};
use sqlx::Row;

pub(super) fn is_spatial_type(column_type: &str) -> bool {
    matches!(
        column_type.to_ascii_lowercase().as_str(),
        "geometry"
            | "point"
            | "linestring"
            | "polygon"
            | "multipoint"
            | "multilinestring"
            | "multipolygon"
            | "geometrycollection"
    )
}

impl MysqlConnection {
    pub(super) async fn name_spatial_srids(&self, schema: Option<&str>, table: &str, columns: &mut [ColumnInfo]) {
        let Ok(rows) = sqlx::query(
            "SELECT CAST(column_name AS CHAR), CAST(srs_id AS SIGNED)
             FROM information_schema.st_geometry_columns
             WHERE table_schema = COALESCE(?, DATABASE()) AND table_name = ? AND srs_id IS NOT NULL",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        else {
            return;
        };
        for row in rows {
            let (Ok(name), Ok(srid)) = (row.try_get::<String, _>(0), row.try_get::<i64, _>(1)) else {
                continue;
            };
            if let Some(column) = columns.iter_mut().find(|column| column.name == name) {
                column.data_type = format!("{} SRID {srid}", column.data_type);
            }
        }
    }
}
