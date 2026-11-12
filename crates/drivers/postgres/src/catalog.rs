use bookie_core::{
    CATALOG_OBJECT_LIMIT, CatalogObject, CatalogObjectKind, DriverError, ROUTINE_DEFINITION_LIMIT_BYTES,
    RoutineDefinition, RoutineLookup, RoutineRef, cap_definition,
};
use sqlx::postgres::PgRow;
use sqlx::{Pool, Postgres, Row};

use crate::map_sqlx_error;

pub(crate) const SUPPORTED_KINDS: &[CatalogObjectKind] = &CatalogObjectKind::ALL;

const USER_SCHEMA: &str = "n.nspname NOT IN ('pg_catalog', 'information_schema') \
     AND n.nspname NOT LIKE 'pg\\_toast%' AND n.nspname NOT LIKE 'pg\\_temp\\_%' \
     AND ($1::text IS NULL OR n.nspname = $1) \
     AND NOT EXISTS (SELECT 1 FROM pg_catalog.unnest($3::text[]) AS hidden_schema(schema_name) \
                     WHERE hidden_schema.schema_name = n.nspname) \
     AND pg_catalog.has_schema_privilege(n.oid, 'USAGE')";

const TABLE_REACHABLE: &str = "pg_catalog.has_any_column_privilege(c.oid, 'SELECT,INSERT,UPDATE,REFERENCES')";

fn not_generated(catalog: &str, oid: &str) -> String {
    format!(
        "NOT EXISTS (SELECT 1 FROM pg_catalog.pg_depend d WHERE d.classid = '{catalog}'::regclass \
         AND d.objid = {oid} AND d.deptype IN ('i', 'e'))"
    )
}

fn routines_sql() -> String {
    let generated = not_generated("pg_catalog.pg_proc", "p.oid");
    format!(
        "SELECT n.nspname, p.proname,
             CASE p.prokind WHEN 'p' THEN 'procedure' WHEN 'a' THEN 'aggregate' WHEN 'w' THEN 'window'
                 ELSE 'function' END || '(' || pg_catalog.pg_get_function_identity_arguments(p.oid) || ')'
         FROM pg_catalog.pg_proc p
         JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
         WHERE {USER_SCHEMA} AND {generated} AND pg_catalog.has_function_privilege(p.oid, 'EXECUTE')
         ORDER BY 1, 2, 3 LIMIT $2"
    )
}

fn triggers_sql() -> String {
    format!(
        "SELECT n.nspname, t.tgname, 'on ' || c.relname
         FROM pg_catalog.pg_trigger t
         JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
         WHERE NOT t.tgisinternal AND {USER_SCHEMA} AND {TABLE_REACHABLE}
         ORDER BY 1, 2, 3 LIMIT $2"
    )
}

fn sequences_sql() -> String {
    format!(
        "SELECT n.nspname, c.relname,
                pg_catalog.format_type(s.seqtypid, NULL) || ' · '
                    || COALESCE('last value ' || pg_catalog.pg_sequence_last_value(s.seqrelid)::text, 'not used yet')
                    || ' · step ' || s.seqincrement::text
         FROM pg_catalog.pg_sequence s
         JOIN pg_catalog.pg_class c ON c.oid = s.seqrelid
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
         WHERE {USER_SCHEMA} AND pg_catalog.has_sequence_privilege(s.seqrelid, 'USAGE,SELECT,UPDATE')
         ORDER BY 1, 2 LIMIT $2"
    )
}

fn types_sql() -> String {
    let generated = not_generated("pg_catalog.pg_type", "t.oid");
    format!(
        "SELECT n.nspname, t.typname,
             CASE t.typtype WHEN 'e' THEN 'enum' WHEN 'c' THEN 'composite' WHEN 'r' THEN 'range'
                 ELSE 'domain over ' || pg_catalog.format_type(t.typbasetype, t.typtypmod) END
         FROM pg_catalog.pg_type t
         JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
         LEFT JOIN pg_catalog.pg_class c ON c.oid = t.typrelid
         WHERE t.typtype IN ('e', 'c', 'd', 'r') AND (t.typtype <> 'c' OR c.relkind = 'c')
             AND {USER_SCHEMA} AND {generated} AND pg_catalog.has_type_privilege(t.oid, 'USAGE')
         ORDER BY 1, 2 LIMIT $2"
    )
}

