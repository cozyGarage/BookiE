use gtk4 as gtk;
use gtk4::prelude::*;

use bookie_app_services::decoded_preview::{DecodedFormat, DecodedPreview, decoded_preview};
use bookie_core::Value;

pub(super) fn preview_of(value: &Value) -> Option<DecodedPreview> {
    let Value::Bytes(bytes) = value else {
        return None;
    };
    if matches!(std::str::from_utf8(bytes), Ok(text) if !text.contains('\0')) {
        return None;
    }
    decoded_preview(bytes)
}

pub(super) fn heading(format: DecodedFormat) -> String {
    match format {
        DecodedFormat::MessagePack => bookie_app_services::tr!("Decoded as MessagePack"),
        DecodedFormat::Protobuf => {
            bookie_app_services::tr!("Decoded as Protobuf without a schema (field numbers only, a best guess)")
        }
    }
}

fn footer(preview: &DecodedPreview) -> String {
    if preview.truncated {
        return bookie_app_services::tr!("Read only and cut short. Copy value copies the original bytes.");
    }
    bookie_app_services::tr!("Read only. Copy value copies the original bytes.")
}

pub(super) fn section(value: &Value) -> Option<gtk::Box> {
    let preview = preview_of(value)?;
    let title = gtk::Label::builder()
        .label(heading(preview.format))
        .xalign(0.0)
        .wrap(true)
        .margin_top(8)
        .margin_start(12)
        .margin_end(12)
        .build();
    title.add_css_class("heading");
    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::None)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(12)
        .right_margin(12)
        .build();
    view.buffer().set_text(&preview.text);
    view.update_property(&[gtk::accessible::Property::Label(&heading(preview.format))]);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&view)
        .min_content_height(160)
        .build();
    let note = gtk::Label::builder()
        .label(footer(&preview))
        .xalign(0.0)
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(8)
        .build();
    note.add_css_class("dim-label");
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.append(&title);
    container.append(&scroller);
    container.append(&note);
    Some(container)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_binary_bytes_get_a_decoded_section() {
        let packed = vec![0x82, 0xa1, b'a', 0x01, 0xa1, b'b', 0xc3];
        let preview = preview_of(&Value::Bytes(packed.clone())).expect("binary msgpack decodes");
        assert_eq!(preview.format, DecodedFormat::MessagePack);
        assert!(preview_of(&Value::Text("hello".into())).is_none());
        assert!(preview_of(&Value::Json(Box::new(serde_json::json!([1, 2])))).is_none());
        assert!(preview_of(&Value::Bytes(b"[1,2]".to_vec())).is_none());
        assert!(preview_of(&Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef])).is_none());
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn the_decoded_section_is_read_only_and_absent_for_unrecognised_bytes() {
        gtk::init().expect("display");
        let packed = Value::Bytes(vec![0x82, 0xa1, b'a', 0x01, 0xa1, b'b', 0xc3]);
        let section = section(&packed).expect("section");
        let scroller = section
            .first_child()
            .and_then(|title| title.next_sibling())
            .expect("scroller");
        let view = scroller
            .downcast::<gtk::ScrolledWindow>()
            .expect("scroller")
            .child()
            .and_downcast::<gtk::TextView>()
            .expect("text view");
        assert!(!view.is_editable());
        let buffer = view.buffer();
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        assert!(text.contains("\"a\": 1"));
        assert!(super::section(&Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef])).is_none());
    }
}
