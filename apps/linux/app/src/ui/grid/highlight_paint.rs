use bookie_app_services::highlight_match::{
    DisplayCell, HighlightPaint, HighlightTarget, PreparedHighlightRule, paint_for_cell,
};
use bookie_core::Value;
use gtk4::prelude::*;

use super::display::value_to_display_text;
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
    rules: &[PreparedHighlightRule],
    column: usize,
    shown_text: &str,
    shown_null: bool,
    row: &RowObject,
) -> Option<HighlightPaint> {
    if rules.is_empty() {
        return None;
    }
    let cell = DisplayCell {
        text: shown_text,
        is_null: shown_null,
    };
    paint_for_cell(rules, column, cell, |idx| {
        let value = row.cell_value_in_row(idx)?;
        let is_null = matches!(value, Value::Null);
        Some((value_to_display_text(&value), is_null))
    })
}

#[cfg(test)]
mod tests {
    use bookie_app_services::highlight_match::{HighlightOp, HighlightRule, HighlightTarget, prepare_rules};
    use bookie_core::{FilterValue, Value};

    use super::*;

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
        let paint = paint_for_bound_cell(&rules, 0, "bad", false, &row);
        assert_eq!(
            paint,
            Some(HighlightPaint {
                color: "red",
                target: HighlightTarget::Cell,
            })
        );
        assert_eq!(row.cell_value(0), Value::Text("bad".into()));
    }
}
