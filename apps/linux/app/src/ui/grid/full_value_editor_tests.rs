use super::*;
use futures::FutureExt;

#[test]
fn text_payload_preserves_empty_and_multiline_values_exactly() {
    assert_eq!(
        full_value_payload(FullValueKind::Text, ""),
        Ok(Value::Text(String::new()))
    );
    assert_eq!(
        full_value_payload(FullValueKind::Text, "first\r\nsecond  \n"),
        Ok(Value::Text("first\r\nsecond  \n".into()))
    );
}

#[test]
fn text_payload_refuses_values_above_the_edit_budget() {
    assert_eq!(
        full_value_payload(FullValueKind::Text, &"x".repeat(MAX_EDIT_BYTES + 1)),
        Err(PayloadError::TooLarge)
    );
    assert!(full_value_payload(FullValueKind::Text, &"x".repeat(MAX_EDIT_BYTES)).is_ok());
}

#[test]
fn text_column_detection_excludes_json_and_typed_values() {
    for data_type in ["text", "varchar(100)", "character varying(255)", "ntext"] {
        assert!(is_text_column(data_type));
    }
    for data_type in ["jsonb", "uuid", "decimal(20, 4)", "integer", "uuid[]"] {
        assert!(!is_text_column(data_type));
    }
}

#[test]
fn text_editor_staging_enforces_the_size_budget() {
    let column = ColumnInfo {
        name: "notes".into(),
        data_type: "varchar(40)".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        is_generated: false,
        comment: None,
        default_value: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    };
    assert!(is_text_column(&column.data_type));
    assert!(can_stage_full_value(
        &column,
        &Value::Text("before".into()),
        &Value::Text("x".repeat(MAX_EDIT_BYTES))
    ));
    assert!(!can_stage_full_value(
        &column,
        &Value::Text("before".into()),
        &Value::Text("x".repeat(MAX_EDIT_BYTES + 1))
    ));
    assert!(!is_text_column("jsonb"));
}

