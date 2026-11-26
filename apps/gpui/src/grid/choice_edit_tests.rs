use bookie_core::ColumnInfo;
use bookie_gpui::model::Cell;
use gpui::TestAppContext;

use crate::GridView;
use crate::choice_edit::{Choice, choice_for, is_choice_candidate, member_cell, next_boolean};
use crate::live_ui_tests::{SqliteFixture, sqlite_fixture};

fn column(data_type: &str, nullable: bool) -> ColumnInfo {
    ColumnInfo {
        name: "state".into(),
        data_type: data_type.into(),
        nullable,
        primary_key: false,
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
fn a_boolean_cycles_through_null_only_when_the_column_is_nullable() {
    assert_eq!(next_boolean(&Cell::Null, true), Cell::Bool(true));
    assert_eq!(next_boolean(&Cell::Bool(true), true), Cell::Bool(false));
    assert_eq!(next_boolean(&Cell::Bool(false), true), Cell::Null);
    assert_eq!(next_boolean(&Cell::Bool(true), false), Cell::Bool(false));
    assert_eq!(next_boolean(&Cell::Bool(false), false), Cell::Bool(true));
    assert_eq!(next_boolean(&Cell::Null, false), Cell::Bool(true));
}

#[test]
fn declared_enum_and_set_columns_offer_their_members() {
    let status = column("enum('new','it''s done')", false);
    assert_eq!(
        choice_for("ENUM", Some(&status), &Cell::Text("new".into())),
        Some(Choice::Members(vec!["new".into(), "it's done".into()]))
    );
    let rights = column("set('read','write')", true);
    assert_eq!(
        choice_for("SET", Some(&rights), &Cell::Null),
        Some(Choice::Members(vec!["read".into(), "write".into()]))
    );
    assert_eq!(
        choice_for("text", Some(&column("text", true)), &Cell::Text("a".into())),
        None
    );
    assert!(is_choice_candidate("ENUM", &Cell::Text("new".into())));
    assert!(!is_choice_candidate("text", &Cell::Text("new".into())));
}

#[test]
fn a_schema_qualified_postgres_enum_uses_the_picker_but_an_array_does_not() {
    let mut status = column("app.mood", true);
    let type_name = bookie_core::QualifiedTypeName {
        schema: "app".into(),
        name: "mood".into(),
    };
    status.enum_type = Some(type_name.clone());
    let current = Cell::Text("new".into());
    assert_eq!(
        choice_for("app.mood", Some(&status), &current),
        Some(Choice::PostgresEnum(type_name.clone()))
    );
    assert_eq!(
        choice_for("app.mood", Some(&status), &Cell::Null),
        Some(Choice::PostgresEnum(type_name))
    );
    status.data_type = "app.mood[]".into();
    assert!(choice_for("app.mood[]", Some(&status), &current).is_none());
}

#[test]
fn a_null_enum_named_bool_opens_the_enum_picker() {
    let mut status = column("bool", true);
    let type_name = bookie_core::QualifiedTypeName {
        schema: "app".into(),
        name: "bool".into(),
    };
    status.enum_type = Some(type_name.clone());
    assert_eq!(
        choice_for("bool", Some(&status), &Cell::Null),
        Some(Choice::PostgresEnum(type_name))
    );
    assert_eq!(
        choice_for("bool", Some(&column("boolean", true)), &Cell::Null),
        Some(Choice::Toggle(Cell::Bool(true)))
    );
}

#[test]
fn a_member_is_staged_as_its_declared_text() {
    let labels = vec!["new".into(), "done".into()];
    assert_eq!(member_cell("done", &labels), Some(Cell::Text("done".into())));
    assert_eq!(member_cell("gone", &labels), None);
}

#[test]
fn a_menu_pick_preserves_two_apostrophes_as_a_literal_member() {
    for declaration in [r"enum('','\'\'')", r"set('','\'\'','read','write')"] {
        let status = column(declaration, false);
        let Some(Choice::Members(labels)) = choice_for(declaration, Some(&status), &Cell::Null) else {
            panic!("the declared members are offered");
        };
        assert_eq!(member_cell("", &labels), Some(Cell::Text(String::new())));
        assert_eq!(member_cell("''", &labels), Some(Cell::Text("''".into())));
        assert_eq!(member_cell("read,write", &labels), None);
    }
}

fn connect_flags(cx: &mut TestAppContext) -> (gpui::Entity<GridView>, &mut gpui::VisualTestContext, SqliteFixture) {
    use bookie_gpui::live::{LiveSession, Origin};
    let fixture = sqlite_fixture(&[
        "CREATE TABLE flags (id INTEGER PRIMARY KEY, done BOOLEAN NOT NULL, maybe BOOLEAN)",
        "INSERT INTO flags VALUES (1, 1, NULL)",
    ]);
    let mut session = LiveSession::open(&Origin::File(fixture.0.clone())).expect("open");
    std::sync::Arc::get_mut(&mut session).expect("not shared").writable = Some(uuid::Uuid::new_v4());
    let table = bookie_core::TableInfo {
        schema: None,
        name: "flags".into(),
    };
    let table = std::sync::Arc::new(
        session
            .load_table_controlled(&table, 100, &tokio_util::sync::CancellationToken::new())
            .expect("load"),
    );
    cx.update(crate::text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = GridView::with_source(table.clone(), std::time::Instant::now(), window, cx);
        view.attach_live(table.clone());
        view
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    (view, cx, fixture)
}

fn toggle(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, column: usize) -> Cell {
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, column, window, cx)));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.active.is_none(), "{}", view.status);
        view.cell_at(0, column)
    })
}

