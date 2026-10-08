//! Pure SQL planning shared by browse page and count requests.
//! Native unfiltered fetches stay available for non-SQL drivers.
use std::collections::HashSet;

use tablepro_core::{ColumnInfo, FilterSet, KEYSET_OFFSET_THRESHOLD, Value, build_filter_where, keyset_where_clause};

pub(crate) struct BrowseTarget<'a> {
    pub driver_id: &'a str,
    pub schema: Option<&'a str>,
    pub table: &'a str,
    pub columns: &'a [ColumnInfo],
    pub filter: &'a FilterSet,
    pub hidden_columns: Option<&'a HashSet<String>>,
}

#[derive(Debug)]
pub(crate) struct BoundQuery {
    pub sql: String,
    pub params: Vec<Value>,
    pub projected_columns: Option<Vec<usize>>,
}

#[derive(Debug)]
pub(crate) enum PageQuery {
    Native,
    Sql(BoundQuery),
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
        if matches!(self.driver_id, "mongodb" | "redis") {
            return Err(format!("single-cell fetch is unsupported for {}", self.driver_id));
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
        })
    }

    pub fn page(
        &self,
        offset: u64,
        limit: u64,
        sort: Option<(usize, bool)>,
        cursor: Option<&[Value]>,
    ) -> Result<PageQuery, String> {
        let (projection, projected) = self.projection();
        let mut query = self.filtered_query(&projection)?;
        query.projected_columns = projected;
        // These drivers use their native page readers; SQL ordering by the
        // inferred key would force an otherwise-unfiltered browse through the
        // SQL query path, which their command grammar does not support.
        let order = if sort.is_none() && matches!(self.driver_id, "mongodb" | "redis") {
            None
        } else {
            resolved_order_by(self.driver_id, self.columns, sort)
        };
        let keys: Vec<&str> = self
            .columns
            .iter()
            .filter(|c| c.primary_key)
            .map(|c| c.name.as_str())
            .collect();
        let cursor = cursor.filter(|c| {
            offset >= KEYSET_OFFSET_THRESHOLD && sort.is_none() && !keys.is_empty() && c.len() == keys.len()
        });
        let actual_offset = if let Some(cursor) = cursor {
            let (clause, params) =
                keyset_where_clause(self.driver_id, &keys, cursor, query.params.len()).map_err(|e| e.to_string())?;
            query
                .sql
                .push_str(if self.filter.is_empty() { " WHERE " } else { " AND " });
            query.sql.push_str(&clause);
            query.params.extend(params);
            0
        } else {
            if self.filter.is_empty() && order.is_none() {
                return Ok(PageQuery::Native);
            }
            offset
        };
        query
            .sql
            .push_str(&tablepro_core::sql_dialect::build_order_and_pagination(
                self.driver_id,
                order.as_deref(),
                limit,
                actual_offset,
            ));
        Ok(PageQuery::Sql(query))
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

fn resolved_order_by(driver_id: &str, columns: &[ColumnInfo], sort: Option<(usize, bool)>) -> Option<String> {
    let mut terms = Vec::new();
    let selected = sort.and_then(|(index, ascending)| {
        columns.get(index).map(|column| {
            let direction = if ascending { "ASC" } else { "DESC" };
            terms.push(format!(
                "{} {direction}",
                tablepro_core::sql_dialect::quote_ident(driver_id, &column.name)
            ));
            column.name.as_str()
        })
    });
    for column in columns.iter().filter(|column| column.primary_key) {
        if selected == Some(column.name.as_str()) {
            continue;
        }
        terms.push(format!(
            "{} ASC",
            tablepro_core::sql_dialect::quote_ident(driver_id, &column.name)
        ));
    }
    (!terms.is_empty()).then(|| terms.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tablepro_core::{FilterOp, FilterRule, FilterValue};
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
            resolved_order_by("postgres", &columns, None).as_deref(),
            Some("\"tenant\" ASC, \"id\" ASC")
        );
    }

    #[test]
    fn explicit_sort_appends_primary_key_tie_breakers() {
        let columns = vec![column("tenant", true), column("name", false), column("id", true)];
        assert_eq!(
            resolved_order_by("postgres", &columns, Some((1, false))).as_deref(),
            Some("\"name\" DESC, \"tenant\" ASC, \"id\" ASC")
        );
    }

    #[test]
    fn sorted_primary_key_is_not_duplicated() {
        let columns = vec![column("tenant", true), column("id", true)];
        assert_eq!(
            resolved_order_by("postgres", &columns, Some((0, false))).as_deref(),
            Some("\"tenant\" DESC, \"id\" ASC")
        );
    }

    #[test]
    fn table_without_pk_or_sort_has_no_promised_order() {
        assert_eq!(resolved_order_by("postgres", &[column("name", false)], None), None);
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
        };

        let PageQuery::Sql(page) = target.page(0, 100, None, None).unwrap() else {
            panic!("expected projected SQL")
        };
        assert!(page.sql.starts_with("SELECT \"id\", \"name\" FROM"));
        assert!(page.sql.contains("secret <> ''"));
        assert_eq!(page.projected_columns, Some(vec![0, 1]));
    }

    #[test]
    fn invalid_filter_refuses_both_page_and_count() {
        let filter = FilterSet {
            rules: vec![FilterRule {
                column: "missing".into(),
                op: FilterOp::Eq,
                value: Some(FilterValue::Single("x".into())),
            }],
            ..Default::default()
        };
        let target = BrowseTarget {
            driver_id: "postgres",
            schema: None,
            table: "items",
            columns: &[],
            filter: &filter,
            hidden_columns: None,
        };
        assert!(target.count().unwrap_err().contains("missing"));
        assert!(target.page(0, 100, None, None).unwrap_err().contains("missing"));
    }

    #[test]
    fn page_and_count_share_filter_and_keyset_parameters_follow_it() {
        let columns = vec![column("id", true), column("name", false)];
        let filter = FilterSet {
            rules: vec![FilterRule {
                column: "name".into(),
                op: FilterOp::Eq,
                value: Some(FilterValue::Single("Ada".into())),
            }],
            ..Default::default()
        };
        let target = BrowseTarget {
            driver_id: "postgres",
            schema: Some("odd\"schema"),
            table: "items",
            columns: &columns,
            filter: &filter,
            hidden_columns: None,
        };
        let count = target.count().unwrap();
        let PageQuery::Sql(page) = target.page(10_000, 100, None, Some(&[Value::Int(50)])).unwrap() else {
            panic!("expected SQL")
        };
        assert_eq!(
            count.sql,
            "SELECT COUNT(*) FROM \"odd\"\"schema\".\"items\" WHERE \"name\" = $1"
        );
        assert!(page.sql.contains("\"name\" = $1 AND \"id\" > $2"));
        assert!(!page.sql.contains("10000"));
        assert_eq!(page.params, vec![Value::Text("Ada".into()), Value::Int(50)]);
        assert_eq!(count.params, vec![Value::Text("Ada".into())]);
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
    fn explicit_sort_ignores_a_keyset_cursor_and_keeps_offset_pagination() {
        let columns = vec![column("id", true), column("rank", false)];
        let target = BrowseTarget {
            driver_id: "postgres",
            schema: None,
            table: "items",
            columns: &columns,
            filter: &FilterSet::default(),
            hidden_columns: None,
        };
        let PageQuery::Sql(page) = target
            .page(KEYSET_OFFSET_THRESHOLD, 25, Some((1, true)), Some(&[Value::Int(5)]))
            .unwrap()
        else {
            panic!("expected offset SQL")
        };
        assert!(page.sql.contains("ORDER BY \"rank\" ASC, \"id\" ASC"));
        assert!(
            page.sql
                .ends_with(&format!("LIMIT 25 OFFSET {KEYSET_OFFSET_THRESHOLD}"))
        );
        assert!(!page.sql.contains(" > "));
        assert!(page.params.is_empty());
    }

    #[test]
    fn incomplete_composite_cursor_is_ignored_and_keeps_offset_pagination() {
        let columns = vec![column("tenant", true), column("id", true), column("name", false)];
        let target = BrowseTarget {
            driver_id: "postgres",
            schema: None,
            table: "items",
            columns: &columns,
            filter: &FilterSet::default(),
            hidden_columns: None,
        };
        let PageQuery::Sql(page) = target
            .page(KEYSET_OFFSET_THRESHOLD, 25, None, Some(&[Value::Int(5)]))
            .unwrap()
        else {
            panic!("expected offset SQL")
        };
        assert!(
            page.sql
                .ends_with(&format!("LIMIT 25 OFFSET {KEYSET_OFFSET_THRESHOLD}"))
        );
        assert!(!page.sql.contains(" > "));
        assert!(page.params.is_empty());
    }

    #[test]
    fn keyset_cursor_is_ignored_when_the_table_has_no_primary_key() {
        let columns = vec![column("name", false)];
        let target = BrowseTarget {
            driver_id: "postgres",
            schema: None,
            table: "items",
            columns: &columns,
            filter: &FilterSet::default(),
            hidden_columns: None,
        };
        assert!(matches!(
            target
                .page(KEYSET_OFFSET_THRESHOLD, 25, None, Some(&[Value::Int(5)]))
                .unwrap(),
            PageQuery::Native
        ));
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
            };
            assert!(matches!(target.page(0, 100, None, None).unwrap(), PageQuery::Native));
        }
    }

    #[tokio::test]
    async fn sqlite_hidden_projection_keeps_the_primary_key_and_leaves_hidden_values_stored() {
        use tablepro_core::{ConnectOptions, DatabaseDriver};

        let directory = tempfile::tempdir().unwrap();
        let connection = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: directory.path().join("projection.db").to_string_lossy().into_owned(),
                ..Default::default()
            })
            .await
            .unwrap();
        let control = crate::services::operation_control::bounded(30);
        connection
            .execute_controlled(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, secret TEXT)",
                &control,
            )
            .await
            .unwrap();
        connection
            .execute_controlled("INSERT INTO items VALUES (7, 'Ada', 'still stored')", &control)
            .await
            .unwrap();

        let columns = vec![column("id", true), column("name", false), column("secret", false)];
        let hidden: HashSet<String> = ["secret"].map(str::to_owned).into();
        let target = BrowseTarget {
            driver_id: "sqlite",
            schema: None,
            table: "items",
            columns: &columns,
            filter: &FilterSet::default(),
            hidden_columns: Some(&hidden),
        };
        let PageQuery::Sql(query) = target.page(0, 10, None, None).unwrap() else {
            panic!("expected projected SQL")
        };
        assert_eq!(query.projected_columns, Some(vec![0, 1]));
        let projected = connection
            .query_params_controlled(&query.sql, &query.params, &control)
            .await
            .unwrap();
        assert_eq!(projected.rows, vec![vec![Value::Int(7), Value::Text("Ada".into())]]);

        let native = connection
            .query_controlled("SELECT secret FROM items WHERE id = 7", &control)
            .await
            .unwrap();
        assert_eq!(native.rows, vec![vec![Value::Text("still stored".into())]]);
    }

    #[tokio::test]
    async fn sqlite_value_query_refetches_the_exact_blob_for_a_composite_key() {
        use tablepro_core::{ConnectOptions, DatabaseDriver};

        let directory = tempfile::tempdir().unwrap();
        let connection = drivers_sqlite::SqliteDriver
            .connect(ConnectOptions {
                database: directory.path().join("value-fetch.db").to_string_lossy().into_owned(),
                ..Default::default()
            })
            .await
            .unwrap();
        let control = crate::services::operation_control::bounded(30);
        connection
            .execute_controlled(
                "CREATE TABLE records (tenant TEXT, id INTEGER, payload BLOB, PRIMARY KEY (tenant, id))",
                &control,
            )
            .await
            .unwrap();
        let tenant = "tenant ' OR 1=1 --";
        let bytes: Vec<u8> = (0..9000).map(|index| (index % 256) as u8).collect();
        connection
            .execute_params_controlled(
                "INSERT INTO records VALUES (?1, ?2, ?3)",
                &[Value::Text(tenant.into()), Value::Int(7), Value::Bytes(bytes.clone())],
                &control,
            )
            .await
            .unwrap();
        let native = connection
            .query_controlled(
                "SELECT typeof(payload), length(payload), hex(substr(payload, 1, 8)) FROM records WHERE tenant = 'tenant '' OR 1=1 --' AND id = 7",
                &control,
            )
            .await
            .unwrap();
        assert_eq!(
            native.rows,
            vec![vec![
                Value::Text("blob".into()),
                Value::Int(9000),
                Value::Text("0001020304050607".into())
            ]]
        );

        let columns = vec![column("tenant", true), column("id", true), column("payload", false)];
        let target = BrowseTarget {
            driver_id: "sqlite",
            schema: None,
            table: "records",
            columns: &columns,
            filter: &FilterSet::default(),
            hidden_columns: None,
        };
        let query = target
            .value_query(2, &[Value::Text(tenant.into()), Value::Int(7)])
            .unwrap();
        let fetched = connection
            .query_params_controlled(&query.sql, &query.params, &control)
            .await
            .unwrap();
        assert_eq!(fetched.rows, vec![vec![Value::Bytes(bytes)]]);
    }

    #[tokio::test]
    #[ignore = "requires docker"]
    async fn postgres_value_query_refetches_by_enum_and_domain_composite_key() {
        use tablepro_core::{ConnectOptions, DatabaseDriver, OperationControl};
        use testcontainers::ImageExt;
        use testcontainers::runners::AsyncRunner;
        use testcontainers_modules::postgres::Postgres;

        let container = Postgres::default().with_tag("16-alpine").start().await.unwrap();
        let connection = drivers_postgres::PgDriver
            .connect(ConnectOptions {
                host: container.get_host().await.unwrap().to_string(),
                port: container.get_host_port_ipv4(5432).await.unwrap(),
                database: "postgres".into(),
                username: "postgres".into(),
                password: secrecy::SecretString::new("postgres".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        let control = OperationControl::with_timeout(std::time::Duration::from_secs(30));
        for sql in [
            "CREATE TYPE value_preview_state AS ENUM ('ready')",
            "CREATE DOMAIN value_preview_tenant AS text CHECK (VALUE <> '')",
            "CREATE TABLE value_preview_records (state value_preview_state, tenant value_preview_tenant, payload bytea, PRIMARY KEY (state, tenant))",
            "INSERT INTO value_preview_records VALUES ('ready', 'tenant '' OR 1=1 --', decode(repeat('ab', 9000), 'hex'))",
        ] {
            connection.execute_controlled(sql, &control).await.unwrap();
        }
        let columns = connection
            .fetch_columns_controlled(None, "value_preview_records", &control)
            .await
            .unwrap();
        assert!(columns[0].enum_type.is_some());
        assert!(columns[1].domain_type.is_some());
        let native = connection
            .query_controlled(
                "SELECT pg_typeof(state)::text, state::text, pg_typeof(tenant)::text, tenant::text, octet_length(payload), encode(substring(payload from 1 for 8), 'hex') FROM value_preview_records",
                &control,
            )
            .await
            .unwrap();
        assert_eq!(
            native.rows,
            vec![vec![
                Value::Text("value_preview_state".into()),
                Value::Text("ready".into()),
                Value::Text("value_preview_tenant".into()),
                Value::Text("tenant ' OR 1=1 --".into()),
                Value::Int(9000),
                Value::Text("abababababababab".into()),
            ]]
        );

        let target = BrowseTarget {
            driver_id: "postgres",
            schema: None,
            table: "value_preview_records",
            columns: &columns,
            filter: &FilterSet::default(),
            hidden_columns: None,
        };
        let query = target
            .value_query(
                2,
                &[Value::Text("ready".into()), Value::Text("tenant ' OR 1=1 --".into())],
            )
            .unwrap();
        let fetched = connection
            .query_params_controlled(&query.sql, &query.params, &control)
            .await
            .unwrap();
        assert_eq!(fetched.rows, vec![vec![Value::Bytes(vec![0xab; 9000])]]);
    }
}