#[test]
fn full_value_editor_accepts_exact_json_text_and_json_carriers() {
    let column = ColumnInfo {
        name: "payload".into(),
        data_type: "json".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        is_generated: false,
        comment: None,
        default_value: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    };
    let original = Value::JsonText(r#"{ "b": 1, "a": 2, "b": 3 }"#.into());
    let edited = Value::JsonText(r#"{ "b": 4, "a": 5, "b": 6 }"#.into());
    assert!(can_stage_full_value(&column, &original, &edited));
    assert!(can_stage_full_value(
        &column,
        &Value::Json(Box::new(serde_json::json!({"before": 1}))),
        &Value::Json(Box::new(serde_json::json!({"after": 2})))
    ));
    assert!(can_stage_full_value(
        &column,
        &Value::Text(r#"{"before":1}"#.into()),
        &Value::Text(r#"{"after":2}"#.into())
    ));
    assert!(!can_stage_full_value(
        &column,
        &original,
        &Value::JsonText("{invalid".into())
    ));
    assert!(!can_stage_full_value(
        &column,
        &Value::Null,
        &Value::JsonText("x".repeat(MAX_EDIT_BYTES + 1))
    ));
}

#[test]
fn json_column_and_value_kind_detection_preserves_carrier_identity() {
    assert!(is_full_value_column("JSON"));
    assert!(is_full_value_column("Nullable(JSONB)"));
    assert!(is_json_column("Nullable(JSONB)"));
    assert_eq!(
        edit_kind("json", &Value::JsonText("{}".into())),
        Some(FullValueKind::JsonText)
    );
    assert_eq!(
        edit_kind("jsonb", &Value::Json(Box::new(serde_json::json!({})))),
        Some(FullValueKind::Json)
    );
    assert_eq!(edit_kind("json", &Value::Null), Some(FullValueKind::JsonText));
    assert_eq!(edit_kind("jsonb", &Value::Null), Some(FullValueKind::Json));
    assert_eq!(
        edit_kind("json", &Value::Text("{}".into())),
        Some(FullValueKind::JsonFallbackText)
    );
    assert!(!FullValueKind::JsonText.supports_format());
    assert!(FullValueKind::Json.supports_format());
}

#[test]
fn json_payload_keeps_source_text_and_semantic_json_distinct() {
    let source = "{ \"n\": 900719925474099312345, \"k\": 1, \"k\": 2 }";
    assert_eq!(
        full_value_payload(FullValueKind::JsonText, source),
        Ok(Value::JsonText(source.into()))
    );
    assert!(matches!(
        full_value_payload(FullValueKind::Json, source),
        Ok(Value::Json(_))
    ));
    assert_eq!(
        full_value_payload(FullValueKind::JsonFallbackText, "null"),
        Ok(Value::Text("null".into()))
    );
    assert!(matches!(
        full_value_payload(FullValueKind::Json, "null"),
        Ok(Value::Json(value)) if value.is_null()
    ));
    assert!(matches!(
        full_value_payload(FullValueKind::JsonText, "null"),
        Ok(Value::JsonText(value)) if value == "null"
    ));
    assert!(matches!(
        full_value_payload(FullValueKind::JsonText, "{invalid"),
        Err(PayloadError::InvalidJson { .. })
    ));
}

#[test]
fn formatting_json_is_explicit_and_preserves_large_number_lexemes() {
    let formatted = format_json_text(r#"{"value":900719925474099312345,"items":[1,2]}"#).unwrap();
    assert_eq!(
        formatted,
        "{\n  \"value\": 900719925474099312345,\n  \"items\": [\n    1,\n    2\n  ]\n}"
    );
}

#[test]
fn staging_rejects_noneditable_cells_and_nontext_values() {
    let column = ColumnInfo {
        name: "notes".into(),
        data_type: "text".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        is_generated: false,
        comment: None,
        default_value: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    };
    assert!(can_stage_full_value(&column, &Value::Null, &Value::Text("".into())));
    assert!(!can_stage_full_value(
        &column,
        &Value::Int(1),
        &Value::Text("new".into())
    ));
    assert!(!can_stage_full_value(
        &column,
        &Value::Text("old".into()),
        &Value::Int(2)
    ));
    let mut key_column = column;
    key_column.primary_key = true;
    assert!(!can_stage_full_value(
        &key_column,
        &Value::Text("old".into()),
        &Value::Text("new".into())
    ));
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn applying_empty_text_stages_empty_text_without_converting_it_to_null() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let parent = adw::Window::new();
    parent.present();
    let (sender, receiver) = relm4::channel::<super::super::GridMsg>();
    let dialog = present(
        &parent,
        "notes",
        "before",
        EditContext {
            row_position: 4,
            col_index: 2,
            row_key: vec![Value::Int(7)],
            nullable: true,
            kind: FullValueKind::Text,
            sender,
        },
    );
    text_view(dialog.child().unwrap().upcast_ref()).buffer().set_text("");
    button(dialog.child().unwrap().upcast_ref(), &crate::tr!("Apply")).emit_clicked();
    assert!(matches!(
        receiver.recv().now_or_never().flatten(),
        Some(super::super::GridMsg::CellEditedValue {
            row_position: 4,
            col_index: 2,
            value: Value::Text(text),
            row_key,
        }) if text.is_empty() && row_key == vec![Value::Int(7)]
    ));
    parent.close();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn invalid_json_stages_nothing_and_valid_apply_keeps_json_typed() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let parent = adw::Window::new();
    parent.present();
    let (sender, receiver) = relm4::channel::<super::super::GridMsg>();
    let dialog = present(
        &parent,
        "payload",
        r#"{"before":1}"#,
        EditContext {
            row_position: 2,
            col_index: 1,
            row_key: vec![Value::Int(9)],
            nullable: true,
            kind: FullValueKind::Json,
            sender,
        },
    );
    let root = dialog.child().unwrap().upcast_ref::<gtk::Widget>().clone();
    let buffer = text_view(&root).buffer();
    assert!(button(&root, &crate::tr!("Format JSON")).is_sensitive());
    buffer.set_text("{invalid");
    button(&root, &crate::tr!("Apply")).emit_clicked();
    assert!(receiver.recv().now_or_never().flatten().is_none());
    buffer.set_text(r#"{"after":2}"#);
    button(&root, &crate::tr!("Apply")).emit_clicked();
    assert!(matches!(
        receiver.recv().now_or_never().flatten(),
        Some(super::super::GridMsg::CellEditedValue {
            row_position: 2,
            col_index: 1,
            value: Value::Json(value),
            row_key,
        }) if value["after"] == 2 && row_key == vec![Value::Int(9)]
    ));
    parent.close();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn set_null_stages_null_and_cancel_stages_nothing() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let parent = adw::Window::new();
    parent.present();
    let (sender, receiver) = relm4::channel::<super::super::GridMsg>();
    let dialog = present(
        &parent,
        "notes",
        "before",
        EditContext {
            row_position: 1,
            col_index: 0,
            row_key: vec![Value::Int(3)],
            nullable: true,
            kind: FullValueKind::Text,
            sender,
        },
    );
    button(dialog.child().unwrap().upcast_ref(), &crate::tr!("Set to NULL")).emit_clicked();
    assert!(matches!(
        receiver.recv().now_or_never().flatten(),
        Some(super::super::GridMsg::SetCellNull {
            row_position: 1,
            col_index: 0,
            row_key,
        }) if row_key == vec![Value::Int(3)]
    ));
    let (sender, receiver) = relm4::channel::<super::super::GridMsg>();
    let dialog = present(
        &parent,
        "notes",
        "before",
        EditContext {
            row_position: 1,
            col_index: 0,
            row_key: vec![Value::Int(3)],
            nullable: true,
            kind: FullValueKind::Text,
            sender,
        },
    );
    button(dialog.child().unwrap().upcast_ref(), &crate::tr!("Cancel")).emit_clicked();
    assert!(receiver.recv().now_or_never().flatten().is_none());
    parent.close();
}

fn button(root: &gtk::Widget, label: &str) -> gtk::Button {
    descendants(root)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.label().as_deref() == Some(label))
        .unwrap()
}

fn text_view(root: &gtk::Widget) -> gtk::TextView {
    descendants(root)
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk::TextView>().ok())
        .unwrap()
}

fn descendants(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut pending = vec![root.clone()];
    let mut widgets = Vec::new();
    while let Some(widget) = pending.pop() {
        let mut child = widget.first_child();
        while let Some(next) = child {
            child = next.next_sibling();
            pending.push(next);
        }
        widgets.push(widget);
    }
    widgets
}
