use bookie_app_services::highlight_match::{
    DisplayCell, HighlightPaint, HighlightTarget, PreparedHighlightRule, compute_row_rule_matches, paint_for_cell,
    paint_for_cell_with_row_matches,
};
use bookie_core::ColumnInfo;
use gtk4::prelude::*;

use crate::ui::row_object::RowObject;

const HIGHLIGHT_CSS_CLASSES: &[&str] = &[
    "tp-hl-cell-blue",
    "tp-hl-cell-teal",
    "tp-hl-cell-green",
    "tp-hl-cell-yellow",
    "tp-hl-cell-orange",
    "tp-hl-cell-red",
    "tp-hl-cell-purple",
    "tp-hl-cell-brown",
    "tp-hl-row-blue",
    "tp-hl-row-teal",
    "tp-hl-row-green",
    "tp-hl-row-yellow",
    "tp-hl-row-orange",
    "tp-hl-row-red",
    "tp-hl-row-purple",
    "tp-hl-row-brown",
];

const CELL_CLASSES: &[(&str, &str)] = &[
    ("blue", "tp-hl-cell-blue"),
    ("teal", "tp-hl-cell-teal"),
    ("green", "tp-hl-cell-green"),
    ("yellow", "tp-hl-cell-yellow"),
    ("orange", "tp-hl-cell-orange"),
    ("red", "tp-hl-cell-red"),
    ("purple", "tp-hl-cell-purple"),
    ("brown", "tp-hl-cell-brown"),
];

const ROW_CLASSES: &[(&str, &str)] = &[
    ("blue", "tp-hl-row-blue"),
    ("teal", "tp-hl-row-teal"),
    ("green", "tp-hl-row-green"),
    ("yellow", "tp-hl-row-yellow"),
    ("orange", "tp-hl-row-orange"),
    ("red", "tp-hl-row-red"),
    ("purple", "tp-hl-row-purple"),
    ("brown", "tp-hl-row-brown"),
];

pub(super) fn css_class_for(paint: HighlightPaint) -> Option<&'static str> {
    let table = match paint.target {
        HighlightTarget::Cell => CELL_CLASSES,
        HighlightTarget::Row => ROW_CLASSES,
    };
    table
        .iter()
        .find(|(color, _)| *color == paint.color)
        .map(|(_, class)| *class)
}

pub(super) fn apply_highlight(widget: &gtk4::Widget, paint: Option<HighlightPaint>) {
    for class in HIGHLIGHT_CSS_CLASSES {
        widget.remove_css_class(class);
    }
    if let Some(class) = paint.and_then(css_class_for) {
        widget.add_css_class(class);
    }
}

pub(super) fn clear_highlight(widget: &gtk4::Widget) {
    for class in HIGHLIGHT_CSS_CLASSES {
        widget.remove_css_class(class);
    }
}

pub(super) fn paint_for_bound_cell(
    rules: &std::sync::Arc<[PreparedHighlightRule]>,
    column: usize,
    shown_text: &str,
    shown_null: bool,
    row: &RowObject,
    columns: &[ColumnInfo],
) -> Option<HighlightPaint> {
    if rules.is_empty() {
        return None;
    }
    let cell = DisplayCell {
        text: shown_text,
        is_null: shown_null,
    };
    if rules.len() > u64::BITS as usize {
        return paint_for_cell(rules, column, cell, |index| highlight_source_cell(row, index, columns));
    }
    let row_matches = row.highlight_row_matches(rules, || {
        compute_row_rule_matches(rules, |idx| highlight_source_cell(row, idx, columns))
    });
    paint_for_cell_with_row_matches(rules, column, cell, row_matches)
}

fn highlight_source_cell(row: &RowObject, index: usize, columns: &[ColumnInfo]) -> Option<(String, bool)> {
    if let Some(preview) = row.cell_preview(index) {
        return Some((super::display::preview_display_text(&preview, 512), false));
    }
    row.with_cell_value(index, |value| {
        let column = columns.get(index)?;
        Some(super::display::highlight_display_text(value, column))
    })
    .flatten()
}

#[cfg(test)]
mod tests {
    use bookie_app_services::highlight_match::{HighlightOp, HighlightRule, HighlightTarget, prepare_rules};
    use bookie_core::{ColumnInfo, FilterValue, Value};

    use super::*;

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
            primary_key: false,
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
    fn css_class_maps_palette_colour_and_rejects_unknown() {
        assert_eq!(
            css_class_for(HighlightPaint {
                color: "red",
                target: HighlightTarget::Cell,
            }),
            Some("tp-hl-cell-red")
        );
        assert_eq!(
            css_class_for(HighlightPaint {
                color: "red",
                target: HighlightTarget::Row,
            }),
            Some("tp-hl-row-red")
        );
        assert_eq!(
            css_class_for(HighlightPaint {
                color: "chartreuse",
                target: HighlightTarget::Cell,
            }),
            None
        );
    }

    #[test]
    fn bound_paint_uses_shown_text_and_leaves_row_values_unchanged() {
        let rules = prepare_rules(
            &[HighlightRule {
                column: "status".into(),
                op: HighlightOp::Eq,
                value: Some(FilterValue::Single("bad".into())),
                color: "red".into(),
                target: HighlightTarget::Cell,
            }],
            &["status"],
        )
        .expect("prepare");
        let row = RowObject::new(vec![Value::Text("bad".into())]);
        let rules = std::sync::Arc::from(rules);
        let paint = paint_for_bound_cell(&rules, 0, "bad", false, &row, &[column("status")]);
        assert_eq!(
            paint,
            Some(HighlightPaint {
                color: "red",
                target: HighlightTarget::Cell,
            })
        );
        assert_eq!(row.cell_value(0), Value::Text("bad".into()));
    }

    #[test]
    fn row_highlight_does_not_match_text_beyond_another_cells_display_limit() {
        let rules = prepare_rules(
            &[HighlightRule {
                column: "detail".into(),
                op: HighlightOp::Contains,
                value: Some(FilterValue::Single("tail-marker".into())),
                color: "red".into(),
                target: HighlightTarget::Row,
            }],
            &["name", "detail"],
        )
        .expect("prepare");
        let row = RowObject::new(vec![
            Value::Text("first".into()),
            Value::Text(format!("{}tail-marker", "a".repeat(512))),
        ]);
        let columns = [column("name"), column("detail")];
        let rules = std::sync::Arc::from(rules);
        let paint = paint_for_bound_cell(&rules, 0, "first", false, &row, &columns);
        assert_eq!(paint, None);
    }
}
