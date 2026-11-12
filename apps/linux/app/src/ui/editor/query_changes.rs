use std::cell::{Cell, RefCell};
use std::rc::Rc;

use relm4::gtk::prelude::*;
use relm4::{ComponentSender, gtk};
use sourceview5::prelude::*;

use super::{DropGeneration, SqlEditor, SqlEditorOutput, folding::FoldController};

pub(crate) const DEFAULT_INITIAL_QUERY: &str = "SELECT 1;";
pub(super) const MAX_SYNTAX_HIGHLIGHT_BYTES: usize = 100_000;

pub(super) fn install(
    view: &sourceview5::View,
    generation: Rc<DropGeneration>,
    folding: Rc<RefCell<FoldController>>,
    status: gtk::Label,
    sender: ComponentSender<SqlEditor>,
    refresh: Rc<dyn Fn()>,
) {
    let depth = Rc::new(Cell::new(0u32));
    let pending = Rc::new(Cell::new(false));
    let buffer = view.buffer();
    let (start, end) = buffer.bounds();
    let bytes = Rc::new(Cell::new(buffer.text(&start, &end, true).len()));
    update_syntax_highlighting(&buffer, view, bytes.get());
    let deleted_text = install_edit_guards(&buffer, status, bytes.clone());
    let highlight_bytes = bytes.clone();
    let highlight_view = view.downgrade();
    let publish_bytes = bytes;
    let publish = Rc::new(move |buffer: &gtk::TextBuffer| {
        if let Some(buffer) = buffer.downcast_ref::<sourceview5::Buffer>() {
            folding.borrow_mut().clear(buffer);
        }
        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, true).to_string();
        publish_bytes.set(text.len());
        let _ = sender.output(SqlEditorOutput::QueryChanged(text));
        refresh();
    });
    let begin_depth = depth.clone();
    buffer.connect_begin_user_action(move |_| begin_depth.set(begin_depth.get().saturating_add(1)));
    let end_depth = depth.clone();
    let end_pending = pending.clone();
    let end_deleted_text = deleted_text;
    let end_publish = publish.clone();
    buffer.connect_end_user_action(move |buffer| {
        end_depth.set(end_depth.get().saturating_sub(1));
        end_deleted_text.borrow_mut().take();
        if end_depth.get() == 0 && end_pending.replace(false) {
            end_publish(buffer);
        }
    });
    buffer.connect_changed(move |buffer| {
        generation.changed();
        if let Some(view) = highlight_view.upgrade() {
            update_syntax_highlighting(buffer, &view, highlight_bytes.get());
        }
        if depth.get() > 0 {
            pending.set(true);
            return;
        }
        publish(buffer);
    });
}

fn update_syntax_highlighting(buffer: &gtk::TextBuffer, view: &sourceview5::View, bytes: usize) {
    let Some(buffer) = buffer.downcast_ref::<sourceview5::Buffer>() else {
        return;
    };
    let enabled = bytes <= MAX_SYNTAX_HIGHLIGHT_BYTES;
    if buffer.is_highlight_syntax() != enabled {
        buffer.set_highlight_syntax(enabled);
        let tooltip = (!enabled).then(|| {
            bookie_app_services::tr!("SQL syntax highlighting is paused for large scripts to keep editing responsive.")
        });
        view.set_tooltip_text(tooltip.as_deref());
    }
}

fn install_edit_guards(
    buffer: &gtk::TextBuffer,
    status: gtk::Label,
    bytes: Rc<Cell<usize>>,
) -> Rc<RefCell<Option<(u64, i32, String)>>> {
    let deleted_text: Rc<RefCell<Option<(u64, i32, String)>>> = Rc::new(RefCell::new(None));
    let generation = Rc::new(Cell::new(0u64));
    let insert_bytes = bytes.clone();
    let insert_deleted_text = deleted_text.clone();
    let insert_generation = generation.clone();
    buffer.connect_insert_text(move |buffer, location, text| {
        if let Err(message) = super::open_file::validate_buffer_insert(buffer, location, text, insert_bytes.get()) {
            buffer.stop_signal_emission_by_name("insert-text");
            status.set_label(&message);
            insert_generation.set(insert_generation.get().wrapping_add(1));
            if let Some((_, offset, deleted)) = insert_deleted_text.borrow_mut().take() {
                let buffer = buffer.clone();
                gtk::glib::idle_add_local_once(move || {
                    let mut location = buffer.iter_at_offset(offset);
                    buffer.insert(&mut location, &deleted);
                });
            }
            return;
        }
        insert_bytes.set(insert_bytes.get().saturating_add(text.len()));
        insert_generation.set(insert_generation.get().wrapping_add(1));
        insert_deleted_text.borrow_mut().take();
    });
    let delete_bytes = bytes;
    let delete_text = deleted_text.clone();
    buffer.connect_delete_range(move |buffer, start, end| {
        let deleted = buffer.text(start, end, true).to_string();
        let current = generation.get().wrapping_add(1);
        generation.set(current);
        *delete_text.borrow_mut() = Some((current, start.offset(), deleted.clone()));
        delete_bytes.set(delete_bytes.get().saturating_sub(deleted.len()));
        let clear_text = delete_text.clone();
        let clear_generation = generation.clone();
        gtk::glib::idle_add_local_once(move || {
            if clear_generation.get() == current {
                clear_text.borrow_mut().take();
            }
        });
    });
    deleted_text
}
