use std::collections::HashSet;

use tablepro_core::{ColumnInfo, FilterSet, KEYSET_OFFSET_THRESHOLD, Value, build_filter_where, keyset_where_clause};

use order::resolved_order_by;

const LONG_TEXT_BYTES: usize = 1_048_576;
const PREVIEW_CHARS: usize = 1024;
const PARTIAL_MARKER: &str = "\\x01bookie-partial:";

mod mongodb;
mod order;
mod redis;

pub struct BrowseTarget<'a> {
    pub driver_id: &'a str,
    pub schema: Option<&'a str>,
    pub table: &'a str,
    pub columns: &'a [ColumnInfo],
    pub filter: &'a FilterSet,
    pub hidden_columns: Option<&'a HashSet<String>>,
    pub long_text_preview: bool,
}

#[derive(Debug)]
pub struct BoundQuery {
    pub sql: String,
    pub params: Vec<Value>,
    pub projected_columns: Option<Vec<usize>>,
    pub reverse_rows: bool,
}

#[derive(Debug)]
pub enum PageQuery {
    Native,
    Sql(BoundQuery),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PageDirection {
    Forward,
    Before,
    Last,
}

impl BrowseTarget<'_> {
    fn filtered_query(&self, projection: &str) -> Result<BoundQuery, String> {
        let filter = build_filter_where(self.driver_id, self.columns, self.filter).map_err(|e| e.to_string())?;
        let quote = |s| tablepro_core::sql_dialect::quote_ident(self.driver_id, s);
        let target = match self.schema {
            Some(schema) => format!("{}.{}", quote(schema), quote(self.table)),
            None => quote(self.table),
        };
        let mut query = BoundQuery {
            sql: format!("SELECT {projection} FROM {target}"),
            params: Vec::new(),
            projected_columns: None,
            reverse_rows: false,
        };
        if let Some((clause, params)) = filter {
            query.sql.push_str(" WHERE ");
            query.sql.push_str(&clause);
            query.params = params;
        }
        Ok(query)
    }

    pub fn count(&self) -> Result<BoundQuery, String> {
        self.filtered_query("COUNT(*)")
    }

    pub fn value_query(&self, column_index: usize, pk_values: &[Value]) -> Result<BoundQuery, String> {
        if self.driver_id == "mongodb" {
            return mongodb::value_query(self, column_index, pk_values);
        }
        if self.driver_id == "redis" {
            return redis::value_query(self, column_index, pk_values);
        }
        let (sql, params) = tablepro_core::sql_dialect::build_keyed_value_select(
            self.driver_id,
            self.schema,
            self.table,
            self.columns,
            column_index,
            pk_values,
        )
        .map_err(|error| error.to_string())?;
        Ok(BoundQuery {
            sql,
            params,
            projected_columns: None,
            reverse_rows: false,
        })
    }

    pub fn page(
        &self,
        offset: u64,
        limit: u64,
        sort: Option<(usize, bool)>,
        cursor: Option<&[Value]>,
    ) -> Result<PageQuery, String> {
        self.page_with(offset, limit, sort, cursor, false, PageDirection::Forward)
    }

    pub fn page_before(
        &self,
        offset: u64,
        limit: u64,
        sort: Option<(usize, bool)>,
        cursor: Option<&[Value]>,
    ) -> Result<PageQuery, String> {
        self.page_with(offset, limit, sort, cursor, false, PageDirection::Before)
    }

    pub fn page_last(&self, offset: u64, limit: u64, sort: Option<(usize, bool)>) -> Result<PageQuery, String> {
        self.page_with(offset, limit, sort, None, false, PageDirection::Last)
    }

    pub fn page_by_row_id(
        &self,
        offset: u64,
        limit: u64,
        sort: Option<(usize, bool)>,
        cursor: Option<&[Value]>,
    ) -> Result<PageQuery, String> {
        self.page_with(offset, limit, sort, cursor, true, PageDirection::Forward)
    }

