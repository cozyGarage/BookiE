use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::io::{self, Write};

use tablepro_core::Value;

use super::display::{readonly_null_sentinel, value_to_full_edit_text};

const MAX_VIEW_BYTES: usize = 1 << 20;
const HEX_ROW_BYTES: usize = 16;
const HEX_MAX_BYTES: usize = 16 * 1024;

pub(super) fn body(value: &Value) -> String {
    let text = match value {
        Value::Text(text) | Value::JsonText(text) => return capped(text),
        Value::Null => readonly_null_sentinel(),
        Value::Bytes(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) if !text.contains('\0') => return capped(text),
            _ => hex_dump(bytes),
        },
        Value::Json(json) => return json_preview(json),
        other => value_to_full_edit_text(other),
    };
    if text.len() > MAX_VIEW_BYTES {
        capped(&text)
    } else {
        text
    }
}

fn hex_dump(bytes: &[u8]) -> String {
    let shown = &bytes[..bytes.len().min(HEX_MAX_BYTES)];
    let mut out = String::new();
    for (row, chunk) in shown.chunks(HEX_ROW_BYTES).enumerate() {
        let mut hex = String::new();
        for (index, byte) in chunk.iter().enumerate() {
            if index == HEX_ROW_BYTES / 2 {
                hex.push(' ');
            }
            hex.push_str(&format!("{byte:02x} "));
        }
        let ascii: String = chunk
            .iter()
            .map(|byte| {
                if byte.is_ascii_graphic() || *byte == b' ' {
                    *byte as char
                } else {
                    '.'
                }
            })
            .collect();
        out.push_str(&format!("{:08x}  {hex:<49} |{ascii}|\n", row * HEX_ROW_BYTES));
    }
    if shown.len() < bytes.len() {
        out.push_str(
            &crate::tr!("Showing the first {shown} of {total} bytes.")
                .replace("{shown}", &shown.len().to_string())
                .replace("{total}", &bytes.len().to_string()),
        );
    }
    out
}

fn capped(text: &str) -> String {
    if text.len() <= MAX_VIEW_BYTES {
        return text.to_owned();
    }
    let mut end = MAX_VIEW_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n\n{}", &text[..end], truncation_note(end, text.len()))
}

fn json_preview(value: &serde_json::Value) -> String {
    let mut preview = PreviewWriter::default();
    if serde_json::to_writer_pretty(&mut preview, value).is_err() {
        return crate::tr!("This JSON value could not be previewed.");
    }
    let end = std::str::from_utf8(&preview.bytes).map_or_else(|error| error.valid_up_to(), |_| preview.bytes.len());
    let text = String::from_utf8_lossy(&preview.bytes[..end]).into_owned();
    if preview.total <= MAX_VIEW_BYTES {
        return text;
    }
    format!("{text}\n\n{}", truncation_note(end, preview.total))
}

fn truncation_note(shown: usize, total: usize) -> String {
    crate::tr!("Showing the first {shown} of {total} bytes. Copy value copies the whole value.")
        .replace("{shown}", &shown.to_string())
        .replace("{total}", &total.to_string())
}

#[derive(Default)]
struct PreviewWriter {
    bytes: Vec<u8>,
    total: usize,
}

impl Write for PreviewWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let available = MAX_VIEW_BYTES.saturating_sub(self.bytes.len());
        self.bytes.extend_from_slice(&bytes[..bytes.len().min(available)]);
        self.total = self.total.saturating_add(bytes.len());
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn build_text_view(text: &str) -> gtk::TextView {
    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(12)
        .right_margin(12)
        .build();
    let buffer = view.buffer();
    buffer.set_text(text);
    let no_hyphens = gtk::TextTag::builder()
        .name("value-view-no-hyphens")
        .insert_hyphens(false)
        .build();
    buffer.tag_table().add(&no_hyphens);
    buffer.apply_tag(&no_hyphens, &buffer.start_iter(), &buffer.end_iter());
    view
}

