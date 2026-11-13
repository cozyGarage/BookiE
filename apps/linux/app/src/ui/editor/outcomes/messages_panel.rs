use bookie_app_services::server_messages_view::{MessageRow, MessagesView};
use bookie_core::{MessageSeverity, ServerMessages};
use relm4::gtk;
use relm4::gtk::prelude::*;

const MAX_PANEL_HEIGHT: i32 = 180;

pub(super) fn attach(content: gtk::Widget, messages: &ServerMessages) -> gtk::Widget {
    let view = MessagesView::new(messages);
    if view.is_empty() {
        return content;
    }
    let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    holder.append(&content);
    holder.append(&build_panel(&view));
    holder.upcast()
}

fn build_panel(view: &MessagesView) -> gtk::Expander {
    let expander = gtk::Expander::builder()
        .label(view.title())
        .expanded(true)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    expander.add_css_class("server-messages");
    let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
    body.append(&copy_button(view));
    body.append(&scrolled_rows(view));
    expander.set_child(Some(&body));
    expander
}

fn copy_button(view: &MessagesView) -> gtk::Button {
    let label = bookie_app_services::tr!("Copy messages");
    let button = gtk::Button::builder()
        .icon_name("edit-copy-symbolic")
        .tooltip_text(&label)
        .halign(gtk::Align::End)
        .build();
    button.update_property(&[gtk::accessible::Property::Label(&label)]);
    let text = view.copy_text();
    button.connect_clicked(move |button| button.clipboard().set_text(&text));
    button
}

fn scrolled_rows(view: &MessagesView) -> gtk::ScrolledWindow {
    let rows = gtk::Box::new(gtk::Orientation::Vertical, 2);
    for row in view.rows() {
        rows.append(&row_widget(row));
    }
    if let Some(notice) = view.omitted_notice() {
        let label = gtk::Label::builder().label(notice).halign(gtk::Align::Start).build();
        label.add_css_class("bookie-output-font");
        label.add_css_class("dim-label");
        rows.append(&label);
    }
    gtk::ScrolledWindow::builder()
        .child(&rows)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(MAX_PANEL_HEIGHT)
        .build()
}

fn row_widget(row: &MessageRow) -> gtk::Box {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let severity = gtk::Label::builder()
        .label(&row.severity_label)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Start)
        .build();
    severity.add_css_class("bookie-output-font");
    if let Some(class) = severity_class(row.severity) {
        severity.add_css_class(class);
    }
    line.append(&severity);
    if let Some(code) = &row.code {
        let code = gtk::Label::builder()
            .label(code)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .build();
        code.add_css_class("bookie-output-font");
        code.add_css_class("dim-label");
        code.add_css_class("monospace");
        line.append(&code);
    }
    let text = gtk::Label::builder()
        .label(&row.text)
        .halign(gtk::Align::Start)
        .hexpand(true)
        .wrap(true)
        .xalign(0.0)
        .selectable(true)
        .build();
    text.add_css_class("bookie-output-font");
    line.append(&text);
    line
}

fn severity_class(severity: MessageSeverity) -> Option<&'static str> {
    match severity {
        MessageSeverity::Warning => Some("warning"),
        MessageSeverity::Error => Some("error"),
        MessageSeverity::Debug | MessageSeverity::Info | MessageSeverity::Notice => None,
    }
}

#[cfg(test)]
mod tests {
    use bookie_core::{ServerMessage, ServerMessages};
    use relm4::gtk;
    use relm4::gtk::prelude::*;

    use super::{attach, severity_class};
    use bookie_core::MessageSeverity;

    #[test]
    fn warnings_and_errors_get_a_style_class_and_notices_do_not() {
        assert_eq!(severity_class(MessageSeverity::Warning), Some("warning"));
        assert_eq!(severity_class(MessageSeverity::Error), Some("error"));
        assert_eq!(severity_class(MessageSeverity::Notice), None);
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn the_panel_is_added_only_when_the_statement_produced_messages() {
        gtk::init().unwrap();
        let plain = gtk::Label::new(Some("grid"));
        let untouched = attach(plain.clone().upcast(), &ServerMessages::default());
        assert!(untouched == plain.upcast::<gtk::Widget>());
        let mut messages = ServerMessages::default();
        messages.push(ServerMessage::new(MessageSeverity::Notice, None, "hello"));
        let wrapped = attach(gtk::Label::new(Some("grid")).upcast(), &messages);
        assert!(
            wrapped
                .downcast_ref::<gtk::Box>()
                .is_some_and(|holder| holder.last_child().is_some())
        );
    }
}