    fn page_with(
        &self,
        offset: u64,
        limit: u64,
        sort: Option<(usize, bool)>,
        cursor: Option<&[Value]>,
        row_id_order: bool,
        direction: PageDirection,
    ) -> Result<PageQuery, String> {
        let (mut projection, projected) = self.projection();
        let previewing = match self.long_text_projection().filter(|_| projected.is_none()) {
            Some(preview) => {
                projection = preview;
                true
            }
            None => false,
        };
        let mut query = self.filtered_query(&projection)?;
        query.projected_columns = projected;
        let order = if sort.is_none() && matches!(self.driver_id, "mongodb" | "redis") {
            None
        } else {
            resolved_order_by(self.driver_id, self.columns, sort, row_id_order)
        };
        let keys: Vec<&str> = self
            .columns
            .iter()
            .filter(|c| c.primary_key)
            .map(|c| c.name.as_str())
            .collect();
        let before_is_supported = direction != PageDirection::Before || supports_reverse_keyset(self.driver_id);
        let cursor = valid_keyset_cursor(cursor, offset, sort, &keys).filter(|_| before_is_supported);
        let seek_last = direction == PageDirection::Last
            && offset >= KEYSET_OFFSET_THRESHOLD
            && sort.is_none()
            && !keys.is_empty()
            && supports_reverse_keyset(self.driver_id);
        let before = direction == PageDirection::Before;
        let cursor_applied = !seek_last
            && append_keyset_clause(
                &mut query,
                self.driver_id,
                &keys,
                cursor,
                before,
                self.filter.is_empty(),
            )?;
        let actual_offset = if seek_last || cursor_applied {
            0
        } else {
            if self.filter.is_empty() && order.is_none() && !previewing {
                return Ok(PageQuery::Native);
            }
            offset
        };
        let order = page_order(self.driver_id, &keys, order, (before && cursor_applied) || seek_last);
        query
            .sql
            .push_str(&tablepro_core::sql_dialect::build_order_and_pagination(
                self.driver_id,
                order.as_deref(),
                limit,
                actual_offset,
            ));
        query.reverse_rows = (before && cursor_applied) || seek_last;
        Ok(PageQuery::Sql(query))
    }

    fn long_text_projection(&self) -> Option<String> {
        if !self.long_text_preview || self.driver_id != "postgres" {
            return None;
        }
        let quote = |name: &str| tablepro_core::sql_dialect::quote_ident(self.driver_id, name);
        let mut any = false;
        let list: Vec<String> = self
            .columns
            .iter()
            .map(|column| {
                let name = quote(&column.name);
                if !is_postgres_text_type(&column.data_type) {
                    return name;
                }
                any = true;
                format!(
                    "CASE WHEN octet_length({name}) > {LONG_TEXT_BYTES} THEN E'{PARTIAL_MARKER}' || octet_length({name})::text \
                     || E'\\x01' || left({name}, {PREVIEW_CHARS}) ELSE {name} END AS {name}"
                )
            })
            .collect();
        any.then(|| list.join(", "))
    }

