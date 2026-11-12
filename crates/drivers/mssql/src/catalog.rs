use bookie_core::{
    CATALOG_OBJECT_LIMIT, CatalogObject, CatalogObjectKind, DriverError, ROUTINE_DEFINITION_LIMIT_BYTES,
    RoutineDefinition, RoutineLookup, RoutineRef, Value, cap_definition,
};

use crate::{MssqlClient, run_query};

pub(crate) const SUPPORTED_KINDS: &[CatalogObjectKind] = &[
    CatalogObjectKind::Routine,
    CatalogObjectKind::Trigger,
    CatalogObjectKind::Sequence,
    CatalogObjectKind::Type,
    CatalogObjectKind::Role,
];

const TRIGGERS_SQL: &str = "SELECT s.name AS schema_name, \
    tr.name AS trigger_name, \
    'on ' + o.name + CASE WHEN tr.is_disabled = 1 THEN ' (disabled)' ELSE '' END AS detail \
    FROM sys.triggers tr \
    JOIN sys.objects o ON o.object_id = tr.parent_id \
    JOIN sys.schemas s ON s.schema_id = o.schema_id \
    WHERE (@P1 IS NULL OR s.name = @P1) \
    ORDER BY s.name, tr.name";

const ROUTINES_SQL: &str = "SELECT s.name AS schema_name, \
    o.name AS routine_name, \
    CASE o.type \
        WHEN 'P' THEN 'procedure' \
        WHEN 'FN' THEN 'scalar function' \
        WHEN 'IF' THEN 'inline table-valued function' \
        WHEN 'TF' THEN 'table-valued function' \
        ELSE 'routine' \
    END AS detail \
    FROM sys.objects o \
    JOIN sys.schemas s ON s.schema_id = o.schema_id \
    WHERE o.type IN ('P', 'FN', 'IF', 'TF') \
      AND o.is_ms_shipped = 0 \
      AND (@P1 IS NULL OR s.name = @P1) \
    ORDER BY s.name, o.name";

const SEQUENCES_SQL: &str = "SELECT s.name AS schema_name, \
    sq.name AS sequence_name, \
    t.name AS detail \
    FROM sys.sequences sq \
    JOIN sys.schemas s ON s.schema_id = sq.schema_id \
    JOIN sys.types t ON t.user_type_id = sq.user_type_id \
    WHERE (@P1 IS NULL OR s.name = @P1) \
    ORDER BY s.name, sq.name";

const TYPES_SQL: &str = "SELECT s.name AS schema_name, \
    t.name AS type_name, \
    CASE WHEN t.is_table_type = 1 THEN 'table type' ELSE 'alias type' END AS detail \
    FROM sys.types t \
    JOIN sys.schemas s ON s.schema_id = t.schema_id \
    WHERE t.is_user_defined = 1 \
      AND (@P1 IS NULL OR s.name = @P1) \
    ORDER BY s.name, t.name";

const ROLES_SQL: &str = "SELECT NULL AS schema_name, \
    dp.name AS role_name, \
    CASE dp.is_fixed_role WHEN 1 THEN 'fixed database role' ELSE 'database role' END AS detail \
    FROM sys.database_principals dp \
    WHERE dp.type = 'R' \
    ORDER BY dp.name";

fn sql_for(kind: CatalogObjectKind) -> Option<&'static str> {
    match kind {
        CatalogObjectKind::Trigger => Some(TRIGGERS_SQL),
        CatalogObjectKind::Routine => Some(ROUTINES_SQL),
        CatalogObjectKind::Sequence => Some(SEQUENCES_SQL),
        CatalogObjectKind::Type => Some(TYPES_SQL),
        CatalogObjectKind::Role => Some(ROLES_SQL),
        _ => None,
    }
}

pub(crate) async fn list_objects(
    client: &mut MssqlClient,
    kind: CatalogObjectKind,
    schema: Option<&str>,
) -> Result<Vec<CatalogObject>, DriverError> {
    let sql = sql_for(kind).ok_or_else(|| DriverError::Unsupported(format!("listing {} objects", kind.as_str())))?;
    let params = match kind.is_schema_scoped() {
        true => vec![schema.map(str::to_string).map(Value::Text).unwrap_or(Value::Null)],
        false => Vec::new(),
    };
    let result = run_query(client, sql, &params, CATALOG_OBJECT_LIMIT).await?;
    Ok(result
        .rows
        .iter()
        .filter_map(|row| object_from_row(kind, row))
        .collect())
}

fn object_from_row(kind: CatalogObjectKind, row: &[Value]) -> Option<CatalogObject> {
    let schema = match row.first() {
        Some(Value::Text(s)) => Some(s.clone()),
        _ => None,
    };
    let name = match row.get(1) {
        Some(Value::Text(s)) => s.clone(),
        _ => return None,
    };
    let detail = match row.get(2) {
        Some(Value::Text(s)) => Some(s.clone()),
        _ => None,
    };
    Some(CatalogObject {
        kind,
        schema,
        name,
        detail,
    })
}

const ROUTINE_DEFINITION_SQL: &str = "SELECT o.type, d.definition_text, d.over_cap \
    FROM sys.objects o \
    JOIN sys.schemas s ON s.schema_id = o.schema_id \
    CROSS APPLY (SELECT OBJECT_DEFINITION(o.object_id) AS full_text) x \
    CROSS APPLY (SELECT LEFT(x.full_text, @P3) AS definition_text, \
        CASE WHEN DATALENGTH(x.full_text) > @P3 * 2 THEN 1 ELSE 0 END AS over_cap) d \
    WHERE o.type IN ('P', 'FN', 'IF', 'TF') \
      AND o.is_ms_shipped = 0 \
      AND s.name = @P1 AND o.name = @P2";