fn grants_sql() -> String {
    format!(
        "SELECT n.nspname, c.relname,
             CASE WHEN a.grantee = 0 THEN 'PUBLIC' ELSE pg_catalog.pg_get_userbyid(a.grantee) END
                 || ': ' || string_agg(a.privilege_type, ', ' ORDER BY a.privilege_type)
         FROM pg_catalog.pg_class c
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
         CROSS JOIN LATERAL pg_catalog.aclexplode(COALESCE(c.relacl, pg_catalog.acldefault('r', c.relowner))) a
         WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f') AND {USER_SCHEMA} AND {TABLE_REACHABLE}
         GROUP BY n.nspname, c.relname, a.grantee
         ORDER BY 1, 2, 3 LIMIT $2"
    )
}

const EXTENSIONS_SQL: &str = "SELECT n.nspname, e.extname, e.extversion
     FROM pg_catalog.pg_extension e
     JOIN pg_catalog.pg_namespace n ON n.oid = e.extnamespace
     WHERE NOT EXISTS (SELECT 1 FROM pg_catalog.unnest($1::text[]) AS hidden_schema(schema_name) \
                       WHERE hidden_schema.schema_name = n.nspname)
       AND pg_catalog.has_schema_privilege(n.oid, 'USAGE')
     ORDER BY 2 LIMIT $2";

const ROLES_SQL: &str = "SELECT NULL::text, r.rolname,
         NULLIF(concat_ws(', ',
             CASE WHEN r.rolsuper THEN 'superuser' END,
             CASE WHEN r.rolcanlogin THEN 'login' END,
             CASE WHEN r.rolcreatedb THEN 'create database' END,
             CASE WHEN r.rolcreaterole THEN 'create role' END), '')
     FROM pg_catalog.pg_roles r
     WHERE r.rolname NOT LIKE 'pg\\_%'
     ORDER BY 2 LIMIT $1";

fn sql_for(kind: CatalogObjectKind) -> String {
    match kind {
        CatalogObjectKind::Routine => routines_sql(),
        CatalogObjectKind::Trigger => triggers_sql(),
        CatalogObjectKind::Sequence => sequences_sql(),
        CatalogObjectKind::Type => types_sql(),
        CatalogObjectKind::Grant => grants_sql(),
        CatalogObjectKind::Extension => EXTENSIONS_SQL.to_string(),
        CatalogObjectKind::Role => ROLES_SQL.to_string(),
    }
}

pub(crate) async fn list_objects(
    pool: &Pool<Postgres>,
    kind: CatalogObjectKind,
    schema: Option<&str>,
) -> Result<Vec<CatalogObject>, DriverError> {
    list_objects_excluding(pool, kind, schema, &[]).await
}

pub(crate) async fn list_objects_excluding(
    pool: &Pool<Postgres>,
    kind: CatalogObjectKind,
    schema: Option<&str>,
    excluded_schemas: &[String],
) -> Result<Vec<CatalogObject>, DriverError> {
    let limit = i64::try_from(CATALOG_OBJECT_LIMIT).unwrap_or(i64::MAX);
    let query = sqlx::query(sqlx::AssertSqlSafe(sql_for(kind)));
    let query = match kind {
        CatalogObjectKind::Routine
        | CatalogObjectKind::Trigger
        | CatalogObjectKind::Sequence
        | CatalogObjectKind::Type
        | CatalogObjectKind::Grant => query.bind(schema).bind(limit).bind(excluded_schemas),
        CatalogObjectKind::Extension => query.bind(excluded_schemas).bind(limit),
        CatalogObjectKind::Role => query.bind(limit),
    };
    let rows = query.fetch_all(pool).await.map_err(map_sqlx_error)?;
    rows.iter().map(|row| object_from_row(kind, row)).collect()
}

