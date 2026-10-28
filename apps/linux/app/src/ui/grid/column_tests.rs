use super::super::display::{POSITION_SLOT, value_is_inline_editable};
use super::*;
use tablepro_core::{ColumnInfo, Value};

fn col(data_type: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: "x".into(),
        data_type: data_type.into(),
        nullable: true,
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
fn editable_for_normal_column() {
    assert!(is_cell_editable(&col("text", false)));
    assert!(is_cell_editable(&col("integer", false)));
}

#[test]
fn not_editable_for_primary_key() {
    assert!(!is_cell_editable(&col("integer", true)));
}

#[test]
fn not_editable_for_generated_column() {
    let mut c = col("integer", false);
    c.is_generated = true;
    assert!(!is_cell_editable(&c));
}

#[test]
fn not_editable_for_auto_increment_non_pk() {
    let mut c = col("integer", false);
    c.is_auto_increment = true;
    assert!(!is_cell_editable(&c));
}

#[test]
fn an_edited_preview_keeps_the_full_value_and_hides_the_preview() {
    let full = "x".repeat(9_000);
    let row = crate::ui::row_object::RowObject::new(vec![Value::Int(1), Value::Text(full.clone())]);
    row.preview_long_values(&[0]);
    assert!(preview_for_bind(&row, 1, false).is_some());
    assert!(preview_for_bind(&row, 1, true).is_none());
    assert_eq!(row.cell_value(1), Value::Text(full));
}

#[test]
fn preview_binding_does_not_retain_a_second_full_value() {
    let row = crate::ui::row_object::RowObject::new(vec![Value::Int(1), Value::Text("x".repeat(9_000))]);
    row.preview_long_values(&[0]);
    let preview = row.cell_preview(1).unwrap();
    assert!(value_for_bind(&row, 1, Some(&preview), None).is_none());

    let edited = Value::Text("pending".repeat(1_200));
    assert_eq!(value_for_bind(&row, 1, None, Some(edited.clone())), Some(edited));
}

#[test]
fn header_title_shows_type_and_key_markers() {
    let mut info = col("INTEGER", true);
    info.name = "id".into();
    assert_eq!(column_header_title(&info, false), "id \u{1F511}\nINTEGER");
    assert_eq!(column_header_title(&info, true), "id \u{1F511} \u{1F517}\nINTEGER");
}

#[test]
fn a_result_column_takes_the_primary_key_flag_from_the_table_metadata() {
    let mut result = col("int4", false);
    result.name = "id".into();
    let mut schema = col("int4", true);
    schema.name = "id".into();
    assert!(header_column(&result, Some(&schema)).primary_key);
    assert!(!header_column(&result, None).primary_key);
    schema.name = "other".into();
    assert!(!header_column(&result, Some(&schema)).primary_key);
}

#[test]
fn comments_are_in_column_details_without_growing_every_header() {
    let mut info = col("INTEGER", false);
    info.name = "id".into();
    info.comment = Some("Customer id\nsecond line".into());
    assert_eq!(column_header_title(&info, false), "id\nINTEGER");
    let menu = header_details(&info, false);
    let details = menu.item_link(0, "section").unwrap();
    let comment = details
        .item_attribute_value(3, "label", None)
        .unwrap()
        .get::<String>()
        .unwrap();
    assert_eq!(comment, "Customer id\nsecond line");
    info.comment = Some("x".repeat(2000));
    let menu = header_details(&info, false);
    let details = menu.item_link(0, "section").unwrap();
    let comment = details
        .item_attribute_value(3, "label", None)
        .unwrap()
        .get::<String>()
        .unwrap();
    assert_eq!(comment.chars().count(), 1024);
}

#[test]
fn a_result_column_takes_the_comment_from_the_table_metadata() {
    let mut result = col("int4", false);
    result.name = "id".into();
    let mut schema = col("int4", false);
    schema.name = "id".into();
    schema.comment = Some("Customer id".into());
    assert_eq!(
        header_column(&result, Some(&schema)).comment.as_deref(),
        Some("Customer id")
    );
    assert_eq!(header_column(&result, None).comment, None);
}

#[test]
fn header_title_omits_an_unknown_type() {
    let mut info = col("", false);
    info.name = "name".into();
    assert_eq!(column_header_title(&info, false), "name");
}

#[test]
fn pending_cell_description_names_each_edit_state() {
    assert_eq!(
        pending_description(&["tp-cell-modified"]),
        Some(crate::tr!("Modified").to_string())
    );
    assert_eq!(
        pending_description(&["tp-row-pending-insert"]),
        Some(crate::tr!("New row").to_string())
    );
    assert_eq!(
        pending_description(&["tp-row-pending-delete"]),
        Some(crate::tr!("Pending deletion").to_string())
    );
    assert_eq!(pending_description(&[]), None);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn recycled_cell_clears_pending_visual_state_and_resets_accessible_description() {
    gtk4::init().unwrap();
    let label = gtk4::Label::new(None);
    label.add_css_class("tp-cell-modified");
    apply_pending_description(label.upcast_ref(), &["tp-cell-modified"]);
    assert_eq!(pending_description(&[]), None);

    clear_pending_classes(label.upcast_ref());
    apply_pending_description(label.upcast_ref(), &[]);

    assert!(!label.has_css_class("tp-cell-modified"));
}

#[test]
fn not_editable_for_bytes() {
    assert!(!is_cell_editable(&col("bytea", false)));
    assert!(!is_cell_editable(&col("blob", false)));
    assert!(!is_cell_editable(&col("longblob", false)));
    assert!(!is_cell_editable(&col("BINARY", false)));
    assert!(!is_cell_editable(&col("varbinary", false)));
}

#[test]
fn text_column_stays_column_editable_when_a_cell_holds_bytes() {
    assert!(is_cell_editable(&col("text", false)));
    assert!(!value_is_inline_editable(&Value::Bytes(vec![0xFF, 0xFE])));
}

fn collect_cell_editors(widget: &gtk4::Widget) -> Vec<crate::ui::cell_editor::CellEditor> {
    use gtk4::prelude::*;
    let mut found = Vec::new();
    if let Ok(editor) = widget.clone().downcast::<crate::ui::cell_editor::CellEditor>() {
        found.push(editor);
        return found;
    }
    let mut child = widget.first_child();
    while let Some(node) = child {
        found.extend(collect_cell_editors(&node));
        child = node.next_sibling();
    }
    found
}

fn wait_for_editors(view: &gtk4::Widget, n: usize) -> Vec<crate::ui::cell_editor::CellEditor> {
    let ctx = gtk4::glib::MainContext::default();
    for _ in 0..200 {
        let mut pumped = 0;
        while ctx.iteration(false) && pumped < 64 {
            pumped += 1;
        }
        let found = collect_cell_editors(view);
        if found.len() >= n {
            return found;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    collect_cell_editors(view)
}

fn editor_at(editors: &[crate::ui::cell_editor::CellEditor], position: u32) -> &crate::ui::cell_editor::CellEditor {
    editors
        .iter()
        .find(|editor| POSITION_SLOT.get(*editor) == Some(position))
        .unwrap_or_else(|| panic!("no cell editor bound at position {position}"))
}

fn capture_bind_visual(
    editors: &[crate::ui::cell_editor::CellEditor],
    artifact_dir: &std::path::Path,
) -> std::path::PathBuf {
    let log = artifact_dir.join("text-column-binary-bind.txt");
    let mut lines = String::from("position\ttext\tinline_editable\tediting\n");
    for position in 0..3 {
        let editor = editor_at(editors, position);
        lines.push_str(&format!(
            "{position}\t{}\t{}\t{}\n",
            editor.text(),
            editor.is_inline_editable(),
            editor.is_editing()
        ));
    }
    std::fs::write(&log, lines).expect("write bind visual log");
    let png = artifact_dir.join("text-column-binary-bind.png");
    let _ = std::process::Command::new("scrot").arg(&png).status();
    png
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn boolean_affinity_mismatch_cannot_emit_an_edit() {
    use futures::FutureExt;
    fn checkbox(widget: &gtk4::Widget) -> Option<gtk4::CheckButton> {
        if let Ok(button) = widget.clone().downcast::<gtk4::CheckButton>() {
            return Some(button);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(button) = checkbox(&current) {
                return Some(button);
            }
            child = current.next_sibling();
        }
        None
    }
    gtk4::init().unwrap();
    let columns = vec![col("BOOLEAN", false)];
    let result = tablepro_core::QueryResult {
        columns: columns.clone(),
        rows: vec![vec![Value::Bool(false)]],
        truncated: false,
    };
    let (sender, receiver) = relm4::channel::<GridMsg>();
    let (view, selection) = crate::ui::grid::build_column_view(
        &std::sync::Arc::new(result),
        &columns,
        "flags",
        Some(sender),
        None,
        None,
        None,
        None,
        TabGridContext::default(),
        None,
        std::sync::Arc::new(crate::services::database_service::DatabaseService::new()),
    );
    let window = gtk4::Window::builder().child(&view).build();
    window.present();
    let store = selection
        .model()
        .unwrap()
        .downcast::<crate::ui::row_store::RowStore>()
        .unwrap();
    for (value, allowed) in [
        (Value::Bytes(vec![1]), false),
        (Value::Text("true".into()), false),
        (Value::Null, true),
        (Value::Bool(false), true),
        (Value::Bytes(vec![]), false),
    ] {
        store.splice(0, 1, &[crate::ui::row_object::RowObject::new(vec![value])]);
        let context = gtk4::glib::MainContext::default();
        for _ in 0..50 {
            for _ in 0..64 {
                if !context.iteration(false) {
                    break;
                }
            }
            if checkbox(view.upcast_ref()).is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let button = checkbox(view.upcast_ref()).expect("bound checkbox");
        assert_eq!(button.is_sensitive(), allowed);
        button.set_active(!button.is_active());
        let event = receiver.recv().now_or_never().flatten();
        assert_eq!(
            matches!(event, Some(GridMsg::CellEdited { .. })),
            allowed,
            "blocked values must emit no pending edit"
        );
    }
    window.close();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn mysql_bit_one_grid_shows_numeric_values_and_emits_boolean_edits() {
    use futures::FutureExt;
    fn checkbox(widget: &gtk4::Widget) -> Option<gtk4::CheckButton> {
        if let Ok(button) = widget.clone().downcast::<gtk4::CheckButton>() {
            return Some(button);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(button) = checkbox(&current) {
                return Some(button);
            }
            child = current.next_sibling();
        }
        None
    }

    gtk4::init().unwrap();
    let columns = vec![col("bit(1)", false)];
    let result = tablepro_core::QueryResult {
        columns: columns.clone(),
        rows: vec![vec![Value::Int(0)]],
        truncated: false,
    };
    let (sender, receiver) = relm4::channel::<GridMsg>();
    let (view, selection) = crate::ui::grid::build_column_view(
        &std::sync::Arc::new(result),
        &columns,
        "flags",
        Some(sender),
        None,
        None,
        None,
        None,
        TabGridContext::default(),
        None,
        std::sync::Arc::new(crate::services::database_service::DatabaseService::new()),
    );
    let window = gtk4::Window::builder().child(&view).build();
    window.present();
    let store = selection
        .model()
        .unwrap()
        .downcast::<crate::ui::row_store::RowStore>()
        .unwrap();
    let context = gtk4::glib::MainContext::default();
    for (value, active, edit) in [(Value::Int(0), false, "true"), (Value::Int(1), true, "false")] {
        store.splice(0, 1, &[crate::ui::row_object::RowObject::new(vec![value])]);
        for _ in 0..50 {
            for _ in 0..64 {
                if !context.iteration(false) {
                    break;
                }
            }
            if checkbox(view.upcast_ref()).is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let button = checkbox(view.upcast_ref()).expect("bound BIT(1) checkbox");
        assert!(button.is_sensitive());
        assert!(!button.is_inconsistent());
        assert_eq!(button.is_active(), active);
        button.set_active(!active);
        let event = receiver.recv().now_or_never().flatten();
        assert!(matches!(event, Some(GridMsg::CellEdited { new_value, .. }) if new_value == edit));
    }
    window.close();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn bind_does_not_open_binary_bytes_in_a_text_column_for_editing() {
    use gtk4::prelude::*;
    gtk4::init().unwrap();
    let columns = vec![col("TEXT", false)];
    let result = tablepro_core::QueryResult {
        columns: columns.clone(),
        rows: vec![
            vec![Value::Text("hello".into())],
            vec![Value::Bytes(vec![0xFF, 0xFE, 0x00, 0x01])],
            vec![Value::Bytes(b"utf8 blob".to_vec())],
        ],
        truncated: false,
    };
    let (sender, _receiver) = relm4::channel::<GridMsg>();
    let (view, selection) = crate::ui::grid::build_column_view(
        &std::sync::Arc::new(result),
        &columns,
        "notes",
        Some(sender),
        None,
        None,
        None,
        None,
        TabGridContext::default(),
        None,
        std::sync::Arc::new(crate::services::database_service::DatabaseService::new()),
    );
    let window = gtk4::Window::builder()
        .title("TEXT cells with binary values")
        .default_width(520)
        .default_height(220)
        .child(&view)
        .build();
    window.present();
    let editors = wait_for_editors(view.upcast_ref(), 3);
    assert_eq!(editors.len(), 3, "column factory should bind a cell editor per row");

    let text_cell = editor_at(&editors, 0);
    assert_eq!(text_cell.text().as_str(), "hello");
    assert!(text_cell.is_inline_editable());
    text_cell.start_editing();
    assert!(text_cell.is_editing());
    text_cell.stop_editing(false);
    assert!(!text_cell.is_editing());

    let binary_cell = editor_at(&editors, 1);
    assert_eq!(binary_cell.text().as_str(), "<4 bytes>");
    assert!(!binary_cell.is_inline_editable());
    binary_cell.start_editing();
    assert!(!binary_cell.is_editing());
    assert_eq!(binary_cell.text().as_str(), "<4 bytes>");

    let utf8_bytes_cell = editor_at(&editors, 2);
    assert_eq!(utf8_bytes_cell.text().as_str(), "utf8 blob");
    assert!(!utf8_bytes_cell.is_inline_editable());
    utf8_bytes_cell.start_editing();
    assert!(!utf8_bytes_cell.is_editing());

    let artifact_dir = std::env::var_os("TABLEPRO_GTK_ARTIFACT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let _ = std::fs::create_dir_all(&artifact_dir);
    let png = capture_bind_visual(&editors, &artifact_dir);
    if std::env::var_os("TABLEPRO_GTK_REQUIRE_SCREENSHOT").is_some() {
        assert!(
            png.is_file(),
            "scrot should capture the bound TEXT/binary grid to {png:?}"
        );
    }

    let store = selection
        .model()
        .expect("selection model")
        .downcast::<crate::ui::row_store::RowStore>()
        .expect("row store");
    store.remove(0);
    store.insert(
        0,
        &crate::ui::row_object::RowObject::new(vec![Value::Bytes(vec![0xAA, 0xBB, 0xCC])]),
    );
    let rebound = wait_for_editors(view.upcast_ref(), 3);
    let recycled = editor_at(&rebound, 0);
    assert_eq!(recycled.text().as_str(), "<3 bytes>");
    assert!(!recycled.is_inline_editable());
    recycled.start_editing();
    assert!(!recycled.is_editing());

    store.remove(0);
    store.insert(
        0,
        &crate::ui::row_object::RowObject::new(vec![Value::Text("hello".into())]),
    );
    let restored = wait_for_editors(view.upcast_ref(), 3);
    let restored_cell = editor_at(&restored, 0);
    assert_eq!(restored_cell.text().as_str(), "hello");
    assert!(restored_cell.is_inline_editable());
    restored_cell.start_editing();
    assert!(restored_cell.is_editing());
    restored_cell.stop_editing(false);

    window.close();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn mysql_spatial_grid_bytes_bind_as_read_only_labels_without_edits() {
    use futures::FutureExt;
    use gtk4::prelude::*;
    fn spatial_labels(widget: &gtk4::Widget) -> Vec<gtk4::Label> {
        let mut found = Vec::new();
        if let Ok(label) = widget.clone().downcast::<gtk4::Label>() {
            if label.text() == "<9 bytes>" {
                found.push(label);
            }
            return found;
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            found.extend(spatial_labels(&current));
            child = current.next_sibling();
        }
        found
    }
    gtk4::init().unwrap();

    let columns = ["geometry", "point", "multipolygon"]
        .into_iter()
        .map(|data_type| col(data_type, false))
        .collect::<Vec<_>>();
    let values = [
        Value::Bytes(vec![0, 0, 0, 1, 1, 1, 0, 0, 0]),
        Value::Bytes(vec![0, 0, 0, 1, 1, 1, 0, 0, 0]),
        Value::Bytes(vec![0, 0, 0, 1, 1, 1, 0, 0, 0]),
    ];
    let result = tablepro_core::QueryResult {
        columns: columns.clone(),
        rows: vec![values.to_vec()],
        truncated: false,
    };
    let (sender, receiver) = relm4::channel::<GridMsg>();
    let (view, _) = crate::ui::grid::build_column_view(
        &std::sync::Arc::new(result),
        &columns,
        "spatial_values",
        Some(sender),
        None,
        None,
        None,
        None,
        TabGridContext::default(),
        None,
        std::sync::Arc::new(crate::services::database_service::DatabaseService::new()),
    );
    let window = gtk4::Window::builder().child(&view).build();
    window.present();
    let context = gtk4::glib::MainContext::default();
    let mut labels = Vec::new();
    for _ in 0..200 {
        while context.iteration(false) {}
        labels = spatial_labels(view.upcast_ref());
        if labels.len() >= 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(labels.len(), 3, "all spatial values should bind to read-only labels");
    assert!(
        collect_cell_editors(view.upcast_ref()).is_empty(),
        "spatial bytes must not be given editable CellEditor widgets"
    );
    assert!(
        receiver.recv().now_or_never().flatten().is_none(),
        "read-only spatial values must not emit a pending edit"
    );
    window.close();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn entering_edit_on_a_truncated_long_text_cell_seeds_the_full_value() {
    use gtk4::prelude::*;
    gtk4::init().unwrap();
    let columns = vec![col("TEXT", false)];
    let long = "a".repeat(50_000);
    let result = tablepro_core::QueryResult {
        columns: columns.clone(),
        rows: vec![vec![Value::Text(long.clone())]],
        truncated: false,
    };
    let (sender, _receiver) = relm4::channel::<GridMsg>();
    let (view, _selection) = crate::ui::grid::build_column_view(
        &std::sync::Arc::new(result),
        &columns,
        "notes",
        Some(sender),
        None,
        None,
        None,
        None,
        TabGridContext::default(),
        None,
        std::sync::Arc::new(crate::services::database_service::DatabaseService::new()),
    );
    let window = gtk4::Window::builder().child(&view).build();
    window.present();
    let editors = wait_for_editors(view.upcast_ref(), 1);
    let cell = editor_at(&editors, 0);
    assert!(cell.text().contains("more bytes"), "display text should stay truncated");

    super::super::editing::enter_edit_mode(cell);
    assert!(cell.is_editing());
    assert_eq!(
        cell.entry().text().as_str(),
        long,
        "editing must seed the untruncated value"
    );
    cell.stop_editing(false);

    window.close();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn editing_literal_null_sentinel_text_keeps_it_distinct_from_sql_null() {
    use gtk4::prelude::*;
    gtk4::init().unwrap();
    let columns = vec![col("TEXT", false), col("TEXT", false)];
    let result = tablepro_core::QueryResult {
        columns: columns.clone(),
        rows: vec![vec![Value::Text("<NULL>".into()), Value::Null]],
        truncated: false,
    };
    let (sender, _receiver) = relm4::channel::<GridMsg>();
    let (view, _) = crate::ui::grid::build_column_view(
        &std::sync::Arc::new(result),
        &columns,
        "null_sentinel_text",
        Some(sender),
        None,
        None,
        None,
        None,
        TabGridContext::default(),
        None,
        std::sync::Arc::new(crate::services::database_service::DatabaseService::new()),
    );
    let window = gtk4::Window::builder().child(&view).build();
    window.present();
    let editors = wait_for_editors(view.upcast_ref(), 2);
    for (cell, expected) in editors.iter().zip(["<NULL>", ""]) {
        super::super::editing::enter_edit_mode(cell);
        assert_eq!(cell.entry().text().as_str(), expected);
        cell.stop_editing(false);
    }
    window.close();
}