pub(crate) fn present(parent: &impl IsA<gtk::Widget>, column: &str, value: Value, null_copy_text: Option<String>) {
    let view = build_text_view(&body(&value));
    let scroller = gtk::ScrolledWindow::builder().child(&view).vexpand(true).build();

    let copy = gtk::Button::with_label(&crate::tr!("Copy value"));
    copy.add_css_class("suggested-action");
    copy.connect_clicked(move |button| {
        let text = null_copy_text
            .as_deref()
            .map(str::to_owned)
            .unwrap_or_else(|| value_to_full_edit_text(&value));
        button.clipboard().set_text(&text);
    });
    let header = adw::HeaderBar::new();
    header.pack_end(&copy);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scroller));

    let dialog = adw::Dialog::builder()
        .title(column)
        .content_width(640)
        .content_height(480)
        .child(&toolbar)
        .build();
    dialog
        .upcast_ref::<gtk::Widget>()
        .update_property(&[gtk::accessible::Property::Label(column)]);
    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_text_view_keeps_extra_memory_bounded() {
        const CHILD_KIND: &str = "BOOKIE_VALUE_VIEW_MEMORY_KIND";
        if let Ok(kind) = std::env::var(CHILD_KIND) {
            let text = "é".repeat(32 * 1024 * 1024);
            let expected_bytes = text.len() + usize::from(kind == "json") * 2;
            let value = match kind.as_str() {
                "text" => Value::Text(text),
                "json-text" => Value::JsonText(text),
                "bytes" => Value::Bytes(text.into_bytes()),
                "json" => Value::Json(Box::new(serde_json::Value::String(text))),
                _ => panic!("unknown synthetic value kind"),
            };
            let before = peak_memory_kib();
            let shown = body(&value);
            let extra = peak_memory_kib().saturating_sub(before);
            eprintln!("viewer-memory kind={kind} input_bytes={expected_bytes} extra_peak_kib={extra}");
            assert!(shown.len() < MAX_VIEW_BYTES + 200);
            assert!(shown.contains(&format!("{expected_bytes} bytes")));
            assert!(
                extra < 16 * 1024,
                "capped value viewing allocated {extra} KiB beyond its input"
            );
            return;
        }
        for kind in ["text", "json-text", "bytes", "json"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "ui::grid::value_viewer::tests::oversized_text_view_keeps_extra_memory_bounded",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD_KIND, kind)
                .output()
                .unwrap();
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
        }
    }

    fn peak_memory_kib() -> u64 {
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("VmHWM:"))
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    #[test]
    fn json_looking_text_preserves_duplicate_keys_numbers_and_whitespace() {
        for text in [
            r#"{"a":1,"a":2}"#,
            r#"  {"n":123456789012345678901234567890.000,"e":1e+004}  "#,
            "\n[1, 2, 3]\t",
            r#"{"escape":"\u0061","unicode":"é"}"#,
        ] {
            for value in [
                Value::Text(text.into()),
                Value::JsonText(text.into()),
                Value::Bytes(text.as_bytes().to_vec()),
            ] {
                assert_eq!(body(&value), text);
            }
        }
    }

    #[test]
    fn text_is_shown_without_reformatting() {
        assert_eq!(body(&Value::Text(r#"{"a":1}"#.into())), r#"{"a":1}"#);
        assert_eq!(body(&Value::Text("{not json".into())), "{not json");
        assert_eq!(body(&Value::Text("hello".into())), "hello");
    }

    #[test]
    fn a_json_value_is_pretty_printed() {
        assert_eq!(
            body(&Value::Json(Box::new(serde_json::json!([1, 2])))),
            "[\n  1,\n  2\n]"
        );
    }

    #[test]
    fn null_reads_as_null() {
        assert_eq!(body(&Value::Null), readonly_null_sentinel());
    }

    #[test]
    fn binary_bytes_show_a_hex_dump_and_utf8_bytes_show_text() {
        let dump = body(&Value::Bytes(vec![0x00, 0x41, 0xff]));
        assert!(dump.starts_with("00000000  00 41 ff "));
        assert!(dump.trim_end().ends_with("|.A.|"));
        assert_eq!(body(&Value::Bytes(b"plain".to_vec())), "plain");
    }

    #[test]
    fn a_long_hex_dump_says_how_much_it_shows() {
        let dump = body(&Value::Bytes(vec![0u8; MAX_VIEW_BYTES]));
        assert!(dump.contains("Showing the first 16384 of 1048576 bytes."));
    }

    #[test]
    fn oversized_text_is_cut_on_a_character_boundary_with_a_note() {
        let text = "é".repeat(MAX_VIEW_BYTES);
        let shown = body(&Value::Text(text));
        assert!(shown.contains("Showing the first"));
        assert!(shown.len() < MAX_VIEW_BYTES + 200);
    }

    #[test]
    fn oversized_typed_json_preview_is_bounded_and_reports_serialized_size() {
        let value = Value::Json(Box::new(serde_json::Value::String("é".repeat(MAX_VIEW_BYTES))));
        let shown = body(&value);
        let (preview, note) = shown.split_once("\n\n").expect("truncation notice");
        assert!(preview.len() <= MAX_VIEW_BYTES);
        assert!(note.contains(&format!("of {} bytes", MAX_VIEW_BYTES * 2 + 2)));
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn the_viewer_dialog_builds_and_shows_the_full_value() {
        gtk::init().unwrap();
        adw::init().unwrap();
        let view = build_text_view(&"a".repeat(64));
        let buffer = view.buffer();
        let tag = buffer.tag_table().lookup("value-view-no-hyphens").unwrap();
        assert!(!tag.property::<bool>("insert-hyphens"));
        assert_eq!(view.wrap_mode(), gtk::WrapMode::WordChar);
        let start = buffer.start_iter();
        assert!(start.has_tag(&tag));
        let mut end = buffer.end_iter();
        end.backward_char();
        assert!(end.has_tag(&tag));
        let window = adw::Window::new();
        window.present();
        present(&window, "payload", Value::Text(r#"{"a":1}"#.into()), None);
        let dialogs = window.observe_children().n_items();
        assert!(dialogs >= 1);
        window.close();
    }
}