    fn projection(&self) -> (String, Option<Vec<usize>>) {
        let projected = self.projected_columns();
        let sql = projected.as_ref().map_or_else(
            || "*".to_owned(),
            |indices| {
                indices
                    .iter()
                    .map(|&index| tablepro_core::sql_dialect::quote_ident(self.driver_id, &self.columns[index].name))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        );
        (sql, projected)
    }

    fn projected_columns(&self) -> Option<Vec<usize>> {
        let hidden = self.hidden_columns?;
        if hidden.is_empty()
            || matches!(self.driver_id, "mongodb" | "redis")
            || !self.columns.iter().any(|column| column.primary_key)
        {
            return None;
        }
        let mut required = HashSet::new();
        for (index, column) in self.columns.iter().enumerate() {
            if !hidden.contains(&column.name) || column.primary_key {
                required.insert(index);
            }
        }
        (required.len() < self.columns.len())
            .then(|| (0..self.columns.len()).filter(|i| required.contains(i)).collect())
    }
}

fn is_postgres_text_type(data_type: &str) -> bool {
    let lower = data_type.trim().to_ascii_lowercase();
    lower == "text" || lower.starts_with("character varying") || lower.starts_with("varchar")
}

pub fn mark_partial_cells(result: &mut tablepro_core::QueryResult) {
    const MARKER: &str = "\u{1}bookie-partial:";
    for row in &mut result.rows {
        for cell in row {
            let Value::Text(text) = cell else {
                continue;
            };
            let Some(rest) = text.strip_prefix(MARKER) else {
                continue;
            };
            let Some((bytes, _preview)) = rest.split_once('\u{1}') else {
                continue;
            };
            if bytes.parse::<u64>().is_ok() {
                *cell = Value::Undecodable(format!("partial {bytes} bytes"));
            }
        }
    }
}

fn valid_keyset_cursor<'a>(
    cursor: Option<&'a [Value]>,
    offset: u64,
    sort: Option<(usize, bool)>,
    keys: &[&str],
) -> Option<&'a [Value]> {
    cursor.filter(|values| {
        offset >= KEYSET_OFFSET_THRESHOLD && sort.is_none() && !keys.is_empty() && values.len() == keys.len()
    })
}

fn supports_reverse_keyset(driver_id: &str) -> bool {
    matches!(driver_id, "postgres" | "mysql" | "sqlite" | "mssql" | "clickhouse")
}

fn page_order(driver_id: &str, keys: &[&str], order: Option<String>, reverse: bool) -> Option<String> {
    if !reverse {
        return order;
    }
    tablepro_core::keyset_order_by_before(driver_id, keys)
        .trim()
        .strip_prefix("ORDER BY")
        .map(str::trim)
        .map(str::to_owned)
}

