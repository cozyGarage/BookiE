use crate::ui::spacing;
use bookie_app_services::explain_plan::PlanNode;
use relm4::{gtk, gtk::prelude::*};

pub(crate) fn build_tree(nodes: &[PlanNode]) -> gtk::Widget {
    let list = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(spacing::SM)
        .margin_top(spacing::MD)
        .margin_bottom(spacing::MD)
        .margin_start(spacing::MD)
        .margin_end(spacing::MD)
        .build();
    let max_cost = nodes.iter().filter_map(|node| node.cost).fold(0.0_f64, f64::max);
    for node in nodes {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(spacing::OPTICAL)
            .margin_start(node.depth as i32 * 18)
            .build();
        let title = gtk::Label::builder()
            .label(&node.title)
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .build();
        row.append(&title);
        if let Some(cost) = node.cost
            && max_cost > 0.0
        {
            let bar = gtk::ProgressBar::builder()
                .fraction((cost / max_cost).clamp(0.0, 1.0))
                .build();
            let rows = node.rows.map(|rows| format!(", ~{rows:.0} rows")).unwrap_or_default();
            bar.set_tooltip_text(Some(&format!(
                "{} {cost:.2}{rows}",
                bookie_app_services::tr!("Estimated cost")
            )));
            row.append(&bar);
        }
        for detail in &node.details {
            let label = gtk::Label::builder()
                .label(detail)
                .xalign(0.0)
                .wrap(true)
                .selectable(true)
                .build();
            label.add_css_class("dim-label");
            label.add_css_class("caption");
            row.append(&label);
        }
        list.append(&row);
    }
    list.upcast()
}