fn routine_kind(object_type: &str) -> &'static str {
    match object_type.trim() {
        "P" => "procedure",
        "FN" => "scalar function",
        "IF" => "inline table-valued function",
        "TF" => "table-valued function",
        _ => "routine",
    }
}

pub(crate) async fn routine_definition(
    client: &mut MssqlClient,
    routine: &RoutineRef,
) -> Result<RoutineLookup, DriverError> {
    let cap = i64::try_from(ROUTINE_DEFINITION_LIMIT_BYTES).unwrap_or(i64::MAX);
    let params = [
        Value::Text(routine.schema.clone()),
        Value::Text(routine.name.clone()),
        Value::Int(cap),
    ];
    let result = run_query(client, ROUTINE_DEFINITION_SQL, &params, 2).await?;
    Ok(result
        .rows
        .first()
        .and_then(|row| definition_from_row(routine, row))
        .map_or(RoutineLookup::NotFound, RoutineLookup::Found))
}

fn definition_from_row(routine: &RoutineRef, row: &[Value]) -> Option<RoutineDefinition> {
    let Some(Value::Text(object_type)) = row.first() else {
        return None;
    };
    let Some(Value::Text(text)) = row.get(1) else {
        return None;
    };
    let over_cap = matches!(row.get(2), Some(Value::Int(flag)) if *flag != 0);
    let (definition, cut) = cap_definition(text, ROUTINE_DEFINITION_LIMIT_BYTES);
    Some(RoutineDefinition {
        schema: routine.schema.clone(),
        name: routine.name.clone(),
        kind: routine_kind(object_type).to_string(),
        identity_arguments: String::new(),
        definition,
        total_bytes: None,
        truncated: cut || over_cap,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_definition_row_keeps_the_kind_and_flags_a_cut() {
        let routine = RoutineRef {
            schema: "dbo".into(),
            name: "p".into(),
            identity_arguments: None,
        };
        let row = vec![
            Value::Text("P ".into()),
            Value::Text("CREATE PROCEDURE p".into()),
            Value::Int(1),
        ];
        let definition = definition_from_row(&routine, &row).unwrap();
        assert_eq!(definition.kind, "procedure");
        assert!(definition.truncated);
        assert!(definition_from_row(&routine, &[Value::Text("P".into()), Value::Null, Value::Int(0)]).is_none());
    }

    #[test]
    fn supported_kinds_contain_expected_variants() {
        assert_eq!(SUPPORTED_KINDS.len(), 5);
        assert!(SUPPORTED_KINDS.contains(&CatalogObjectKind::Routine));
        assert!(SUPPORTED_KINDS.contains(&CatalogObjectKind::Trigger));
        assert!(SUPPORTED_KINDS.contains(&CatalogObjectKind::Sequence));
        assert!(SUPPORTED_KINDS.contains(&CatalogObjectKind::Type));
        assert!(SUPPORTED_KINDS.contains(&CatalogObjectKind::Role));
        assert!(!SUPPORTED_KINDS.contains(&CatalogObjectKind::Grant));
        assert!(!SUPPORTED_KINDS.contains(&CatalogObjectKind::Extension));
    }

    #[test]
    fn sql_for_matches_supported_kinds() {
        for &kind in SUPPORTED_KINDS {
            let sql = sql_for(kind);
            assert!(sql.is_some(), "missing sql for {kind:?}");
            if kind.is_schema_scoped() {
                assert!(sql.unwrap().contains("@P1"), "{kind:?} should filter on schema @P1");
                assert!(
                    sql.unwrap().contains("schema_name"),
                    "{kind:?} query should select schema_name"
                );
            }
        }
        assert!(sql_for(CatalogObjectKind::Grant).is_none());
        assert!(sql_for(CatalogObjectKind::Extension).is_none());
    }

    #[test]
    fn object_from_row_parses_schema_and_detail() {
        let row = vec![
            Value::Text("sales".into()),
            Value::Text("tr_orders_audit".into()),
            Value::Text("on orders".into()),
        ];
        let obj = object_from_row(CatalogObjectKind::Trigger, &row).unwrap();
        assert_eq!(obj.kind, CatalogObjectKind::Trigger);
        assert_eq!(obj.schema.as_deref(), Some("sales"));
        assert_eq!(obj.name, "tr_orders_audit");
        assert_eq!(obj.detail.as_deref(), Some("on orders"));
    }

    #[test]
    fn object_from_row_handles_null_schema_and_missing_name() {
        let row_no_schema = vec![
            Value::Null,
            Value::Text("db_owner".into()),
            Value::Text("fixed database role".into()),
        ];
        let obj = object_from_row(CatalogObjectKind::Role, &row_no_schema).unwrap();
        assert_eq!(obj.kind, CatalogObjectKind::Role);
        assert_eq!(obj.schema, None);
        assert_eq!(obj.name, "db_owner");

        let row_missing_name = vec![Value::Text("sales".into())];
        assert!(object_from_row(CatalogObjectKind::Trigger, &row_missing_name).is_none());
    }
}
