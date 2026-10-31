use std::borrow::Cow;
use std::cell::Cell;
use std::io::{self, Write};
use std::marker::PhantomData;
use std::sync::OnceLock;

use gtk4::glib;
use gtk4::prelude::*;

use tablepro_core::{ColumnInfo, Value};

use crate::services::preferences::NullDisplayPref;

const DISPLAY_TEXT_MAX_CHARS: usize = 10_000;
const DISPLAY_TEXT_BYTES_THRESHOLD: usize = DISPLAY_TEXT_MAX_CHARS * 4;
const GRID_CELL_DISPLAY_MAX_CHARS: usize = 512;

thread_local! {
    static NULL_DISPLAY: Cell<NullDisplayPref> = const { Cell::new(NullDisplayPref::Standard) };
}

pub fn set_null_display(style: NullDisplayPref) {
    NULL_DISPLAY.set(style);
}

pub fn editable_null_sentinel() -> String {
    match NULL_DISPLAY.get() {
        NullDisplayPref::Standard => crate::tr!("<NULL>"),
        NullDisplayPref::Symbol => "\u{2205}".to_owned(),
        NullDisplayPref::Parenthesized => crate::tr!("(null)"),
    }
}

pub(crate) fn readonly_null_sentinel() -> String {
    match NULL_DISPLAY.get() {
        NullDisplayPref::Standard => crate::tr!("NULL"),
        NullDisplayPref::Symbol => "\u{2205}".to_owned(),
        NullDisplayPref::Parenthesized => crate::tr!("(null)"),
    }
}

pub(crate) fn auto_filled_sentinel() -> String {
    crate::tr!("(auto)")
}

#[derive(Debug, Clone)]
pub(super) struct CellView<'a> {
    pub(super) display_text: Cow<'a, str>,
    pub(super) truncated: bool,
    pub(super) is_null: bool,
    pub(super) inline_editable: bool,
    pub(super) auto_filled: bool,
    pub(super) checked: Option<bool>,
}

impl<'a> CellView<'a> {
    pub(super) fn text_for_bind(&self, editing_available: bool) -> Cow<'_, str> {
        if self.is_null && !self.auto_filled && editing_available && self.inline_editable {
            Cow::Owned(editable_null_sentinel())
        } else {
            Cow::Borrowed(self.display_text.as_ref())
        }
    }
}

impl CellView<'static> {
    pub(super) fn from_preview(preview: &crate::ui::row_object::CellPreview) -> Self {
        Self {
            display_text: Cow::Owned(preview_display_text(preview, 512)),
            truncated: true,
            is_null: false,
            inline_editable: false,
            auto_filled: false,
            checked: None,
        }
    }
}

pub(super) fn cell_view<'a>(value: &'a Value, column: &ColumnInfo) -> CellView<'a> {
    let is_null = matches!(value, Value::Null);
    let auto_filled = column.is_auto_increment || column.is_generated;
    let (display_text, truncated) = if is_null && auto_filled {
        (Cow::Owned(auto_filled_sentinel()), false)
    } else {
        cell_display_text(value)
    };
    CellView {
        display_text,
        truncated,
        is_null,
        inline_editable: super::presentation::cell_allows_inline_edit(column, value),
        auto_filled,
        checked: match value {
            Value::Bool(checked) => Some(*checked),
            Value::Int(1) if column.data_type.eq_ignore_ascii_case("bit(1)") => Some(true),
            Value::Int(0) if column.data_type.eq_ignore_ascii_case("bit(1)") => Some(false),
            _ => None,
        },
    }
}

fn cell_display_text(value: &Value) -> (Cow<'_, str>, bool) {
    match value {
        Value::Text(text) => text_to_display_text_cow(text, GRID_CELL_DISPLAY_MAX_CHARS, true, None),
        Value::Bytes(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) if !text.contains('\0') => {
                text_to_display_text_cow(text, GRID_CELL_DISPLAY_MAX_CHARS, false, None)
            }
            _ => bounded_owned_text(value_to_display_text(value), GRID_CELL_DISPLAY_MAX_CHARS),
        },
        Value::Json(json) => {
            json_to_display_text_with_limits(json, GRID_CELL_DISPLAY_MAX_CHARS * 4, GRID_CELL_DISPLAY_MAX_CHARS, true)
        }
        Value::JsonText(text) => text_to_display_text_cow(text, GRID_CELL_DISPLAY_MAX_CHARS, false, None),
        _ => bounded_owned_text(value_to_display_text(value), GRID_CELL_DISPLAY_MAX_CHARS),
    }
}

