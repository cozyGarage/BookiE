use gtk4::prelude::*;
use gtk4::{self as gtk};
use relm4::adw;
use relm4::adw::prelude::*;
use serde_json::value::RawValue;
use tablepro_core::{ColumnInfo, Value};

const MAX_EDIT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FullValueKind {
    Text,
    Json,
    JsonText,
    JsonFallbackText,
}

impl FullValueKind {
    fn supports_format(self) -> bool {
        matches!(self, Self::Json)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum PayloadError {
    TooLarge,
    InvalidJson { line: usize, column: usize },
}

pub(crate) fn is_text_column(data_type: &str) -> bool {
    let data_type = data_type.trim().to_ascii_lowercase();
    let base = data_type.split('(').next().unwrap_or_default().trim();
    matches!(
        base,
        "text"
            | "tinytext"
            | "mediumtext"
            | "longtext"
            | "ntext"
            | "citext"
            | "clob"
            | "nclob"
            | "xml"
            | "string"
            | "character varying"
            | "character"
            | "char"
            | "varchar"
            | "nvarchar"
            | "nchar"
    )
}

pub(crate) fn is_json_column(data_type: &str) -> bool {
    ["json", "jsonb"]
        .iter()
        .any(|name| base_type(data_type).eq_ignore_ascii_case(name))
}

pub(crate) fn is_full_value_column(data_type: &str) -> bool {
    is_text_column(data_type) || is_json_column(data_type)
}

fn base_type(data_type: &str) -> &str {
    let normalized = data_type.trim();
    let unwrapped = if normalized.len() >= "nullable(".len()
        && normalized
            .get(.."nullable(".len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("nullable("))
        && normalized.ends_with(')')
    {
        &normalized["nullable(".len()..normalized.len() - 1]
    } else {
        normalized
    };
    unwrapped.split('(').next().unwrap_or_default().trim()
}

pub(super) fn edit_kind(data_type: &str, value: &Value) -> Option<FullValueKind> {
    if is_text_column(data_type) {
        return matches!(value, Value::Text(_) | Value::Null).then_some(FullValueKind::Text);
    }
    if !is_json_column(data_type) {
        return None;
    }
    Some(match value {
        Value::Json(_) => FullValueKind::Json,
        Value::JsonText(_) => FullValueKind::JsonText,
        Value::Text(_) => FullValueKind::JsonFallbackText,
        Value::Null if base_type(data_type).eq_ignore_ascii_case("jsonb") => FullValueKind::Json,
        Value::Null => FullValueKind::JsonText,
        _ => return None,
    })
}

pub(crate) fn can_stage_full_value(column: &ColumnInfo, current: &Value, edited: &Value) -> bool {
    if !super::cell_allows_inline_edit(column, current) {
        return false;
    }
    if is_text_column(&column.data_type) {
        return matches!(current, Value::Text(_) | Value::Null)
            && matches!(edited, Value::Text(text) if text.len() <= MAX_EDIT_BYTES);
    }
    if !is_json_column(&column.data_type) {
        return false;
    }
    json_edit_is_bounded(current, edited)
}

fn json_edit_is_bounded(current: &Value, edited: &Value) -> bool {
    match (current, edited) {
        (Value::JsonText(_), Value::JsonText(text)) | (Value::Text(_), Value::Text(text)) => valid_json_text(text),
        (Value::Json(_), Value::Json(value)) => json_serialized_size(value).is_some_and(|size| size <= MAX_EDIT_BYTES),
        (Value::Null, Value::JsonText(text)) | (Value::Null, Value::Text(text)) => valid_json_text(text),
        (Value::Null, Value::Json(value)) => json_serialized_size(value).is_some_and(|size| size <= MAX_EDIT_BYTES),
        _ => false,
    }
}

fn valid_json_text(text: &str) -> bool {
    text.len() <= MAX_EDIT_BYTES && serde_json::from_str::<Box<RawValue>>(text).is_ok()
}

fn json_serialized_size(value: &serde_json::Value) -> Option<usize> {
    let mut counter = ByteCounter(0);
    serde_json::to_writer(&mut counter, value).ok()?;
    Some(counter.0)
}

struct ByteCounter(usize);

impl std::io::Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("serialized JSON size overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn full_value_payload(kind: FullValueKind, text: &str) -> Result<Value, PayloadError> {
    if text.len() > MAX_EDIT_BYTES {
        return Err(PayloadError::TooLarge);
    }
    match kind {
        FullValueKind::Text => Ok(Value::Text(text.to_owned())),
        FullValueKind::Json => serde_json::from_str(text).map(Value::Json).map_err(invalid_json_error),
        FullValueKind::JsonText => {
            serde_json::from_str::<Box<RawValue>>(text).map_err(invalid_json_error)?;
            Ok(Value::JsonText(text.to_owned()))
        }
        FullValueKind::JsonFallbackText => {
            serde_json::from_str::<Box<RawValue>>(text).map_err(invalid_json_error)?;
            Ok(Value::Text(text.to_owned()))
        }
    }
}

fn format_json_text(text: &str) -> Result<String, serde_json::Error> {
    serde_json::from_str::<serde_json::Value>(text).and_then(|value| serde_json::to_string_pretty(&value))
}

fn invalid_json_error(error: serde_json::Error) -> PayloadError {
    PayloadError::InvalidJson {
        line: error.line(),
        column: error.column(),
    }
}

pub(super) fn within_edit_budget(size: usize) -> bool {
    size <= MAX_EDIT_BYTES
}

pub(super) struct EditContext {
    pub(super) row_position: u32,
    pub(super) col_index: usize,
    pub(super) row_key: Vec<Value>,
    pub(super) nullable: bool,
    pub(super) kind: FullValueKind,
    pub(super) sender: relm4::Sender<super::GridMsg>,
}

struct EditorActions {
    container: gtk::Box,
    apply: gtk::Button,
    cancel: gtk::Button,
    set_null: Option<gtk::Button>,
    format: Option<gtk::Button>,
}

pub(super) fn present(
    parent: &impl IsA<gtk::Widget>,
    column_name: &str,
    initial_text: &str,
    context: EditContext,
) -> adw::Dialog {
    let view = create_text_view(initial_text);
    let actions = create_actions(context.nullable, context.kind.supports_format());
    let (content, error) = create_content(&view, &actions.container);
    let dialog = create_dialog(column_name, &content);
    connect_actions(&dialog, &view.buffer(), &error, actions, &context);
    crate::ui::leak_probe::present_tracked("full_value_editor", &dialog, parent);
    dialog
}

fn create_text_view(initial_text: &str) -> gtk::TextView {
    let view = gtk::TextView::builder()
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(12)
        .right_margin(12)
        .build();
    view.buffer().set_text(initial_text);
    view
}

fn create_actions(nullable: bool, is_json: bool) -> EditorActions {
    let apply = gtk::Button::with_label(&crate::tr!("Apply"));
    apply.add_css_class("suggested-action");
    let cancel = gtk::Button::with_label(&crate::tr!("Cancel"));
    let format = is_json.then(|| gtk::Button::with_label(&crate::tr!("Format JSON")));
    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    let set_null = nullable.then(|| gtk::Button::with_label(&crate::tr!("Set to NULL")));
    if let Some(button) = &set_null {
        container.append(button);
    }
    if let Some(button) = &format {
        container.append(button);
    }
    container.append(&cancel);
    container.append(&apply);
    EditorActions {
        container,
        apply,
        cancel,
        set_null,
        format,
    }
}

fn create_content(view: &gtk::TextView, actions: &gtk::Box) -> (gtk::Box, gtk::Label) {
    let scroller = gtk::ScrolledWindow::builder()
        .child(view)
        .min_content_width(520)
        .min_content_height(320)
        .vexpand(true)
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&scroller);
    let error = gtk::Label::builder().xalign(0.0).visible(false).build();
    error.add_css_class("error");
    content.append(&error);
    content.append(actions);
    (content, error)
}

fn create_dialog(column_name: &str, content: &gtk::Box) -> adw::Dialog {
    let dialog = adw::Dialog::builder()
        .title(column_name)
        .content_width(720)
        .content_height(520)
        .child(content)
        .build();
    dialog
        .upcast_ref::<gtk::Widget>()
        .update_property(&[gtk::accessible::Property::Label(column_name)]);
    dialog
}

fn connect_actions(
    dialog: &adw::Dialog,
    buffer: &gtk::TextBuffer,
    error: &gtk::Label,
    actions: EditorActions,
    context: &EditContext,
) {
    let error_for_change = error.clone();
    buffer.connect_changed(move |_| error_for_change.set_visible(false));
    connect_cancel(&actions.cancel, dialog);
    connect_set_null(actions.set_null, dialog, context);
    connect_format(actions.format, buffer, error);
    connect_apply(&actions.apply, dialog, buffer, error, context);
}

fn connect_cancel(button: &gtk::Button, dialog: &adw::Dialog) {
    let dialog = dialog.downgrade();
    button.connect_clicked(move |_| {
        if let Some(dialog) = dialog.upgrade() {
            dialog.close();
        }
    });
}

fn connect_set_null(button: Option<gtk::Button>, dialog: &adw::Dialog, context: &EditContext) {
    let Some(button) = button else { return };
    let sender = context.sender.clone();
    let row_key = context.row_key.clone();
    let row_position = context.row_position;
    let col_index = context.col_index;
    let dialog = dialog.downgrade();
    button.connect_clicked(move |_| {
        sender
            .send(super::GridMsg::SetCellNull {
                row_position,
                col_index,
                row_key: row_key.clone(),
            })
            .ok();
        if let Some(dialog) = dialog.upgrade() {
            dialog.close();
        }
    });
}

fn connect_format(button: Option<gtk::Button>, buffer: &gtk::TextBuffer, error: &gtk::Label) {
    let Some(button) = button else { return };
    let buffer = buffer.clone();
    let error = error.clone();
    button.connect_clicked(move |_| {
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
        match format_json_text(text.as_str()) {
            Ok(formatted) => buffer.set_text(&formatted),
            Err(json_error) => show_json_error(&error, json_error.line(), json_error.column()),
        }
    });
}

fn connect_apply(
    button: &gtk::Button,
    dialog: &adw::Dialog,
    buffer: &gtk::TextBuffer,
    error: &gtk::Label,
    context: &EditContext,
) {
    let dialog = dialog.downgrade();
    let buffer = buffer.clone();
    let error = error.clone();
    let context = EditContext {
        row_position: context.row_position,
        col_index: context.col_index,
        row_key: context.row_key.clone(),
        nullable: context.nullable,
        kind: context.kind,
        sender: context.sender.clone(),
    };
    button.connect_clicked(move |_| {
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
        let value = match full_value_payload(context.kind, text.as_str()) {
            Ok(value) => value,
            Err(payload_error) => {
                show_payload_error(&error, payload_error);
                return;
            }
        };
        context
            .sender
            .send(super::GridMsg::CellEditedValue {
                row_position: context.row_position,
                col_index: context.col_index,
                value,
                row_key: context.row_key.clone(),
            })
            .ok();
        if let Some(dialog) = dialog.upgrade() {
            dialog.close();
        }
    });
}

fn show_payload_error(label: &gtk::Label, error: PayloadError) {
    match error {
        PayloadError::TooLarge => label.set_text(&crate::tr!("Values over 1 MiB cannot be edited in this dialog.")),
        PayloadError::InvalidJson { line, column } => show_json_error(label, line, column),
    }
    label.set_visible(true);
}

fn show_json_error(label: &gtk::Label, line: usize, column: usize) {
    label.set_text(&format!(
        "{} {}, {} {}.",
        crate::tr!("Invalid JSON at line"),
        line,
        crate::tr!("column"),
        column
    ));
    label.set_visible(true);
}

pub(super) fn present_too_large(parent: &impl IsA<gtk::Widget>) {
    let dialog = adw::AlertDialog::new(
        Some(&crate::tr!("Value is too large to edit")),
        Some(&crate::tr!("Values over 1 MiB cannot be edited in this dialog.")),
    );
    dialog.add_response("close", &crate::tr!("Close"));
    dialog.set_close_response("close");
    dialog.present(Some(parent));
}

#[cfg(test)]
#[path = "full_value_editor_tests.rs"]
mod tests;
