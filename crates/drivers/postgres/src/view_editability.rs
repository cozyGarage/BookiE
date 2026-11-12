use std::collections::{HashMap, HashSet};

use bookie_core::{DriverError, ViewEditability};
use sqlx::{Pool, Postgres, Row};

use crate::map_sqlx_error;

const MAX_VIEW_DEPTH: usize = 8;
const EVENT_UPDATE: i32 = 1 << 2;
const EVENT_INSERT: i32 = 1 << 3;
const EVENT_DELETE: i32 = 1 << 4;

macro_rules! relation_sql {
    ($tail:literal) => {
        concat!(
            "SELECT c.oid::int8, c.relkind::text, \
    pg_catalog.pg_relation_is_updatable(c.oid, false), \
    EXISTS (SELECT 1 FROM pg_catalog.pg_trigger t WHERE t.tgrelid = c.oid AND NOT t.tgisinternal \
        AND (t.tgtype & 64) <> 0), \
    EXISTS (SELECT 1 FROM pg_catalog.pg_rewrite r WHERE r.ev_class = c.oid AND r.rulename <> '_RETURN'), \
    (SELECT r.ev_action::text FROM pg_catalog.pg_rewrite r WHERE r.ev_class = c.oid AND r.rulename = '_RETURN') \
    FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace ",
            $tail
        )
    };
}

const RELATION_BY_NAME_SQL: &str = relation_sql!(
    "WHERE (($2::text IS NULL AND c.relname = $1 AND pg_catalog.pg_table_is_visible(c.oid)) \
     OR (n.nspname = $2 AND c.relname = $1))"
);

const RELATION_BY_OID_SQL: &str = relation_sql!("WHERE c.oid = $1::int8::oid");

const COLUMNS_SQL: &str = "SELECT a.attnum::int4, a.attname::text, \
    pg_catalog.pg_column_is_updatable(a.attrelid, a.attnum, false) \
    FROM pg_catalog.pg_attribute a \
    WHERE a.attrelid = $1::int8::oid AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum";

const PRIMARY_KEY_SQL: &str = "SELECT k.attnum::int4 FROM pg_catalog.pg_constraint c, \
    LATERAL pg_catalog.unnest(c.conkey) WITH ORDINALITY AS k(attnum, position) \
    WHERE c.conrelid = $1::int8::oid AND c.contype = 'p' ORDER BY k.position";

struct Relation {
    oid: i64,
    relkind: String,
    events: i32,
    instead_trigger: bool,
    extra_rule: bool,
    definition: Option<String>,
}