fn bounded_owned_text(mut text: String, max_chars: usize) -> (Cow<'static, str>, bool) {
    let Some((cut, _)) = text.char_indices().nth(max_chars) else {
        return (Cow::Owned(text), false);
    };
    let remaining_bytes = text.len() - cut;
    text.truncate(cut);
    text.push_str(&format!("… (+{remaining_bytes} more bytes)"));
    (Cow::Owned(text), true)
}

fn value_to_text(value: &Value, cap: impl Fn(&str) -> String) -> String {
    match value {
        Value::Null => readonly_null_sentinel(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Text(s) => cap(s),
        // A NUL byte survives UTF-8 decoding (it's a valid codepoint)
        // but is a strong signal the column really is binary, not
        // text that happens to be stored as bytes -- keep the byte
        // count for that case instead of displaying invisible NULs.
        Value::Bytes(b) => match std::str::from_utf8(b) {
            Ok(s) if !s.contains('\0') => cap(s),
            _ => format!("<{} bytes>", b.len()),
        },
        Value::Date(d) => d.format("%Y-%m-%d").to_string(),
        Value::Time(t) => t.format("%H:%M:%S").to_string(),
        Value::DateTime(dt) => dt.format("%Y-%m-%d %H:%M:%S%.f").to_string(),
        Value::TimestampTz(ts) => ts.format("%Y-%m-%d %H:%M:%S%:z").to_string(),
        Value::Decimal(d) => d.to_string(),
        Value::Uuid(u) => u.to_string(),
        Value::Json(j) => cap(&j.to_string()),
        Value::JsonText(j) => cap(j),
        Value::Undecodable(_) if value.is_missing_field() => crate::tr!("(missing)"),
        Value::Undecodable(type_name) => format!("<undecodable {type_name}>"),
    }
}

pub fn value_to_display_text(value: &Value) -> String {
    match value {
        Value::Text(text) => text_to_display_text(text),
        Value::Json(json) => json_to_display_text(json),
        other => value_to_text(other, truncate_for_display),
    }
}

fn text_to_display_text(text: &str) -> String {
    text_to_display_text_cow(text, DISPLAY_TEXT_MAX_CHARS, true, Some(DISPLAY_TEXT_BYTES_THRESHOLD))
        .0
        .into_owned()
}

fn text_to_display_text_cow(
    text: &str,
    max_chars: usize,
    replace_nul: bool,
    byte_threshold: Option<usize>,
) -> (Cow<'_, str>, bool) {
    let skip_truncation = byte_threshold.is_some_and(|threshold| text.len() < threshold)
        || byte_threshold.is_none() && text.len() <= max_chars;
    if skip_truncation {
        return if replace_nul && text.contains('\0') {
            (Cow::Owned(text.replace('\0', "\u{2400}")), false)
        } else {
            (Cow::Borrowed(text), false)
        };
    }
    let cut = text
        .char_indices()
        .nth(max_chars)
        .map_or(text.len(), |(index, _)| index);
    if cut == text.len() {
        return if replace_nul && text.contains('\0') {
            (Cow::Owned(text.replace('\0', "\u{2400}")), false)
        } else {
            (Cow::Borrowed(text), false)
        };
    }
    let head = if replace_nul {
        text[..cut].replace('\0', "\u{2400}")
    } else {
        text[..cut].to_owned()
    };
    let remaining_bytes = text.len() - cut;
    (Cow::Owned(format!("{head}… (+{remaining_bytes} more bytes)")), true)
}

fn json_to_display_text(value: &serde_json::Value) -> String {
    json_to_display_text_with_limits(value, DISPLAY_TEXT_BYTES_THRESHOLD, DISPLAY_TEXT_MAX_CHARS, false)
        .0
        .into_owned()
}

fn json_to_display_text_with_limits(
    value: &serde_json::Value,
    byte_limit: usize,
    char_limit: usize,
    bound_complete: bool,
) -> (Cow<'static, str>, bool) {
    let mut writer = BoundedJsonWriter::new(byte_limit);
    let result = serde_json::to_writer(&mut writer, value);
    let byte_budget_exceeded = result.is_err();
    let (text, truncated) = json_prefix_for_display(writer.into_display(), byte_budget_exceeded, char_limit);
    if truncated {
        (Cow::Owned(text), true)
    } else if bound_complete {
        bounded_owned_text(text, char_limit)
    } else {
        (Cow::Owned(text), false)
    }
}

fn json_prefix_for_display(mut text: String, byte_budget_exceeded: bool, char_limit: usize) -> (String, bool) {
    if !byte_budget_exceeded {
        return (text, false);
    }
    let end = text.char_indices().nth(char_limit).map(|(index, _)| index);
    if let Some(end) = end {
        text.truncate(end);
        return (format!("{text}…"), true);
    }
    (format!("{text}…"), true)
}

struct BoundedJsonWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl BoundedJsonWriter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }

    fn into_display(self) -> String {
        let end = match std::str::from_utf8(&self.bytes) {
            Ok(_) => self.bytes.len(),
            Err(error) => error.valid_up_to(),
        };
        String::from_utf8_lossy(&self.bytes[..end]).into_owned()
    }
}