fn object_from_row(kind: CatalogObjectKind, row: &PgRow) -> Result<CatalogObject, DriverError> {
    Ok(CatalogObject {
        kind,
        schema: row.try_get(0).map_err(map_sqlx_error)?,
        name: row.try_get(1).map_err(map_sqlx_error)?,
        detail: row.try_get(2).map_err(map_sqlx_error)?,
    })
}

const AMBIGUITY_PROBE_LIMIT: i64 = 3;

fn routine_definition_sql() -> String {
    let generated = not_generated("pg_catalog.pg_proc", "p.oid");
    format!(
        "SELECT p.prokind::text, pg_catalog.pg_get_function_identity_arguments(p.oid),
             pg_catalog.octet_length(d.definition)::bigint, pg_catalog.left(d.definition, $6::int)
         FROM pg_catalog.pg_proc p
         JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
         CROSS JOIN LATERAL (
             SELECT CASE WHEN p.prokind <> 'a' THEN pg_catalog.pg_get_functiondef(p.oid) END AS definition
         ) d
         WHERE {USER_SCHEMA} AND {generated} AND pg_catalog.has_function_privilege(p.oid, 'EXECUTE')
             AND p.proname::text = $4
             AND ($5::text IS NULL OR pg_catalog.pg_get_function_identity_arguments(p.oid) = $5)
         ORDER BY 2 LIMIT $2"
    )
}

fn routine_kind(prokind: &str) -> &'static str {
    match prokind {
        "p" => "procedure",
        "a" => "aggregate",
        "w" => "window",
        _ => "function",
    }
}

pub(crate) async fn routine_definition(
    pool: &Pool<Postgres>,
    routine: &RoutineRef,
) -> Result<RoutineLookup, DriverError> {
    let cap = i64::try_from(ROUTINE_DEFINITION_LIMIT_BYTES).unwrap_or(i64::MAX);
    let no_hidden: &[String] = &[];
    let rows = sqlx::query(sqlx::AssertSqlSafe(routine_definition_sql()))
        .bind(routine.schema.as_str())
        .bind(AMBIGUITY_PROBE_LIMIT)
        .bind(no_hidden)
        .bind(routine.name.as_str())
        .bind(routine.identity_arguments.as_deref())
        .bind(cap)
        .fetch_all(pool)
        .await
        .map_err(map_sqlx_error)?;
    match rows.as_slice() {
        [] => Ok(RoutineLookup::NotFound),
        [row] => definition_from_row(routine, row),
        many => {
            let signatures = many
                .iter()
                .map(|row| row.try_get::<String, _>(1).map_err(map_sqlx_error))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(RoutineLookup::Ambiguous { signatures })
        }
    }
}

fn definition_from_row(routine: &RoutineRef, row: &PgRow) -> Result<RoutineLookup, DriverError> {
    let prokind: String = row.try_get(0).map_err(map_sqlx_error)?;
    let kind = routine_kind(&prokind);
    let identity_arguments: String = row.try_get(1).map_err(map_sqlx_error)?;
    let total: Option<i64> = row.try_get(2).map_err(map_sqlx_error)?;
    let text: Option<String> = row.try_get(3).map_err(map_sqlx_error)?;
    let (Some(total), Some(text)) = (total, text) else {
        return Err(DriverError::Unsupported(format!("{kind} definitions")));
    };
    let (definition, cut) = cap_definition(&text, ROUTINE_DEFINITION_LIMIT_BYTES);
    let total_bytes = u64::try_from(total).unwrap_or(u64::MAX);
    Ok(RoutineLookup::Found(RoutineDefinition {
        schema: routine.schema.clone(),
        name: routine.name.clone(),
        kind: kind.to_string(),
        identity_arguments,
        truncated: cut || total_bytes > definition.len() as u64,
        definition,
        total_bytes: Some(total_bytes),
    }))
}