fn append_keyset_clause(
    query: &mut BoundQuery,
    driver_id: &str,
    keys: &[&str],
    cursor: Option<&[Value]>,
    before: bool,
    filter_is_empty: bool,
) -> Result<bool, String> {
    let Some(cursor) = cursor else {
        return Ok(false);
    };
    let (clause, params) = if before {
        tablepro_core::keyset_before_where_clause(driver_id, keys, cursor, query.params.len())
    } else {
        keyset_where_clause(driver_id, keys, cursor, query.params.len())
    }
    .map_err(|error| error.to_string())?;
    query.sql.push_str(if filter_is_empty { " WHERE " } else { " AND " });
    query.sql.push_str(&clause);
    query.params.extend(params);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tablepro_core::{FilterOp, FilterRule, FilterValue};
    #[path = "reverse_paging.rs"]
    mod reverse_paging;
    #[path = "stable_order.rs"]
    mod stable_order;
    fn column(name: &str, primary_key: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: false,
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

    #[test]
    fn default_order_uses_every_primary_key_column() {
        let columns = vec![column("tenant", true), column("name", false), column("id", true)];
        assert_eq!(
            resolved_order_by("postgres", &columns, None, false).as_deref(),
            Some("\"tenant\" ASC, \"id\" ASC")
        );
    }

    #[test]
    fn explicit_sort_appends_primary_key_tie_breakers() {
        let columns = vec![column("tenant", true), column("name", false), column("id", true)];
        assert_eq!(
            resolved_order_by("postgres", &columns, Some((1, false)), false).as_deref(),
            Some("\"name\" DESC, \"tenant\" ASC, \"id\" ASC")
        );
    }

    #[test]
    fn sorted_primary_key_is_not_duplicated() {
        let columns = vec![column("tenant", true), column("id", true)];
        assert_eq!(
            resolved_order_by("postgres", &columns, Some((0, false)), false).as_deref(),
            Some("\"tenant\" DESC, \"id\" ASC")
        );
    }

    #[test]
    fn hidden_filter_column_stays_in_predicate_but_not_projection() {
        let columns = vec![
            column("tenant", true),
            column("name", false),
            column("status", false),
            column("secret", false),
        ];
        let filter = FilterSet {
            rules: vec![FilterRule {
                column: "status".into(),
                op: FilterOp::Eq,
                value: Some(FilterValue::Single("ready".into())),
            }],
            ..Default::default()
        };
        let hidden: HashSet<String> = ["status", "secret"].map(str::to_owned).into();
        let target = BrowseTarget {
            driver_id: "postgres",
            schema: None,
            table: "items",
            columns: &columns,
            filter: &filter,
            hidden_columns: Some(&hidden),
            long_text_preview: false,
        };

        let PageQuery::Sql(page) = target.page(0, 100, None, None).unwrap() else {
            panic!("expected projected SQL")
        };
        assert_eq!(
            page.sql,
            "SELECT \"tenant\", \"name\" FROM \"items\" WHERE \"status\" = $1 ORDER BY \"tenant\" ASC LIMIT 100 OFFSET 0"
        );
        assert_eq!(page.projected_columns, Some(vec![0, 1]));
        assert_eq!(page.params, vec![Value::Text("ready".into())]);
    }

    #[test]
    fn hidden_sort_column_stays_in_order_clause_and_composite_keys_stay_projected() {
        let columns = vec![
            column("tenant", true),
            column("id", true),
            column("name", false),
            column("rank", false),
            column("secret", false),
        ];
        let hidden: HashSet<String> = ["tenant", "rank", "secret"].map(str::to_owned).into();
        let target = BrowseTarget {
            driver_id: "postgres",
            schema: None,
            table: "items",
            columns: &columns,
            filter: &FilterSet::default(),
            hidden_columns: Some(&hidden),
            long_text_preview: false,
        };

        let PageQuery::Sql(page) = target.page(0, 100, Some((3, true)), None).unwrap() else {
            panic!("expected projected SQL")
        };
        assert!(page.sql.starts_with("SELECT \"tenant\", \"id\", \"name\" FROM"));
        assert_eq!(page.projected_columns, Some(vec![0, 1, 2]));
        assert!(
            page.sql
                .ends_with("ORDER BY \"rank\" ASC, \"tenant\" ASC, \"id\" ASC LIMIT 100 OFFSET 0")
        );
    }

    #[test]
    fn hidden_projection_falls_back_without_a_key() {
        let columns = vec![column("name", false), column("secret", false)];
        let hidden: HashSet<String> = ["secret"].map(str::to_owned).into();
        let target = BrowseTarget {
            driver_id: "postgres",
            schema: None,
            table: "items",
            columns: &columns,
            filter: &FilterSet::default(),
            hidden_columns: Some(&hidden),
            long_text_preview: false,
        };
        assert_eq!(target.projected_columns(), None);
    }

    #[test]
    fn hidden_projection_stays_disabled_when_no_non_key_column_is_hidden() {
        let columns = vec![column("id", true), column("name", false)];
        for hidden in [["id"], ["stale"]] {
            let hidden: HashSet<String> = hidden.map(str::to_owned).into();
            let target = BrowseTarget {
                driver_id: "postgres",
                schema: None,
                table: "items",
                columns: &columns,
                filter: &FilterSet::default(),
                hidden_columns: Some(&hidden),
                long_text_preview: false,
            };
            assert_eq!(target.projected_columns(), None);
        }
    }

    #[test]
    fn hidden_projection_stays_disabled_for_native_document_and_key_browsing() {
        let columns = vec![column("_id", true), column("name", false), column("secret", false)];
        let hidden: HashSet<String> = ["secret"].map(str::to_owned).into();
        for driver_id in ["mongodb", "redis"] {
            let target = BrowseTarget {
                driver_id,
                schema: None,
                table: "items",
                columns: &columns,
                filter: &FilterSet::default(),
                hidden_columns: Some(&hidden),
                long_text_preview: false,
            };
            assert_eq!(target.projected_columns(), None);
            assert!(matches!(target.page(0, 100, None, None).unwrap(), PageQuery::Native));
        }
    }

    #[test]
    fn hidden_raw_filter_column_stays_in_sql_but_not_projection() {
        let columns = vec![column("id", true), column("name", false), column("secret", false)];
        let filter = FilterSet {
            extra_sql: Some("secret <> ''".into()),
            ..Default::default()
        };
        let hidden: HashSet<String> = ["secret"].map(str::to_owned).into();
        let target = BrowseTarget {
            driver_id: "postgres",
            schema: None,
            table: "items",
            columns: &columns,
            filter: &filter,
            hidden_columns: Some(&hidden),
            long_text_preview: false,
        };

        let PageQuery::Sql(page) = target.page(0, 100, None, None).unwrap() else {
            panic!("expected projected SQL")
        };
        assert!(page.sql.starts_with("SELECT \"id\", \"name\" FROM"));
        assert!(page.sql.contains("secret <> ''"));
        assert_eq!(page.projected_columns, Some(vec![0, 1]));
    }

    #[test]
    fn value_query_fetches_one_column_by_primary_key() {
        let columns = vec![column("tenant", true), column("id", true), column("payload", false)];
        let target = BrowseTarget {
            driver_id: "postgres",
            schema: Some("app"),
            table: "records",
            columns: &columns,
            filter: &FilterSet::default(),
            hidden_columns: None,
            long_text_preview: false,
        };
        let query = target
            .value_query(2, &[Value::Text("acme".into()), Value::Int(7)])
            .unwrap();

        assert_eq!(
            query.sql,
            "SELECT \"payload\" FROM \"app\".\"records\" WHERE \"tenant\" = $1 AND \"id\" = $2 ORDER BY \"tenant\" ASC, \"id\" ASC LIMIT 2 OFFSET 0"
        );
        assert_eq!(query.params, vec![Value::Text("acme".into()), Value::Int(7)]);
    }

    #[test]
    fn unfiltered_unsorted_non_sql_fetch_stays_native_with_a_primary_key() {
        let columns = vec![column("_id", true)];
        for driver_id in ["mongodb", "redis"] {
            let target = BrowseTarget {
                driver_id,
                schema: None,
                table: "stats",
                columns: &columns,
                filter: &FilterSet::default(),
                hidden_columns: None,
                long_text_preview: false,
            };
            assert!(matches!(target.page(0, 100, None, None).unwrap(), PageQuery::Native));
        }
    }

    #[test]
    fn preview_projection_is_postgres_only_and_opt_in() {
        let columns = [column("id", true), column("body", false)];
        let filter = FilterSet::default();
        let target = |driver_id, long_text_preview| BrowseTarget {
            driver_id,
            schema: None,
            table: "notes",
            columns: &columns,
            filter: &filter,
            hidden_columns: None,
            long_text_preview,
        };
        let PageQuery::Sql(on) = target("postgres", true).page(0, 10, None, None).unwrap() else {
            panic!("preview pages run as SQL")
        };
        assert!(on.sql.contains("octet_length(\"body\") > 1048576"), "{}", on.sql);
        for (driver, preview) in [("postgres", false), ("mysql", true), ("sqlite", true)] {
            if let PageQuery::Sql(other) = target(driver, preview).page(0, 10, None, None).unwrap() {
                assert!(!other.sql.contains("octet_length"), "{driver} {preview}: {}", other.sql);
            }
        }
    }

    #[test]
    fn only_a_well_formed_marker_becomes_a_partial_cell() {
        let mut result = tablepro_core::QueryResult {
            columns: vec![],
            rows: vec![vec![
                Value::Text("\u{1}bookie-partial:2097152\u{1}xxxx".into()),
                Value::Text("\u{1}bookie-partial:not-a-number\u{1}x".into()),
                Value::Text("plain".into()),
                Value::Null,
            ]],
            truncated: false,
        };
        mark_partial_cells(&mut result);
        assert_eq!(result.rows[0][0], Value::Undecodable("partial 2097152 bytes".into()));
        assert!(matches!(&result.rows[0][1], Value::Text(_)));
        assert_eq!(result.rows[0][2], Value::Text("plain".into()));
        assert_eq!(result.rows[0][3], Value::Null);
    }
}