impl Write for BoundedJsonWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self.limit.saturating_sub(self.bytes.len());
        let written = remaining.min(bytes.len());
        self.bytes.extend_from_slice(&bytes[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn value_to_search_text(value: &Value) -> Cow<'_, str> {
    match value {
        Value::Text(text) | Value::JsonText(text) => Cow::Borrowed(text),
        Value::Bytes(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) if !text.contains('\0') => Cow::Borrowed(text),
            _ => Cow::Owned(format!("<{} bytes>", bytes.len())),
        },
        Value::Json(json) => Cow::Owned(json.to_string()),
        other => Cow::Owned(value_to_text(other, str::to_owned)),
    }
}

pub(crate) fn preview_to_display_text(preview: &crate::ui::row_object::CellPreview) -> String {
    preview_display_text(preview, usize::MAX)
}

fn preview_display_text(preview: &crate::ui::row_object::CellPreview, text_limit: usize) -> String {
    let total = crate::tr!("{n} bytes total").replace("{n}", &preview.byte_count.to_string());
    match &preview.value {
        Value::Text(text) | Value::JsonText(text) => {
            let end = text
                .char_indices()
                .nth(text_limit)
                .map_or(text.len(), |(index, _)| index);
            let sample = text[..end]
                .chars()
                .map(|character| match character {
                    '\0' => '\u{2400}',
                    '\n' | '\u{2028}' | '\u{2029}' => '↵',
                    '\r' => '␍',
                    '\t' => '⇥',
                    character => character,
                })
                .collect::<String>();
            format!("{sample}… ({total})")
        }
        Value::Bytes(bytes) => {
            let head = bytes
                .iter()
                .take(16)
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            format!("{head}… ({total})")
        }
        other => value_to_display_text(other),
    }
}

pub(crate) fn value_to_full_edit_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        other => value_to_text(other, ToString::to_string),
    }
}

pub(super) fn value_is_inline_editable(value: &Value) -> bool {
    match value {
        Value::Bytes(_) | Value::Undecodable(_) => false,
        Value::Text(text) => !text.contains('\0'),
        _ => true,
    }
}

pub(super) fn truncate_for_display(s: &str) -> String {
    if s.len() < DISPLAY_TEXT_BYTES_THRESHOLD {
        return s.to_string();
    }
    let mut cut = s.len();
    for (i, (byte_idx, _)) in s.char_indices().enumerate() {
        if i >= DISPLAY_TEXT_MAX_CHARS {
            cut = byte_idx;
            break;
        }
    }
    if cut >= s.len() {
        return s.to_string();
    }
    let head = &s[..cut];
    let remaining_bytes = s.len() - cut;
    format!("{head}… (+{remaining_bytes} more bytes)")
}

#[derive(Debug)]
pub(super) struct EditSnapshot {
    pub position: u32,
    pub original: String,
    /// Primary-key values of the row the editor opened on. A refresh or
    /// a sort replaces the store's contents, so the position alone stops
    /// identifying that row and a commit would land on whichever row now
    /// sits there.
    pub row_key: Vec<Value>,
}

pub(super) trait WidgetDataKey: 'static {
    type Value: 'static;

    fn quark() -> glib::Quark;
}

pub(super) struct WidgetSlot<K: WidgetDataKey> {
    _key: PhantomData<K>,
}

impl<K: WidgetDataKey> WidgetSlot<K> {
    const fn new() -> Self {
        Self { _key: PhantomData }
    }

    pub(super) fn set(&self, widget: &impl IsA<gtk4::Widget>, value: K::Value) {
        unsafe { widget.set_qdata(K::quark(), value) };
    }

    pub(super) fn take(&self, widget: &impl IsA<gtk4::Widget>) -> Option<K::Value> {
        unsafe { widget.steal_qdata::<K::Value>(K::quark()) }
    }

    pub(super) fn with<R>(&self, widget: &impl IsA<gtk4::Widget>, f: impl FnOnce(&K::Value) -> R) -> Option<R> {
        unsafe { widget.qdata::<K::Value>(K::quark()).map(|p| f(p.as_ref())) }
    }
}

