use super::stream_pages::StreamTerminal;
use relm4::gtk::prelude::*;
use relm4::{adw, gtk};
use sourceview5::prelude::*;

use super::{ExecutionContext, SqlEditor, StatementOutcome, StatementOutcomeKind, statement_cursor::script_statements};

const OK_CATEGORY: &str = "tp-run-ok";
const ERROR_CATEGORY: &str = "tp-run-error";
const MARK_PRIORITY: i32 = 10;
const MARK_SIZE: i32 = 16;
const MARK_RADIUS: f32 = 6.5;

pub(super) fn statement_start_lines(text: &str, planned: &super::PlannedStatements) -> Vec<Option<u32>> {
    if planned.source_ranges.len() != planned.statements.len() {
        return fallback_start_lines(text, &planned.statements);
    }
    let mut cursor = Some((0, 0));
    planned
        .source_ranges
        .iter()
        .map(|range| {
            let (from, line) = cursor.take()?;
            let line = line
                + text
                    .get(from..range.start)?
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count() as u32;
            cursor = Some((range.start, line));
            Some(line)
        })
        .collect()
}

fn fallback_start_lines(text: &str, statements: &[String]) -> Vec<Option<u32>> {
    let mut from = 0;
    statements
        .iter()
        .map(|statement| {
            let found = text.get(from..)?.find(statement.as_str())?;
            let start = from + found;
            from = start + statement.len();
            Some(text[..start].matches('\n').count() as u32)
        })
        .collect()
}

fn category_of(outcome: &StatementOutcome) -> Option<&'static str> {
    match &outcome.kind {
        StatementOutcomeKind::Rows(_) => Some(OK_CATEGORY),
        StatementOutcomeKind::Streamed(result) => match &result.terminal {
            StreamTerminal::Complete => Some(OK_CATEGORY),
            StreamTerminal::Failed(_) | StreamTerminal::TimedOut => Some(ERROR_CATEGORY),
            StreamTerminal::Loading | StreamTerminal::Cancelled => None,
        },
        StatementOutcomeKind::Error(_) => Some(ERROR_CATEGORY),
        StatementOutcomeKind::NotRun => None,
    }
}

fn mark_colors(dark: bool) -> [(u8, u8, u8); 2] {
    if dark {
        [(0x57, 0xe3, 0x89), (0xff, 0x7b, 0x72)]
    } else {
        [(0x1b, 0x7f, 0x3a), (0xc0, 0x1c, 0x28)]
    }
}

fn relative_luminance(channel: u8) -> f64 {
    let value = f64::from(channel) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn contrast_ratio(foreground: (u8, u8, u8), background: (u8, u8, u8)) -> f64 {
    let left = 0.2126 * relative_luminance(foreground.0)
        + 0.7152 * relative_luminance(foreground.1)
        + 0.0722 * relative_luminance(foreground.2);
    let right = 0.2126 * relative_luminance(background.0)
        + 0.7152 * relative_luminance(background.1)
        + 0.0722 * relative_luminance(background.2);
    let (darker, lighter) = if left < right { (left, right) } else { (right, left) };
    (lighter + 0.05) / (darker + 0.05)
}

fn mark_pixbuf(color: (u8, u8, u8)) -> Option<gtk::gdk_pixbuf::Pixbuf> {
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, MARK_SIZE, MARK_SIZE)?;
    pixbuf.fill(0);
    let center = (MARK_SIZE as f32 - 1.0) / 2.0;
    for y in 0..MARK_SIZE {
        for x in 0..MARK_SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            if dx * dx + dy * dy <= MARK_RADIUS * MARK_RADIUS {
                pixbuf.put_pixel(x as u32, y as u32, color.0, color.1, color.2, 255);
            }
        }
    }
    Some(pixbuf)
}

fn apply_mark_icons(view: &sourceview5::View) {
    let [ok, error] = mark_colors(adw::StyleManager::default().is_dark());
    for (category, color) in [(OK_CATEGORY, ok), (ERROR_CATEGORY, error)] {
        let attributes = sourceview5::MarkAttributes::new();
        if let Some(pixbuf) = mark_pixbuf(color) {
            attributes.set_pixbuf(&pixbuf);
        }
        view.set_mark_attributes(category, &attributes, MARK_PRIORITY);
    }
}

pub(super) fn install(view: &sourceview5::View) {
    view.set_show_line_marks(true);
    if let Ok(buffer) = view.buffer().downcast::<sourceview5::Buffer>() {
        buffer.connect_changed(clear);
    }
    apply_mark_icons(view);
    let view = view.downgrade();
    adw::StyleManager::default().connect_dark_notify(move |_| {
        if let Some(view) = view.upgrade() {
            apply_mark_icons(&view);
        }
    });
}

pub(super) fn clear(buffer: &sourceview5::Buffer) {
    let (start, end) = buffer.bounds();
    for category in [OK_CATEGORY, ERROR_CATEGORY] {
        buffer.remove_source_marks(&start, &end, Some(category));
    }
}

impl SqlEditor {
    pub(super) fn clear_run_marks(&self) {
        if let Ok(buffer) = self.source_view.buffer().downcast::<sourceview5::Buffer>() {
            clear(&buffer);
        }
    }