#[gpui::test]
fn editing_a_boolean_cell_stages_the_next_value(cx: &mut TestAppContext) {
    let (view, cx, _fixture) = connect_flags(cx);
    view.read_with(cx, |view, _| {
        assert!(view.live.is_some(), "{}", view.status);
        assert_eq!(view.cell_at(0, 1), Cell::Bool(true), "{:?}", view.source.columns());
    });
    assert_eq!(toggle(&view, cx, 1), Cell::Bool(false));
    assert_eq!(toggle(&view, cx, 1), Cell::Bool(true));
    assert_eq!(toggle(&view, cx, 2), Cell::Bool(true));
    assert_eq!(toggle(&view, cx, 2), Cell::Bool(false));
    assert_eq!(toggle(&view, cx, 2), Cell::Null);
    view.read_with(cx, |view, _| assert!(!view.cell_edit.undo.is_empty()));
}

#[gpui::test]
fn space_on_a_boolean_cell_toggles_it(cx: &mut TestAppContext) {
    let (view, cx, _fixture) = connect_flags(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
            view.grid_key("space", false, false, window, cx);
        });
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_eq!(view.cell_at(0, 1), Cell::Bool(false)));
}

#[gpui::test]
fn a_choice_metadata_reply_cannot_edit_a_replacement_grid(cx: &mut TestAppContext) {
    let (view, cx, _fixture) = connect_flags(cx);
    let replacement = view.read_with(cx, |view, _| {
        view.live
            .as_ref()
            .unwrap()
            .load_table_controlled(
                &view.catalog.grid_table,
                100,
                &tokio_util::sync::CancellationToken::new(),
            )
            .unwrap()
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.live.as_ref().unwrap().forget_columns();
            view.start_edit(0, 1, window, cx);
            view.source = std::sync::Arc::new(replacement);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.edits.is_empty());
        assert_eq!(view.cell_at(0, 1), Cell::Bool(true));
    });
}

#[gpui::test]
fn starting_a_second_cell_edit_discards_the_first_choice_reply(cx: &mut TestAppContext) {
    let (view, cx, _fixture) = connect_flags(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.live.as_ref().unwrap().forget_columns();
            view.start_edit(0, 1, window, cx);
            view.start_edit(0, 2, window, cx);
        });
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_at(0, 1), Cell::Bool(true));
        assert_eq!(view.cell_at(0, 2), Cell::Bool(true));
    });
}
