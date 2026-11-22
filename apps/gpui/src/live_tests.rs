    #[test]
    fn query_failure_preserves_server_position_without_inventing_one() {
        use bookie_app_services::driver_error_presentation::DriverFailurePresentation as Failure;
        let error = super::QueryFailure::from(Failure::Query {
            message: "unknown column".into(),
            sqlstate: Some("42703".into()),
            position: Some(17),
            hint: Some("try another name".into()),
        });
        assert_eq!(error.position, Some(17));
        assert_eq!(error.hint.as_deref(), Some("try another name"));
        assert!(error.message.contains("unknown column"));
        assert_eq!(super::QueryFailure::from(Failure::TimedOut).position, None);
    }

    #[test]
    fn table_count_sql_uses_a_wide_count_for_sql_server_and_quotes_schema_names() {
        let table = TableInfo {
            schema: Some("orders schema".into()),
            name: "order]items".into(),
        };
        assert_eq!(
            super::table_count_sql("mssql", &table),
            "SELECT COUNT_BIG(*) FROM [orders schema].[order]]items]"
        );
        assert_eq!(
            super::table_count_sql("postgres", &table),
            "SELECT COUNT(*) FROM \"orders schema\".\"order]items\""
        );
    }

    #[test]
    fn a_write_session_is_refused_when_the_saved_connection_changed_since_the_table_opened() {
        assert!(super::refuse_edited_connection(Some("a"), "a").is_ok());
        assert!(super::refuse_edited_connection(None, "b").is_ok());
        assert!(super::refuse_edited_connection(Some("a"), "b").is_err());
    }

    use super::*;

    #[test]
    fn a_stale_open_event_closes_its_session() {
        let Origin::File(source) = fixture() else {
            panic!("the fixture is a file")
        };
        let file = FixtureFile(std::env::temp_dir().join(format!("bookie-gpui-stale-{}.sqlite", uuid::Uuid::new_v4())));
        std::fs::copy(source, &file.0).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let database = Arc::new(DatabaseService::new_isolated());
        let sessions = ReadSessionStore::new(Arc::new(drivers_registry::default_registry()), database.clone());
        let control = OperationControl::with_timeout(Duration::from_secs(5));
        let opened = runtime.block_on(sessions.open_file(&file.0, &control)).unwrap();

        let result = keep_current_event(
            DispatchContext::Sessions(&sessions),
            Event::SessionOpened {
                epoch: Epoch(1),
                outcome: Ok(opened),
            },
            Epoch(2),
        );

        assert!(result.is_err());
        assert!(database.connection_overview().is_empty());
    }

    #[test]
    fn declared_number_types_are_numeric_and_text_types_are_not() {
        for numeric in [
            "INTEGER",
            "bigint",
            "NUMERIC(10,2)",
            "double precision",
            "BIGSERIAL",
            "REAL",
            "decimal",
        ] {
            assert!(numeric_type(numeric), "{numeric}");
        }
        for other in ["TEXT", "varchar(20)", "timestamp", "uuid", "BLOB", ""] {
            assert!(!numeric_type(other), "{other}");
        }
    }

    struct FixtureFile(std::path::PathBuf);

    impl Drop for FixtureFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn oversized_fixture() -> FixtureFile {
        let path = std::env::temp_dir().join(format!("bookie-gpui-budget-{}.sqlite", uuid::Uuid::new_v4()));
        std::fs::File::create(&path).unwrap();
        let file = FixtureFile(path);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let registry = drivers_registry::default_registry();
            let driver = registry.get("sqlite").unwrap();
            let connection = driver
                .connect(bookie_core::ConnectOptions {
                    database: file.0.to_string_lossy().into_owned(),
                    ..Default::default()
                })
                .await
                .unwrap();
            connection.execute("CREATE TABLE wide (value BLOB)").await.unwrap();
            connection
                .execute("INSERT INTO wide VALUES (zeroblob(67108865))")
                .await
                .unwrap();
            connection.close().await.unwrap();
        });
        file
    }

    #[test]
    fn a_driver_truncated_page_is_reported_as_incomplete() {
        let file = oversized_fixture();
        let table = LiveTable::load(
            &Origin::File(file.0.clone()),
            &TableInfo {
                schema: None,
                name: "wide".into(),
            },
            100,
        )
        .unwrap();
        assert_eq!(table.total(), 0);
        assert!(table.truncated);
    }

    #[test]
    fn an_empty_or_oversized_row_cap_is_refused() {
        let session = LiveSession::open(&fixture()).unwrap();
        assert!(
            session
                .load_table(
                    &TableInfo {
                        schema: None,
                        name: "orders".into()
                    },
                    0
                )
                .is_err()
        );
        assert!(
            session
                .load_table(
                    &TableInfo {
                        schema: None,
                        name: "orders".into()
                    },
                    ROW_CAP + 1
                )
                .is_err()
        );
    }

    #[test]
    fn unsupported_server_sort_is_refused_instead_of_sorting_a_partial_page() {
        let session = LiveSession::open(&fixture()).unwrap();
        let sort = bookie_app_services::read_session::TableSort {
            column: "customer".into(),
            ascending: true,
        };
        let result = session.load_table_sorted_controlled(
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
            Some(&sort),
            &CancellationToken::new(),
        );
        assert!(matches!(result, Err(message) if message.contains("Server-side table sorting is not available")));
    }

    #[test]
    fn a_cancelled_live_load_is_not_returned_as_a_complete_table() {
        let session = LiveSession::open(&fixture()).unwrap();
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert!(
            session
                .load_table_controlled(
                    &TableInfo {
                        schema: None,
                        name: "orders".into()
                    },
                    100,
                    &cancellation
                )
                .is_err()
        );
    }

    #[test]
    fn decoded_budget_is_shared_across_pages_and_keeps_a_whole_row_prefix() {
        let make_page = || QueryResult {
            columns: Vec::new(),
            rows: bookie_core::QueryRows::from_rows(
                (0..40)
                    .map(|index| {
                        vec![
                            bookie_core::Value::Int(index),
                            bookie_core::Value::Text("x".repeat(1024 * 1024)),
                        ]
                    })
                    .collect(),
            ),
            truncated: false,
        };
        let mut budget = QueryResultBudget::default();
        let mut first = make_page();
        assert!(!bound_page(&mut first, &mut budget));
        assert_eq!(first.rows.len(), 40);
        drop(first);

        let mut second = make_page();
        assert!(bound_page(&mut second, &mut budget));
        assert!(second.truncated);
        assert!((1..40).contains(&second.rows.len()));
        for (index, row) in second.rows.iter().enumerate() {
            assert_eq!(row[0], bookie_core::Value::Int(index as i64));
        }
    }

    #[test]
    fn a_cursor_page_larger_than_the_remaining_row_cap_is_truncated() {
        let mut page = QueryResult {
            columns: Vec::new(),
            rows: QueryRows::from_rows((0..3).map(|id| vec![bookie_core::Value::Int(id)]).collect()),
            truncated: false,
        };
        assert!(cap_page_rows(&mut page, 2));
        assert!(page.truncated);
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.rows[1][0], bookie_core::Value::Int(1));
    }

    #[test]
    fn a_query_result_is_bounded_in_place_and_keeps_its_typed_prefix() {
        let row = |index| {
            vec![
                bookie_core::Value::Int(index),
                bookie_core::Value::Text("x".repeat(1024 * 1024)),
            ]
        };
        let mut result = QueryResult {
            columns: Vec::new(),
            rows: QueryRows::from_rows((0..70).map(row).collect()),
            truncated: false,
        };
        let mut budget = QueryResultBudget::default();
        assert!(bound_query_result(&mut result, 100, &mut budget));
        assert!((1..70).contains(&result.rows.len()));
        for (index, retained) in result.rows.iter().enumerate() {
            assert_eq!(retained[0], bookie_core::Value::Int(index as i64));
        }
    }

    #[test]
    fn a_query_result_row_cap_keeps_only_the_prefix_and_marks_it_incomplete() {
        let mut result = QueryResult {
            columns: Vec::new(),
            rows: QueryRows::from_rows((0..3).map(|index| vec![bookie_core::Value::Int(index)]).collect()),
            truncated: false,
        };
        assert!(bound_query_result(&mut result, 2, &mut QueryResultBudget::default()));
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows.last(), Some([bookie_core::Value::Int(1)].as_slice()));
    }

    fn fixture() -> Origin {
        let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        Origin::File(std::path::PathBuf::from(manifest_dir).join("tests/fixtures/orders.sqlite"))
    }

    #[test]
    fn a_database_file_cannot_write_from_the_query_editor() {
        let Origin::File(source) = fixture() else {
            panic!("the fixture is a file")
        };
        let path = std::env::temp_dir().join(format!("bookie-gpui-read-only-{}.sqlite", uuid::Uuid::new_v4()));
        std::fs::copy(source, &path).unwrap();
        let file = FixtureFile(path);
        let live = LiveSession::open(&Origin::File(file.0.clone())).unwrap();
        assert!(live.run_query("DELETE FROM orders", 100).is_err());
        assert_eq!(live.run_query("SELECT id FROM orders", 100).unwrap().total(), 6);
    }

    #[test]
    fn a_bound_query_returns_the_matching_row_and_a_bound_write_is_refused() {
        let Origin::File(source) = fixture() else {
            panic!("the fixture is a file")
        };
        let path = std::env::temp_dir().join(format!("bookie-gpui-bound-{}.sqlite", uuid::Uuid::new_v4()));
        std::fs::copy(source, &path).unwrap();
        let file = FixtureFile(path);
        let live = LiveSession::open(&Origin::File(file.0.clone())).unwrap();
        let cancel = CancellationToken::new();
        let params = [bookie_core::Value::Int(2)];
        let found = live
            .run_query_bound("SELECT id FROM orders WHERE id = ?", &params, 100, &cancel)
            .unwrap();
        assert_eq!(found.total(), 1);
        assert_eq!(found.executed_sql(), Some("SELECT id FROM orders WHERE id = ?"));
        assert!(ResultSource::load_elapsed(&found).is_some());
        assert!(
            live.run_query_bound("DELETE FROM orders WHERE id = ?", &params, 100, &cancel)
                .is_err()
        );
        let later = live.run_query("SELECT id FROM orders", 100).unwrap();
        assert_eq!(later.total(), 6);
        assert_eq!(later.executed_sql(), Some("SELECT id FROM orders"));
        assert_eq!(found.executed_sql(), Some("SELECT id FROM orders WHERE id = ?"));
    }

    #[test]
    fn a_database_file_loads_as_a_live_table_with_real_columns_and_nulls() {
        let table = LiveTable::load(
            &fixture(),
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load");
        let names: Vec<&str> = table.columns().iter().map(|column| column.name.as_str()).collect();
        assert_eq!(names, ["id", "customer", "total", "paid", "note"]);
        let keys: Vec<bool> = table.columns().iter().map(|column| column.primary_key).collect();
        assert_eq!(keys, [true, false, false, false, false]);
        assert_eq!(table.total(), 6);
        assert!(!table.truncated);
        assert_eq!(table.row(1)[1], Cell::Text("Grace".into()));
        assert_eq!(table.row(1)[4], Cell::Null);
        assert_eq!(table.row(2)[4], Cell::Text(String::new()));
    }

    #[test]
    fn connection_status_uses_the_read_command_bridge() {
        let session = LiveSession::open(&fixture()).expect("open fixture");

        let overview = session.overview().expect("connection status");

        assert_eq!(overview.len(), 1);
        assert_eq!(overview[0].driver_id, "sqlite");
        assert_eq!(
            overview[0].health,
            bookie_app_services::database_service::ConnectionHealth::Healthy
        );
        assert_eq!(
            overview[0].tunnel,
            bookie_app_services::database_service::TunnelState::Direct
        );
    }

    #[test]
    fn find_and_filter_answer_from_the_loaded_rows() {
        let table = LiveTable::load(
            &fixture(),
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load");
        let cancel = CancellationToken::new();
        let found = table.find("ada", 10, &cancel).expect("find");
        assert_eq!(found.matches, vec![0, 3]);
        let columns = table.columns().to_vec();
        let predicate = crate::model::parse_predicate("customer = Ada", &columns).expect("predicate");
        assert_eq!(table.filter(&predicate, &cancel).expect("filter"), vec![0, 3]);
    }

    #[test]
    fn one_session_loads_each_of_its_tables() {
        let session = LiveSession::open(&fixture()).expect("open");
        assert_eq!(
            session.tables,
            [
                TableInfo {
                    schema: None,
                    name: "customers".into()
                },
                TableInfo {
                    schema: None,
                    name: "orders".into()
                }
            ]
        );
        assert_eq!(session.label, "orders.sqlite");
        let customers = session
            .load_table(
                &TableInfo {
                    schema: None,
                    name: "customers".into(),
                },
                100,
            )
            .expect("customers");
        let orders = session
            .load_table(
                &TableInfo {
                    schema: None,
                    name: "orders".into(),
                },
                100,
            )
            .expect("orders");
        assert_eq!((customers.total(), orders.total()), (2, 6));
        assert_eq!(customers.columns().len(), 2);
        assert_eq!(orders.columns().len(), 5);
    }

    #[test]
    fn an_explain_statement_for_the_driver_runs_in_the_read_only_session() {
        let session = LiveSession::open(&fixture()).expect("open");
        let statement = bookie_core::sql_dialect::explain_statement(&session.driver_id, "SELECT id FROM orders")
            .expect("sqlite can explain");
        let plan = session.run_query(&statement, 100).expect("plan");
        assert!(plan.total() >= 1);
    }

    #[test]
    fn a_query_becomes_a_grid_source_with_its_own_columns_and_cap() {
        let session = LiveSession::open(&fixture()).expect("open");
        let result = session
            .run_query("SELECT customer, total FROM orders WHERE paid = 1 ORDER BY id", 100)
            .expect("query");
        let names: Vec<&str> = result.columns().iter().map(|column| column.name.as_str()).collect();
        assert_eq!(names, ["customer", "total"]);
        assert_eq!(result.total(), 3);
        assert_eq!(result.row(0)[0], Cell::Text("Ada".into()));
        let cancel = CancellationToken::new();
        assert_eq!(result.find("linus", 5, &cancel).expect("find").matches, vec![1]);
        let capped = session.run_query("SELECT id FROM orders", 2).expect("capped");
        assert_eq!((capped.total(), capped.truncated), (2, true));
        assert!(session.run_query("DELETE FROM orders", 100).is_err());
        assert!(session.run_query("SELECT nope FROM orders", 100).is_err());
    }

    #[test]
    fn the_row_cap_truncates_and_says_so() {
        let table = LiveTable::load(
            &fixture(),
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            4,
        )
        .expect("load");
        assert_eq!(table.total(), 4);
        assert!(table.truncated);
        assert_eq!(
            table.origin(),
            "sqlite · orders · read-only · page order may change; rows may be skipped or repeated · 4 rows loaded; result may be incomplete"
        );
        let whole = LiveTable::load(
            &fixture(),
            &TableInfo {
                schema: None,
                name: "orders".into(),
            },
            100,
        )
        .expect("load");
        assert_eq!(
            whole.origin(),
            "sqlite · orders · read-only · page order may change; rows may be skipped or repeated"
        );
        assert!(
            whole
                .origin()
                .contains("page order may change; rows may be skipped or repeated")
        );
    }

    #[test]
    fn a_missing_table_or_file_is_an_error_not_a_panic() {
        assert!(
            LiveTable::load(
                &fixture(),
                &TableInfo {
                    schema: None,
                    name: "nope".into()
                },
                10
            )
            .is_err()
        );
        let missing = Origin::File("/nonexistent/none.sqlite".into());
        assert!(
            LiveTable::load(
                &missing,
                &TableInfo {
                    schema: None,
                    name: "orders".into()
                },
                10
            )
            .is_err()
        );
    }
