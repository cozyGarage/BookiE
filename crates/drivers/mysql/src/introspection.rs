//! Catalog reads: collations, databases, columns, indexes, foreign keys and server version.

use super::*;

pub(crate) async fn read_list_collations(
    connection: &mut sqlx::MySqlConnection,
) -> Result<Vec<bookie_core::CollationInfo>, DriverError> {
    let rows = sqlx::query(
        "SELECT CAST(collation_name AS CHAR), COALESCE(CAST(character_set_name AS CHAR), ''),
                COALESCE(is_default = 'Yes', 0)
         FROM information_schema.collations
         ORDER BY collation_name
         LIMIT ?",
    )
    .bind(bookie_core::MAX_COLLATIONS as i64)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?;
    Ok(rows
        .into_iter()
        .map(|row| bookie_core::CollationInfo {
            name: row.get(0),
            charset: row.get(1),
            is_default: row.get(2),
        })
        .collect())
}

pub(crate) async fn read_list_databases(connection: &mut sqlx::MySqlConnection) -> Result<Vec<String>, DriverError> {
    let rows = sqlx::query(
        "SELECT CAST(schema_name AS CHAR) FROM information_schema.schemata
         WHERE schema_name NOT IN ('information_schema', 'mysql', 'performance_schema', 'sys')
         ORDER BY schema_name",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?;
    Ok(rows.into_iter().map(|row| row.get::<String, _>(0)).collect())
}

pub(crate) async fn read_fetch_columns(
    connection: &mut sqlx::MySqlConnection,
    schema: Option<&str>,
    table: &str,
) -> Result<Vec<ColumnInfo>, DriverError> {
    // `column_type` is canonical: it carries the precision /
    // length the user typed (`tinyint(1)`, `varchar(255)`,
    // `decimal(10,2)`, `enum('a','b')`). `data_type` strips all
    // that — returns `tinyint` for both `tinyint(1)` and
    // `tinyint(4)`, which collapses MySQL's idiomatic boolean
    // type into a generic int and breaks the bool-detection
    // heuristic in `classify_type`. Prefer column_type for the
    // displayed `data_type`.
    let rows = sqlx::query(
        "SELECT CAST(column_name AS CHAR), CAST(column_type AS CHAR),
                CAST(is_nullable AS CHAR), CAST(column_key AS CHAR),
                CAST(extra AS CHAR), CAST(column_default AS CHAR),
                CAST(generation_expression AS CHAR),
                CAST(column_comment AS CHAR),
                CAST(collation_name AS CHAR),
                CAST(VERSION() LIKE '%MariaDB%' AS SIGNED)
         FROM information_schema.columns
         WHERE table_schema = COALESCE(?, DATABASE()) AND table_name = ?
         ORDER BY ordinal_position",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?;
    let mut columns: Vec<ColumnInfo> = rows.iter().map(row_to_column_info).collect();
    if columns.iter().any(|column| spatial::is_spatial_type(&column.data_type)) {
        spatial::name_spatial_srids(connection, schema, table, &mut columns).await;
    }
    Ok(columns)
}

pub(crate) async fn read_fetch_indexes(
    connection: &mut sqlx::MySqlConnection,
    schema: Option<&str>,
    table: &str,
) -> Result<Vec<IndexInfo>, DriverError> {
    // information_schema.statistics returns one row per (index,
    // column). Group rows by index_name in Rust because sqlx can't
    // GROUP_CONCAT-then-split natively for ordered column lists.
    // PRIMARY is the literal index name MySQL uses for the PK.
    let rows = sqlx::query(
        "SELECT
            CAST(index_name AS CHAR) AS index_name,
            non_unique,
            CAST(column_name AS CHAR) AS column_name,
            CAST(seq_in_index AS SIGNED) AS seq_in_index
        FROM information_schema.statistics
        WHERE table_schema = COALESCE(?, DATABASE())
          AND table_name = ?
        ORDER BY index_name, seq_in_index",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?;
    let mut by_name: std::collections::BTreeMap<String, IndexInfo> = std::collections::BTreeMap::new();
    for r in rows {
        let name: String = r.try_get(0).map_err(map_sqlx_error)?;
        let non_unique: i64 = r.try_get(1).unwrap_or(0);
        let column: Option<String> = r.try_get(2).map_err(map_sqlx_error)?;
        let expression = column.is_none();
        let key = match column {
            Some(column) => column,
            None => {
                index_key_expression(connection, schema, table, &name, r.try_get(3).map_err(map_sqlx_error)?).await?
            }
        };
        let entry = by_name.entry(name.clone()).or_insert_with(|| IndexInfo {
            constraint: None,
            name: name.clone(),
            columns: Vec::new(),
            unique: non_unique == 0,
            primary: name == "PRIMARY",
            predicate: None,
            has_expression_key: false,
            using_clause: None,
        });
        entry.has_expression_key |= expression;
        entry.columns.push(key);
    }
    Ok(by_name.into_values().collect())
}

pub(crate) async fn read_fetch_foreign_keys(
    connection: &mut sqlx::MySqlConnection,
    schema: Option<&str>,
    table: &str,
) -> Result<Vec<ForeignKeyInfo>, DriverError> {
    // key_column_usage gives us the FK column ↔ referenced column
    // pairs (one row per (constraint, ordinal)); referential_constraints
    // adds the ON DELETE / ON UPDATE rules. Group by constraint_name
    // in Rust to assemble the column lists.
    let rows = sqlx::query(
        "SELECT
            CAST(kcu.constraint_name AS CHAR),
            CAST(kcu.column_name AS CHAR),
            CAST(kcu.referenced_table_name AS CHAR),
            CAST(kcu.referenced_table_schema AS CHAR),
            CAST(kcu.referenced_column_name AS CHAR),
            CAST(rc.delete_rule AS CHAR),
            CAST(rc.update_rule AS CHAR)
        FROM information_schema.key_column_usage kcu
        JOIN information_schema.referential_constraints rc
            ON rc.constraint_name = kcu.constraint_name
            AND rc.constraint_schema = kcu.constraint_schema
        WHERE kcu.table_schema = COALESCE(?, DATABASE())
          AND kcu.table_name = ?
          AND kcu.referenced_table_name IS NOT NULL
        ORDER BY kcu.constraint_name, kcu.ordinal_position",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?;
    let mut by_name: std::collections::BTreeMap<String, ForeignKeyInfo> = std::collections::BTreeMap::new();
    for r in rows {
        let name: String = r.get(0);
        let column: String = r.get(1);
        let ref_table: String = r.get(2);
        let ref_schema: Option<String> = r.try_get(3).unwrap_or(None);
        let ref_column: String = r.get(4);
        let delete_rule: Option<String> = r.try_get(5).ok();
        let update_rule: Option<String> = r.try_get(6).ok();
        let entry = by_name.entry(name.clone()).or_insert_with(|| ForeignKeyInfo {
            name: name.clone(),
            columns: Vec::new(),
            ref_schema,
            ref_table,
            ref_columns: Vec::new(),
            on_delete: delete_rule.filter(|s| !s.is_empty() && s != "NO ACTION"),
            on_update: update_rule.filter(|s| !s.is_empty() && s != "NO ACTION"),
        });
        entry.columns.push(column);
        entry.ref_columns.push(ref_column);
    }
    Ok(by_name.into_values().collect())
}

pub(crate) async fn read_fetch_inbound_foreign_keys(
    connection: &mut sqlx::MySqlConnection,
    schema: Option<&str>,
    table: &str,
) -> Result<Vec<InboundForeignKeyInfo>, DriverError> {
    let rows = sqlx::query(
        "SELECT
            CAST(kcu.constraint_name AS CHAR),
            CAST(kcu.table_schema AS CHAR),
            CAST(kcu.table_name AS CHAR),
            CAST(kcu.column_name AS CHAR),
            CAST(kcu.referenced_column_name AS CHAR)
        FROM information_schema.key_column_usage kcu
        WHERE kcu.referenced_table_schema = COALESCE(?, DATABASE())
          AND kcu.referenced_table_name = ?
        ORDER BY kcu.constraint_name, kcu.ordinal_position",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?;
    let mut by_name: std::collections::BTreeMap<String, InboundForeignKeyInfo> = std::collections::BTreeMap::new();
    for r in rows {
        let name: String = r.get(0);
        let source_schema: Option<String> = r.try_get(1).unwrap_or(None);
        let source_table: String = r.get(2);
        let source_column: String = r.get(3);
        let target_column: String = r.get(4);
        let entry = by_name.entry(name.clone()).or_insert_with(|| InboundForeignKeyInfo {
            name,
            source_schema,
            source_table,
            source_columns: Vec::new(),
            target_columns: Vec::new(),
        });
        entry.source_columns.push(source_column);
        entry.target_columns.push(target_column);
    }
    Ok(by_name.into_values().collect())
}

pub(crate) async fn read_server_version(connection: &mut sqlx::MySqlConnection) -> Result<Option<String>, DriverError> {
    let row = sqlx::query("SELECT VERSION()")
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx_error)?;
    let v: String = row.try_get(0).unwrap_or_default();
    Ok(Some(v))
}