impl Relation {
    fn automatically_updatable(&self) -> bool {
        let both = EVENT_UPDATE | EVENT_DELETE;
        !self.instead_trigger && !self.extra_rule && self.events & both == both
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Origin {
    pub(crate) table: i64,
    pub(crate) column: i32,
}

fn field(chunk: &str, name: &str) -> Option<i64> {
    let start = chunk.find(name)? + name.len();
    chunk[start..].split_whitespace().next()?.parse().ok()
}

pub(crate) fn target_origins(definition: &str) -> HashMap<i32, Origin> {
    let mut origins = HashMap::new();
    for chunk in definition.split("{TARGETENTRY").skip(1) {
        if chunk.contains(":resjunk true") {
            continue;
        }
        let (Some(resno), Some(table), Some(column)) = (
            field(chunk, ":resno "),
            field(chunk, ":resorigtbl "),
            field(chunk, ":resorigcol "),
        ) else {
            continue;
        };
        if table != 0 {
            origins.insert(
                resno as i32,
                Origin {
                    table,
                    column: column as i32,
                },
            );
        }
    }
    origins
}

async fn relation(pool: &Pool<Postgres>, schema: Option<&str>, table: &str) -> Result<Option<Relation>, DriverError> {
    let row = sqlx::query(RELATION_BY_NAME_SQL)
        .bind(table)
        .bind(schema)
        .fetch_optional(pool)
        .await
        .map_err(map_sqlx_error)?;
    row.map(relation_from_row).transpose()
}

async fn relation_by_oid(pool: &Pool<Postgres>, oid: i64) -> Result<Option<Relation>, DriverError> {
    let row = sqlx::query(RELATION_BY_OID_SQL)
        .bind(oid)
        .fetch_optional(pool)
        .await
        .map_err(map_sqlx_error)?;
    row.map(relation_from_row).transpose()
}

fn relation_from_row(row: sqlx::postgres::PgRow) -> Result<Relation, DriverError> {
    Ok(Relation {
        oid: row.try_get(0).map_err(map_sqlx_error)?,
        relkind: row.try_get(1).map_err(map_sqlx_error)?,
        events: row.try_get(2).map_err(map_sqlx_error)?,
        instead_trigger: row.try_get(3).map_err(map_sqlx_error)?,
        extra_rule: row.try_get(4).map_err(map_sqlx_error)?,
        definition: row.try_get(5).map_err(map_sqlx_error)?,
    })
}

struct ViewColumn {
    attnum: i32,
    name: String,
    updatable: bool,
}

async fn view_columns(pool: &Pool<Postgres>, oid: i64) -> Result<Vec<ViewColumn>, DriverError> {
    let rows = sqlx::query(COLUMNS_SQL)
        .bind(oid)
        .fetch_all(pool)
        .await
        .map_err(map_sqlx_error)?;
    rows.iter()
        .map(|row| {
            Ok(ViewColumn {
                attnum: row.try_get(0).map_err(map_sqlx_error)?,
                name: row.try_get(1).map_err(map_sqlx_error)?,
                updatable: row.try_get(2).map_err(map_sqlx_error)?,
            })
        })
        .collect()
}

async fn primary_key(pool: &Pool<Postgres>, oid: i64) -> Result<Vec<i32>, DriverError> {
    let rows = sqlx::query(PRIMARY_KEY_SQL)
        .bind(oid)
        .fetch_all(pool)
        .await
        .map_err(map_sqlx_error)?;
    rows.iter().map(|row| row.try_get(0).map_err(map_sqlx_error)).collect()
}

fn single_base(origins: &HashMap<i32, Origin>) -> Option<i64> {
    let tables: HashSet<i64> = origins.values().map(|origin| origin.table).collect();
    (tables.len() == 1).then(|| tables.into_iter().next()).flatten()
}

fn key_columns(view: &[ViewColumn], mapping: &HashMap<i32, i32>, primary_key: &[i32]) -> Option<Vec<String>> {
    if primary_key.is_empty() {
        return None;
    }
    primary_key
        .iter()
        .map(|base| {
            view.iter()
                .find(|column| mapping.get(&column.attnum) == Some(base))
                .map(|column| column.name.clone())
        })
        .collect()
}

pub(crate) async fn fetch(
    pool: &Pool<Postgres>,
    schema: Option<&str>,
    table: &str,
) -> Result<ViewEditability, DriverError> {
    let Some(top) = relation(pool, schema, table).await? else {
        return Ok(ViewEditability::NotApplicable);
    };
    if top.relkind != "v" {
        return Ok(ViewEditability::NotApplicable);
    }
    if !top.automatically_updatable() {
        return Ok(ViewEditability::NotUpdatable);
    }
    let columns = view_columns(pool, top.oid).await?;
    let mut mapping: HashMap<i32, i32> = columns.iter().map(|column| (column.attnum, column.attnum)).collect();
    let insertable = top.events & EVENT_INSERT != 0;
    let mut current = top;
    for _ in 0..MAX_VIEW_DEPTH {
        let origins = target_origins(current.definition.as_deref().unwrap_or_default());
        let Some(base_oid) = single_base(&origins) else {
            return Ok(ViewEditability::NotUpdatable);
        };
        mapping = mapping
            .into_iter()
            .filter_map(|(view, via)| origins.get(&via).map(|origin| (view, origin.column)))
            .collect();
        let Some(base) = relation_by_oid(pool, base_oid).await? else {
            return Ok(ViewEditability::NotUpdatable);
        };
        match base.relkind.as_str() {
            "v" if base.automatically_updatable() => current = base,
            "v" => return Ok(ViewEditability::NotUpdatable),
            "r" | "p" => {
                let keys = primary_key(pool, base.oid).await?;
                return Ok(match key_columns(&columns, &mapping, &keys) {
                    Some(key_columns) => ViewEditability::Editable {
                        key_columns,
                        read_only_columns: columns
                            .iter()
                            .filter(|column| !column.updatable)
                            .map(|column| column.name.clone())
                            .collect(),
                        insertable,
                    },
                    None => ViewEditability::KeyNotExposed,
                });
            }
            _ => return Ok(ViewEditability::KeyNotExposed),
        }
    }
    Ok(ViewEditability::NotUpdatable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_column_origins_are_read_from_the_stored_view_query() {
        let definition = "({QUERY :targetList ({TARGETENTRY :expr {VAR :varno 3 :varattno 1} :resno 1 :resname id \
            :ressortgroupref 0 :resorigtbl 16390 :resorigcol 1 :resjunk false} {TARGETENTRY :expr {OPEXPR} :resno 2 \
            :resname doubled :ressortgroupref 0 :resorigtbl 0 :resorigcol 0 :resjunk false} {TARGETENTRY :expr {VAR} \
            :resno 3 :resname label :ressortgroupref 0 :resorigtbl 16390 :resorigcol 2 :resjunk false}) :override 0})";
        let origins = target_origins(definition);
        assert_eq!(origins.len(), 2);
        assert_eq!(
            origins[&1],
            Origin {
                table: 16390,
                column: 1
            }
        );
        assert_eq!(
            origins[&3],
            Origin {
                table: 16390,
                column: 2
            }
        );
        assert_eq!(single_base(&origins), Some(16390));
    }

    #[test]
    fn a_view_over_two_tables_has_no_single_base() {
        let origins = HashMap::from([(1, Origin { table: 1, column: 1 }), (2, Origin { table: 2, column: 1 })]);
        assert_eq!(single_base(&origins), None);
    }

    #[test]
    fn every_primary_key_column_must_be_exposed() {
        let view = [
            ViewColumn {
                attnum: 1,
                name: "pk".into(),
                updatable: true,
            },
            ViewColumn {
                attnum: 2,
                name: "label".into(),
                updatable: true,
            },
        ];
        let mapping = HashMap::from([(1, 5), (2, 6)]);
        assert_eq!(key_columns(&view, &mapping, &[5]), Some(vec!["pk".into()]));
        assert_eq!(key_columns(&view, &mapping, &[5, 7]), None);
        assert_eq!(key_columns(&view, &mapping, &[]), None);
    }
}
