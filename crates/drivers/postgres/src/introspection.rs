use super::*;

pub(crate) async fn read_list_tables_scoped(
    connection: &mut sqlx::PgConnection,
    schemas: &[String],
) -> Result<Vec<TableInfo>, DriverError> {
    let rows = sqlx::query(
        "SELECT n.nspname, c.relname
         FROM pg_catalog.pg_class c
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
         WHERE c.relkind IN ('r', 'p')
           AND n.nspname = ANY($1)
           AND n.nspname NOT IN ('pg_catalog', 'information_schema')
           AND pg_catalog.has_schema_privilege(n.oid, 'USAGE')
           AND pg_catalog.has_any_column_privilege(c.oid, 'SELECT,INSERT,UPDATE,REFERENCES')
         ORDER BY n.nspname, c.relname",
    )
    .bind(schemas)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?;
    Ok(rows
        .into_iter()
        .map(|r| TableInfo {
            schema: Some(r.get::<String, _>(0)),
            name: r.get::<String, _>(1),
        })
        .collect())
}

pub(crate) async fn read_list_collations(
    connection: &mut sqlx::PgConnection,
) -> Result<Vec<bookie_core::CollationInfo>, DriverError> {
    let rows = sqlx::query(
        "SELECT collname::text,
                CASE WHEN collencoding = -1 THEN '' ELSE pg_catalog.pg_encoding_to_char(collencoding) END,
                collname = 'default'
         FROM pg_catalog.pg_collation
         WHERE collencoding IN (-1, pg_catalog.pg_char_to_encoding(pg_catalog.pg_encoding_to_char((SELECT encoding FROM pg_catalog.pg_database WHERE datname = pg_catalog.current_database()))))
         ORDER BY collname
         LIMIT $1",
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

pub(crate) async fn read_list_databases(connection: &mut sqlx::PgConnection) -> Result<Vec<String>, DriverError> {
    let rows = sqlx::query(
        "SELECT datname FROM pg_catalog.pg_database
         WHERE datallowconn AND NOT datistemplate AND pg_catalog.has_database_privilege(datname, 'CONNECT')
         ORDER BY datname",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?;
    Ok(rows.into_iter().map(|row| row.get::<String, _>(0)).collect())
}

pub(crate) async fn read_fetch_columns(
    connection: &mut sqlx::PgConnection,
    schema: Option<&str>,
    table: &str,
) -> Result<Vec<ColumnInfo>, DriverError> {
    // Source schema metadata from pg_catalog rather than
    // information_schema:
    //   - pg_attribute.attgenerated ('s' for STORED, '' otherwise)
    //     is the canonical generated-column flag. The
    //     information_schema.is_generated text column is brittle
    //     across PG versions.
    //   - pg_attribute.attidentity ('a' / 'd' for ALWAYS / BY
    //     DEFAULT identity, '' otherwise) authoritatively flags
    //     identity columns.
    //   - format_type() returns the user-facing type name including
    //     length / precision (e.g. "character varying(255)") which
    //     matches what the user wrote in CREATE TABLE.
    //   - pg_get_expr() returns the default expression text.
    //   - col_description() is the column-level comment accessor;
    //     obj_description() answers for the relation, so using it
    //     here would give every column the table's comment.
    let rows = sqlx::query(
        "SELECT
            a.attname,
            pg_catalog.format_type(a.atttypid, a.atttypmod) AS data_type,
            NOT a.attnotnull AS nullable,
            EXISTS (
                SELECT 1 FROM pg_catalog.pg_constraint c
                WHERE c.conrelid = a.attrelid
                  AND c.contype = 'p'
                  AND a.attnum = ANY(c.conkey)
            ) AS is_pk,
            pg_catalog.pg_get_expr(d.adbin, d.adrelid) AS default_value,
            a.attidentity <> '' AS is_identity,
            a.attgenerated <> '' AS is_generated,
            pg_catalog.col_description(a.attrelid, a.attnum) AS column_comment,
            CASE WHEN a.attcollation <> 0 AND a.attcollation <> ty.typcollation
                THEN co.collname::text END AS collation,
            type_ns.nspname AS enum_schema,
            enum_ty.typname AS enum_name,
            CASE WHEN ty.typtype = 'd' THEN domain_ns.nspname END AS domain_schema,
            CASE WHEN ty.typtype = 'd' THEN ty.typname END AS domain_name
         FROM pg_catalog.pg_attribute a
         JOIN pg_catalog.pg_class t ON a.attrelid = t.oid
         JOIN pg_catalog.pg_namespace n ON t.relnamespace = n.oid
         JOIN pg_catalog.pg_type ty ON ty.oid = a.atttypid
         LEFT JOIN LATERAL (
             WITH RECURSIVE type_chain(oid, typtype, typbasetype, typnamespace, typname) AS (
                 SELECT candidate.oid, candidate.typtype, candidate.typbasetype,
                        candidate.typnamespace, candidate.typname
                 FROM pg_catalog.pg_type candidate
                 WHERE candidate.oid = CASE
                     WHEN ty.typcategory = 'A' AND EXISTS (
                         SELECT 1 FROM pg_catalog.pg_type element
                         WHERE element.oid = ty.typelem AND element.typtype IN ('e', 'd')
                     ) THEN ty.typelem
                     ELSE ty.oid
                 END
                 UNION ALL
                 SELECT base.oid, base.typtype, base.typbasetype,
                        base.typnamespace, base.typname
                 FROM pg_catalog.pg_type base
                 JOIN type_chain ON base.oid = type_chain.typbasetype
                 WHERE type_chain.typtype = 'd'
             )
             SELECT typnamespace, typname
             FROM type_chain
             WHERE typtype = 'e'
                OR (ty.typcategory = 'A' AND oid = ty.typelem AND typtype = 'd')
             ORDER BY CASE
                 WHEN ty.typcategory = 'A' AND oid = ty.typelem AND typtype = 'd' THEN 0
                 ELSE 1
             END
             LIMIT 1
         ) enum_ty ON TRUE
         LEFT JOIN pg_catalog.pg_namespace type_ns ON type_ns.oid = enum_ty.typnamespace
         LEFT JOIN pg_catalog.pg_namespace domain_ns ON domain_ns.oid = ty.typnamespace
         LEFT JOIN pg_catalog.pg_collation co ON co.oid = a.attcollation
         LEFT JOIN pg_catalog.pg_attrdef d
             ON d.adrelid = a.attrelid AND d.adnum = a.attnum
         WHERE (($2::text IS NULL AND t.relname = $1 AND pg_catalog.pg_table_is_visible(t.oid))
              OR (n.nspname = $2 AND t.relname = $1))
           AND a.attnum > 0
           AND NOT a.attisdropped
           AND pg_catalog.has_schema_privilege(n.oid, 'USAGE')
           AND pg_catalog.has_column_privilege(t.oid, a.attnum, 'SELECT,INSERT,UPDATE,REFERENCES')
         ORDER BY a.attnum",
    )
    .bind(table)
    .bind(schema)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?;
    rows.into_iter()
        .map(|r| {
            let raw_default: Option<String> = r.try_get::<Option<String>, _>(4).unwrap_or(None);
            let is_identity = r.try_get::<bool, _>(5).unwrap_or(false);
            let is_generated = r.try_get::<bool, _>(6).unwrap_or(false);
            // SERIAL / BIGSERIAL columns aren't IDENTITY in PG's
            // catalog terms but have a `nextval(...)` default; treat
            // them as auto-increment for the inline-insert UI.
            let is_serial = raw_default
                .as_deref()
                .map(|d| d.starts_with("nextval("))
                .unwrap_or(false);
            // For identity / serial columns the default expression
            // is internal sequence machinery — suppress so the UI
            // doesn't leak implementation details. Otherwise
            // normalise the expression for display.
            let default_value = if is_identity || is_serial {
                None
            } else {
                raw_default.map(normalize_pg_default)
            };
            let enum_schema = r.try_get::<Option<String>, _>(9).map_err(map_sqlx_error)?;
            let enum_name = r.try_get::<Option<String>, _>(10).map_err(map_sqlx_error)?;
            let domain_schema = r.try_get::<Option<String>, _>(11).map_err(map_sqlx_error)?;
            let domain_name = r.try_get::<Option<String>, _>(12).map_err(map_sqlx_error)?;
            Ok(ColumnInfo {
                name: r.get::<String, _>(0),
                data_type: r.get::<String, _>(1),
                nullable: r.get::<bool, _>(2),
                primary_key: r.get::<bool, _>(3),
                is_auto_increment: is_identity || is_serial,
                default_value,
                is_generated,
                comment: r
                    .try_get::<Option<String>, _>(7)
                    .unwrap_or(None)
                    .filter(|c| !c.is_empty()),
                collation: r.try_get::<Option<String>, _>(8).unwrap_or(None),
                enum_type: enum_schema
                    .zip(enum_name)
                    .map(|(schema, name)| QualifiedTypeName { schema, name }),
                domain_type: domain_schema
                    .zip(domain_name)
                    .map(|(schema, name)| QualifiedTypeName { schema, name }),
            })
        })
        .collect::<Result<Vec<_>, DriverError>>()
}

pub(crate) async fn read_fetch_indexes(
    connection: &mut sqlx::PgConnection,
    schema: Option<&str>,
    table: &str,
) -> Result<Vec<IndexInfo>, DriverError> {
    // `indkey` holds 0 for an expression key and lists INCLUDE columns
    // after the first `indnkeyatts` key columns; the expression text
    // comes from `pg_get_indexdef` at that key position.
    let rows = sqlx::query(
        "SELECT
            i.relname AS index_name,
            ix.indisunique,
            ix.indisprimary,
            ARRAY(
                SELECT CASE WHEN k.attnum = 0
                    THEN pg_catalog.pg_get_indexdef(ix.indexrelid, k.ordinality::int, true)
                    ELSE a.attname::text END
                FROM pg_catalog.unnest(ix.indkey) WITH ORDINALITY AS k(attnum, ordinality)
                LEFT JOIN pg_catalog.pg_attribute a ON a.attrelid = ix.indrelid AND a.attnum = k.attnum
                WHERE k.ordinality <= ix.indnkeyatts
                ORDER BY k.ordinality
            ) AS columns,
            pg_catalog.pg_get_expr(ix.indpred, ix.indrelid, true) AS predicate,
            (SELECT c.conname::text FROM pg_catalog.pg_constraint c
             WHERE c.conindid = ix.indexrelid AND c.conrelid = ix.indrelid
               AND c.contype IN ('p', 'u', 'x') LIMIT 1) AS constraint_name,
            0 = ANY(ix.indkey::int2[]) AS has_expression_key,
            (SELECT pg_catalog.substr(d.definition, h.at + pg_catalog.length(h.head))
             FROM (SELECT pg_catalog.pg_get_indexdef(ix.indexrelid) AS definition) d
             CROSS JOIN LATERAL (
                 SELECT q.head, pg_catalog.strpos(d.definition, q.head) AS at
                 FROM (SELECT pg_catalog.quote_ident(n.nspname) || '.' || pg_catalog.quote_ident(t.relname)
                              || ' USING ' AS head) q
             ) h
             WHERE h.at > 0) AS using_clause
        FROM pg_catalog.pg_class t
        JOIN pg_catalog.pg_namespace n ON t.relnamespace = n.oid
        JOIN pg_catalog.pg_index ix ON ix.indrelid = t.oid
        JOIN pg_catalog.pg_class i ON i.oid = ix.indexrelid
        WHERE (($2::text IS NULL AND t.relname = $1 AND pg_catalog.pg_table_is_visible(t.oid))
             OR (n.nspname = $2 AND t.relname = $1))
          AND pg_catalog.has_schema_privilege(n.oid, 'USAGE')
          AND pg_catalog.has_any_column_privilege(t.oid, 'SELECT,INSERT,UPDATE,REFERENCES')
        ORDER BY i.relname",
    )
    .bind(table)
    .bind(schema)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?;
    rows.into_iter()
        .map(|r| {
            Ok(IndexInfo {
                name: r.try_get(0).map_err(map_sqlx_error)?,
                unique: r.try_get(1).map_err(map_sqlx_error)?,
                primary: r.try_get(2).map_err(map_sqlx_error)?,
                columns: r.try_get(3).map_err(map_sqlx_error)?,
                predicate: r.try_get(4).map_err(map_sqlx_error)?,
                constraint: r.try_get(5).map_err(map_sqlx_error)?,
                has_expression_key: r.try_get(6).map_err(map_sqlx_error)?,
                using_clause: r.try_get(7).map_err(map_sqlx_error)?,
            })
        })
        .collect()
}

pub(crate) async fn read_fetch_enum_labels(
    connection: &mut sqlx::PgConnection,
    schema: &str,
    name: &str,
) -> Result<Vec<String>, DriverError> {
    let rows = sqlx::query(
        "WITH RECURSIVE type_chain(oid, typtype, typbasetype) AS (
             SELECT t.oid, t.typtype, t.typbasetype
             FROM pg_catalog.pg_type t
             JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
             WHERE n.nspname = $1 AND t.typname = $2
             UNION ALL
             SELECT base.oid, base.typtype, base.typbasetype
             FROM pg_catalog.pg_type base
             JOIN type_chain ON base.oid = type_chain.typbasetype
             WHERE type_chain.typtype = 'd'
         )
         SELECT e.enumlabel::text
         FROM pg_catalog.pg_enum e
         JOIN type_chain tc ON tc.oid = e.enumtypid
         WHERE tc.typtype = 'e'
         ORDER BY e.enumsortorder",
    )
    .bind(schema)
    .bind(name)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?;
    Ok(rows.into_iter().map(|row| row.get::<String, _>(0)).collect())
}

pub(crate) async fn read_fetch_foreign_keys(
    connection: &mut sqlx::PgConnection,
    schema: Option<&str>,
    table: &str,
) -> Result<Vec<ForeignKeyInfo>, DriverError> {
    // pg_constraint with contype = 'f'. confkey arrays are parallel
    // to conkey via ordinality; the LATERAL join pairs them so the
    // FK column ↔ referenced column mapping survives composite
    // FKs. confdeltype / confupdtype are single chars normalised
    // to canonical SQL keyword strings.
    let rows = sqlx::query(
        "SELECT
            c.conname AS fk_name,
            pg_catalog.array_agg(a.attname ORDER BY kf.ordinality) AS columns,
            fn_class.relname AS ref_table,
            fn_ns.nspname AS ref_schema,
            pg_catalog.array_agg(fa.attname ORDER BY kf.ordinality) AS ref_columns,
            c.confdeltype::text AS on_delete_code,
            c.confupdtype::text AS on_update_code
        FROM pg_catalog.pg_constraint c
        JOIN pg_catalog.pg_class t ON t.oid = c.conrelid
        JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace
        JOIN pg_catalog.pg_class fn_class ON fn_class.oid = c.confrelid
        JOIN pg_catalog.pg_namespace fn_ns ON fn_ns.oid = fn_class.relnamespace
        JOIN LATERAL pg_catalog.unnest(c.conkey) WITH ORDINALITY AS kf(attnum, ordinality) ON true
        JOIN pg_catalog.pg_attribute a ON a.attrelid = t.oid AND a.attnum = kf.attnum
        JOIN LATERAL pg_catalog.unnest(c.confkey) WITH ORDINALITY AS kfr(attnum, ordinality)
            ON kfr.ordinality = kf.ordinality
        JOIN pg_catalog.pg_attribute fa ON fa.attrelid = c.confrelid AND fa.attnum = kfr.attnum
        WHERE c.contype = 'f'
          AND (($2::text IS NULL AND t.relname = $1 AND pg_catalog.pg_table_is_visible(t.oid))
             OR (n.nspname = $2 AND t.relname = $1))
          AND pg_catalog.has_schema_privilege(n.oid, 'USAGE')
          AND pg_catalog.has_any_column_privilege(t.oid, 'SELECT,INSERT,UPDATE,REFERENCES')
          AND pg_catalog.has_schema_privilege(fn_ns.oid, 'USAGE')
          AND pg_catalog.has_any_column_privilege(fn_class.oid, 'SELECT,INSERT,UPDATE,REFERENCES')
        GROUP BY c.conname, fn_class.relname, fn_ns.nspname, c.confdeltype, c.confupdtype
        ORDER BY c.conname",
    )
    .bind(table)
    .bind(schema)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?;
    Ok(rows
        .into_iter()
        .map(|r| ForeignKeyInfo {
            name: r.get::<String, _>(0),
            columns: r.get::<Vec<String>, _>(1),
            ref_table: r.get::<String, _>(2),
            ref_schema: r.try_get::<Option<String>, _>(3).unwrap_or(None),
            ref_columns: r.get::<Vec<String>, _>(4),
            on_delete: pg_action_char_to_keyword(r.try_get::<String, _>(5).ok().as_deref().unwrap_or("a")),
            on_update: pg_action_char_to_keyword(r.try_get::<String, _>(6).ok().as_deref().unwrap_or("a")),
        })
        .collect())
}

pub(crate) async fn read_fetch_inbound_foreign_keys(
    connection: &mut sqlx::PgConnection,
    schema: Option<&str>,
    table: &str,
) -> Result<Vec<InboundForeignKeyInfo>, DriverError> {
    let rows = sqlx::query(
        "SELECT
            c.conname AS fk_name,
            t_ns.nspname AS source_schema,
            t.relname AS source_table,
            pg_catalog.array_agg(a.attname ORDER BY kf.ordinality) AS source_columns,
            pg_catalog.array_agg(fa.attname ORDER BY kf.ordinality) AS target_columns
        FROM pg_catalog.pg_constraint c
        JOIN pg_catalog.pg_class t ON t.oid = c.conrelid
        JOIN pg_catalog.pg_namespace t_ns ON t_ns.oid = t.relnamespace
        JOIN pg_catalog.pg_class fn_class ON fn_class.oid = c.confrelid
        JOIN pg_catalog.pg_namespace fn_ns ON fn_ns.oid = fn_class.relnamespace
        JOIN LATERAL pg_catalog.unnest(c.conkey) WITH ORDINALITY AS kf(attnum, ordinality) ON true
        JOIN pg_catalog.pg_attribute a ON a.attrelid = t.oid AND a.attnum = kf.attnum
        JOIN LATERAL pg_catalog.unnest(c.confkey) WITH ORDINALITY AS kfr(attnum, ordinality)
            ON kfr.ordinality = kf.ordinality
        JOIN pg_catalog.pg_attribute fa ON fa.attrelid = c.confrelid AND fa.attnum = kfr.attnum
        WHERE c.contype = 'f'
          AND (($2::text IS NULL AND fn_class.relname = $1 AND pg_catalog.pg_table_is_visible(fn_class.oid))
             OR (fn_ns.nspname = $2 AND fn_class.relname = $1))
        GROUP BY c.conname, t_ns.nspname, t.relname
        ORDER BY c.conname",
    )
    .bind(table)
    .bind(schema)
    .fetch_all(&mut *connection)
    .await
    .map_err(map_sqlx_error)?;
    let mut keys = Vec::with_capacity(rows.len());
    for row in rows {
        let name: String = row.get("fk_name");
        let source_schema: Option<String> = row.try_get("source_schema").ok();
        let source_table: String = row.get("source_table");
        let source_columns: Vec<String> = row.get("source_columns");
        let target_columns: Vec<String> = row.get("target_columns");
        keys.push(InboundForeignKeyInfo {
            name,
            source_schema,
            source_table,
            source_columns,
            target_columns,
        });
    }
    Ok(keys)
}

pub(crate) async fn read_server_version(connection: &mut sqlx::PgConnection) -> Result<Option<String>, DriverError> {
    let row = sqlx::query("SHOW server_version")
        .fetch_one(&mut *connection)
        .await
        .map_err(map_sqlx_error)?;
    let v: String = row.try_get(0).unwrap_or_default();
    Ok(Some(format!("PostgreSQL {v}")))
}