impl<K: WidgetDataKey> WidgetSlot<K>
where
    K::Value: Clone,
{
    pub(super) fn cloned(&self, widget: &impl IsA<gtk4::Widget>) -> Option<K::Value> {
        unsafe { widget.qdata::<K::Value>(K::quark()).map(|p| p.as_ref().clone()) }
    }
}

impl<K: WidgetDataKey> WidgetSlot<K>
where
    K::Value: Copy,
{
    pub(super) fn get(&self, widget: &impl IsA<gtk4::Widget>) -> Option<K::Value> {
        unsafe { widget.qdata::<K::Value>(K::quark()).map(|p| *p.as_ref()) }
    }
}

macro_rules! define_widget_slot {
    ($slot:ident, $key:ident, $value:ty, $quark:literal) => {
        pub(super) enum $key {}

        impl WidgetDataKey for $key {
            type Value = $value;

            fn quark() -> glib::Quark {
                static QUARK: OnceLock<glib::Quark> = OnceLock::new();
                *QUARK.get_or_init(|| glib::Quark::from_str($quark))
            }
        }

        pub(super) const $slot: WidgetSlot<$key> = WidgetSlot::new();
    };
}

define_widget_slot!(
    CELL_MENU_OPTIONS_SLOT,
    CellMenuOptionsKey,
    super::context_menu::CellGestureOptions,
    "tp-cell-menu-options"
);
define_widget_slot!(POSITION_SLOT, PositionKey, u32, "tp-position");
define_widget_slot!(SNAPSHOT_SLOT, SnapshotKey, EditSnapshot, "tp-snapshot");
define_widget_slot!(ROW_KEY_SLOT, RowKeyKey, Vec<Value>, "tp-row-key");
define_widget_slot!(COLUMN_SLOT, ColumnKey, usize, "tp-column");
define_widget_slot!(SUPPRESS_SLOT, SuppressKey, bool, "tp-suppress-toggle");
define_widget_slot!(POPOVER_SLOT, PopoverKey, gtk4::Popover, "tp-popover");
define_widget_slot!(PREEDIT_SLOT, PreeditKey, bool, "tp-preedit-active");
define_widget_slot!(FULL_EDIT_TEXT_SLOT, FullEditTextKey, String, "tp-full-edit-text");
define_widget_slot!(
    FULL_EDIT_KIND_SLOT,
    FullEditKindKey,
    super::full_value_editor::FullValueKind,
    "tp-full-edit-kind"
);
define_widget_slot!(IS_NULL_SLOT, IsNullKey, bool, "tp-cell-is-null");

struct ClickedCell {
    widget: glib::WeakRef<gtk4::Widget>,
    position: u32,
    column: usize,
    row_key: Vec<Value>,
}

thread_local! {
    static LAST_CLICKED_CELL: std::cell::RefCell<Option<ClickedCell>> = const { std::cell::RefCell::new(None) };
}

pub(super) fn remember_clicked_cell(widget: &gtk4::Widget) {
    let (Some(position), Some(column)) = (POSITION_SLOT.get(widget), COLUMN_SLOT.get(widget)) else {
        return;
    };
    let clicked = ClickedCell {
        widget: widget.downgrade(),
        position,
        column,
        row_key: ROW_KEY_SLOT.cloned(widget).unwrap_or_default(),
    };
    LAST_CLICKED_CELL.with(|slot| *slot.borrow_mut() = Some(clicked));
}

pub fn last_clicked_cell(root: &impl IsA<gtk4::Widget>) -> Option<(u32, usize, Vec<Value>)> {
    LAST_CLICKED_CELL.with(|slot| {
        let slot = slot.borrow();
        let clicked = slot.as_ref()?;
        let widget = clicked.widget.upgrade()?;
        (widget.is_mapped() && widget.is_ancestor(root))
            .then(|| (clicked.position, clicked.column, clicked.row_key.clone()))
    })
}

pub fn focused_cell_identity(widget: &impl IsA<gtk4::Widget>) -> Option<(u32, usize, Vec<Value>)> {
    let root = widget.root()?;
    let window = root.dynamic_cast::<gtk4::Window>().ok()?;
    let focused = gtk4::prelude::GtkWindowExt::focus(&window)?;
    let position = POSITION_SLOT.get(&focused)?;
    let column = COLUMN_SLOT.get(&focused)?;
    let row_key = ROW_KEY_SLOT.cloned(&focused).unwrap_or_default();
    Some((position, column, row_key))
}

#[cfg(test)]
#[path = "display_tests.rs"]
mod tests;