    pub(super) fn mark_statements(&self, context: &ExecutionContext, current: bool, outcomes: &[StatementOutcome]) {
        if !current {
            return;
        }
        if let Ok(buffer) = self.source_view.buffer().downcast::<sourceview5::Buffer>() {
            let lexing = super::statement_cursor::Lexing {
                driver: &context.metadata.driver_id,
                settings: context.lexical,
            };
            mark_outcomes(&buffer, &context.sql, lexing, outcomes);
        }
    }
}

fn mark_outcomes<'a>(
    buffer: &sourceview5::Buffer,
    sql: &str,
    lexing: impl Into<super::statement_cursor::Lexing<'a>>,
    outcomes: &[StatementOutcome],
) {
    let (start, end) = buffer.bounds();
    let text = buffer.text(&start, &end, true).to_string();
    let Ok(planned) = script_statements(sql, lexing) else {
        return;
    };
    if text.trim() != sql || planned.statements.len() != outcomes.len() {
        return;
    }
    let leading_lines = text[..text.len() - text.trim_start().len()].matches('\n').count() as u32;
    clear(buffer);
    for (line, outcome) in statement_start_lines(sql, &planned).iter().zip(outcomes) {
        let (Some(line), Some(category)) = (line, category_of(outcome)) else {
            continue;
        };
        let Some(at) = buffer.iter_at_line((line + leading_lines) as i32) else {
            continue;
        };
        buffer.create_source_mark(None, category, &at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_statement_is_placed_on_the_line_where_it_starts() {
        let text = "SELECT 1;\nSELECT 2;\n\nSELECT 3;";
        let planned = script_statements(text, "postgres").unwrap();
        assert_eq!(statement_start_lines(text, &planned), vec![Some(0), Some(1), Some(3)]);
    }

    #[test]
    fn a_repeated_statement_is_matched_in_order() {
        let text = "SELECT 1;\nSELECT 1;";
        let planned = script_statements(text, "postgres").unwrap();
        assert_eq!(statement_start_lines(text, &planned), vec![Some(0), Some(1)]);
    }

    #[test]
    fn sql_inside_a_comment_does_not_receive_the_statement_mark() {
        assert_eq!(
            statement_start_lines(
                "-- SELECT broken\nSELECT broken;",
                &script_statements("-- SELECT broken\nSELECT broken;", "postgres").unwrap(),
            ),
            vec![Some(1)]
        );
    }

    #[test]
    fn a_statement_missing_from_the_text_has_no_line() {
        let statements = vec!["SELECT 1".to_string(), "DROP x".to_string()];
        assert_eq!(fallback_start_lines("SELECT 1;", &statements), vec![Some(0), None]);
    }

    #[test]
    fn gutter_mark_colors_meet_non_text_contrast_on_light_dark_and_high_contrast() {
        let light = mark_colors(false);
        let dark = mark_colors(true);
        let cases = [
            ("light ok on white", light[0], (255, 255, 255)),
            ("light error on white", light[1], (255, 255, 255)),
            ("light ok on gruvbox view", light[0], (249, 245, 215)),
            ("light error on gruvbox view", light[1], (249, 245, 215)),
            ("dark ok on near-black", dark[0], (30, 30, 30)),
            ("dark error on near-black", dark[1], (30, 30, 30)),
            ("high-contrast ok on black", dark[0], (0, 0, 0)),
            ("high-contrast error on black", dark[1], (0, 0, 0)),
            ("high-contrast ok on white", light[0], (255, 255, 255)),
            ("high-contrast error on white", light[1], (255, 255, 255)),
        ];
        for (label, mark, background) in cases {
            let ratio = contrast_ratio(mark, background);
            assert!(ratio >= 3.0, "{label}: mark {mark:?} on {background:?} is {ratio:.2}:1");
        }
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn each_statement_gets_a_gutter_mark_for_its_outcome() {
        relm4::adw::init().unwrap();
        let buffer = sourceview5::Buffer::new(None::<&relm4::gtk::TextTagTable>);
        buffer.set_text("SELECT 1;\nSELECT x;\nSELECT 3;");
        let outcome = |kind| StatementOutcome {
            identity: super::super::StatementIdentity::default(),
            sql_preview: String::new(),
            elapsed_ms: 0,
            kind,
            pinned: false,
        };
        let outcomes = [
            outcome(StatementOutcomeKind::Rows(Vec::new())),
            outcome(StatementOutcomeKind::Error("boom".into())),
            outcome(StatementOutcomeKind::NotRun),
        ];

        mark_outcomes(&buffer, "SELECT 1;\nSELECT x;\nSELECT 3;", "sqlite", &outcomes);

        assert_eq!(buffer.source_marks_at_line(0, Some(OK_CATEGORY)).len(), 1);
        assert_eq!(buffer.source_marks_at_line(1, Some(ERROR_CATEGORY)).len(), 1);
        assert!(buffer.source_marks_at_line(2, None).is_empty());

        let view = sourceview5::View::with_buffer(&buffer);
        install(&view);
        buffer.insert_at_cursor("-- edit");
        assert!(
            buffer.source_marks_at_line(0, None).is_empty(),
            "an edit drops the marks"
        );

        clear(&buffer);
        buffer.set_text("SELECT 1;\nSELECT x;\nSELECT 3;\n-- edited");
        mark_outcomes(&buffer, "SELECT 1;\nSELECT x;\nSELECT 3;", "sqlite", &outcomes);
        assert!(
            buffer.source_marks_at_line(0, None).is_empty(),
            "a script edited since the run gets no marks"
        );
    }
}
