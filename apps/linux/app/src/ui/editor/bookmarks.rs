use bookie_app_services::editor_bookmarks::BookmarkSet;
use sourceview5::prelude::*;

pub(super) const CATEGORY: &str = "sql-bookmark";
const MARK_PRIORITY: i32 = 20;

pub(super) fn install(view: &sourceview5::View) {
    view.set_show_line_marks(true);
    let attributes = sourceview5::MarkAttributes::new();
    attributes.set_icon_name("bookmark-new-symbolic");
    view.set_mark_attributes(CATEGORY, &attributes, MARK_PRIORITY);
}

pub(super) fn toggle_at_line(buffer: &sourceview5::Buffer, line: i32) -> bool {
    if line < 0 {
        return false;
    }
    if !buffer.source_marks_at_line(line, Some(CATEGORY)).is_empty() {
        remove_at_line(buffer, line);
        return false;
    }
    let Some(iter) = buffer.iter_at_line(line) else {
        return false;
    };
    buffer.create_source_mark(None, CATEGORY, &iter);
    true
}

pub(super) fn toggle_at_cursor(buffer: &sourceview5::Buffer) -> bool {
    let line = buffer.iter_at_mark(&buffer.get_insert()).line();
    toggle_at_line(buffer, line)
}

pub(super) fn remove_at_line(buffer: &sourceview5::Buffer, line: i32) {
    let Some(start) = buffer.iter_at_line(line) else {
        return;
    };
    let mut end = start;
    if !end.forward_line() {
        end = buffer.end_iter();
    }
    buffer.remove_source_marks(&start, &end, Some(CATEGORY));
}

pub(super) fn clear(buffer: &sourceview5::Buffer) {
    buffer.remove_source_marks(&buffer.start_iter(), &buffer.end_iter(), Some(CATEGORY));
}

pub(super) fn jump(view: &sourceview5::View, buffer: &sourceview5::Buffer, forward: bool) -> bool {
    let lines = collect_lines(buffer);
    let from = buffer.iter_at_mark(&buffer.get_insert()).line() as u32;
    let Some(target) = BookmarkSet::from_lines(lines).adjacent(from, forward) else {
        return false;
    };
    let Some(iter) = buffer.iter_at_line(target as i32) else {
        return false;
    };
    buffer.place_cursor(&iter);
    view.scroll_mark_onscreen(&buffer.get_insert());
    true
}

fn collect_lines(buffer: &sourceview5::Buffer) -> Vec<u32> {
    (0..buffer.line_count())
        .filter(|&line| !buffer.source_marks_at_line(line, Some(CATEGORY)).is_empty())
        .map(|line| line as u32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use relm4::gtk;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn toggle_jump_and_clear_move_source_marks_with_the_cursor() {
        relm4::adw::init().unwrap();
        let buffer = sourceview5::Buffer::new(None::<&gtk::TextTagTable>);
        buffer.set_text("SELECT 1;\nSELECT 2;\nSELECT 3;\nSELECT 4;");
        let view = sourceview5::View::with_buffer(&buffer);
        install(&view);

        buffer.place_cursor(&buffer.iter_at_line(1).unwrap());
        assert!(toggle_at_cursor(&buffer));
        assert_eq!(buffer.source_marks_at_line(1, Some(CATEGORY)).len(), 1);

        buffer.place_cursor(&buffer.iter_at_line(3).unwrap());
        assert!(toggle_at_cursor(&buffer));
        buffer.place_cursor(&buffer.start_iter());
        assert!(jump(&view, &buffer, true));
        assert_eq!(buffer.iter_at_mark(&buffer.get_insert()).line(), 1);
        assert!(jump(&view, &buffer, true));
        assert_eq!(buffer.iter_at_mark(&buffer.get_insert()).line(), 3);
        assert!(!jump(&view, &buffer, true));

        clear(&buffer);
        assert!(collect_lines(&buffer).is_empty());
        assert!(!jump(&view, &buffer, true));
    }
}
