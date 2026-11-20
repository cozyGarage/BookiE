use bookie_app_services::sql_file::sql_text_error_message;
pub(crate) use bookie_app_services::sql_file::{file_error_message, read_sql_file, save_sql_file, validate_sql_text};
pub(super) use bookie_app_services::sql_file::{validate_dropped_sql, validate_sql_line_lengths};
use bookie_app_services::sql_text::SqlTextLimits;
use bookie_core::text_file::TextFile;
use relm4::gtk;
use relm4::gtk::prelude::*;

pub(super) fn validate_buffer_insert(
    buffer: &gtk::TextBuffer,
    location: &gtk::TextIter,
    text: &str,
    current_bytes: usize,
) -> Result<(), String> {
    let mut line_start = *location;
    line_start.set_line_offset(0);
    let mut line_end = *location;
    line_end.forward_to_line_end();
    let prefix_len = buffer.text(&line_start, location, true).len();
    let suffix_len = buffer.text(location, &line_end, true).len();
    let first_line = location.line().max(0) as usize + 1;
    SqlTextLimits::EDITOR
        .validate_insertion(current_bytes, prefix_len, suffix_len, first_line, text)
        .map_err(sql_text_error_message)
}

fn sql_filters() -> gtk::gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&bookie_app_services::tr!("SQL files")));
    filter.add_pattern("*.sql");
    filter.add_mime_type("application/sql");
    filter.add_mime_type("text/x-sql");
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters
}

fn sql_file_dialog(title: &str) -> gtk::FileDialog {
    let dialog = gtk::FileDialog::builder().title(title).modal(true).build();
    let filters = sql_filters();
    let default = filters.item(0).and_downcast::<gtk::FileFilter>();
    dialog.set_filters(Some(&filters));
    dialog.set_default_filter(default.as_ref());
    dialog
}

pub(crate) fn choose_sql_file<F>(parent: Option<gtk::Window>, done: F)
where
    F: FnOnce(Result<TextFile, String>) + 'static,
{
    let dialog = sql_file_dialog(&bookie_app_services::tr!("Open SQL file"));
    dialog.open(parent.as_ref(), gtk::gio::Cancellable::NONE, move |result| {
        let Ok(file) = result else {
            return;
        };
        let Some(path) = file.path() else {
            done(Err(bookie_app_services::tr!("That file is not on this computer")));
            return;
        };
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(read_sql_file(&path));
        });
        relm4::spawn_local(async move {
            if let Ok(outcome) = rx.recv().await {
                done(outcome);
            }
        });
    });
}

pub(crate) fn choose_save_path<F>(parent: Option<gtk::Window>, suggested_name: &str, done: F)
where
    F: FnOnce(Option<Result<std::path::PathBuf, String>>) + 'static,
{
    let dialog = sql_file_dialog(&bookie_app_services::tr!("Save SQL file"));
    dialog.set_initial_name(Some(suggested_name));
    dialog.save(parent.as_ref(), gtk::gio::Cancellable::NONE, move |result| {
        let Ok(file) = result else {
            done(None);
            return;
        };
        match file.path() {
            Some(path) => done(Some(Ok(path))),
            None => done(Some(Err(bookie_app_services::tr!("That file is not on this computer")))),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::super::MAX_SQL_FILE_BYTES;
    use std::io::Write;

    #[test]
    fn dropped_sql_uses_the_same_long_line_boundary() {
        let text = "é".repeat(128 * 1024 + 1);
        let (_dir, path) = {
            let dir = tempfile::tempdir().expect("temp dir");
            let path = dir.path().join("script.sql");
            std::fs::File::create(&path)
                .unwrap()
                .write_all(text.as_bytes())
                .unwrap();
            (dir, path)
        };
        assert!(super::super::read_sql_text(&path, MAX_SQL_FILE_BYTES).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }
}
