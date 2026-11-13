use relm4::adw::prelude::*;
use relm4::{Component, ComponentController, ComponentSender, adw, gtk};

use crate::ui::history_dialog::{HistoryDialog, HistoryDialogInit, HistoryDialogOutput};

use super::{App, AppMsg, shortcuts};

const ABOUT_DEVELOPERS: &[&str] = &["BookiE contributors https://github.com/cozyGarage/BookiE"];

impl App {
    pub(super) fn show_welcome_page(&self, _sender: ComponentSender<Self>) {
        // Welcome stays outside the ViewStack; disconnect hides the ViewSwitcherBar.
        self.show_in_content_holder(self.welcome_view.widget());
    }

    pub(super) fn show_in_content_holder(&self, widget: &impl IsA<gtk::Widget>) {
        if self.content_holder.content().as_ref() != Some(self.history_paned.upcast_ref()) {
            self.content_holder.set_content(Some(&self.history_paned));
        }
        if self.history_paned.start_child().as_ref() != Some(widget.upcast_ref()) {
            self.history_paned.set_start_child(Some(widget));
        }
    }

    /// Keep a persistent connect toast until remote and SSH connection attempts resolve.
    pub(super) fn set_loading_page(&mut self, title: &str, description: &str, sender: &ComponentSender<Self>) {
        if let Some(prev) = self.connect_progress_toast.take() {
            prev.dismiss();
        }
        let body = if description.is_empty() {
            title.to_string()
        } else {
            format!("{title} · {description}")
        };
        let toast = crate::ui::plain_text::toast(&body);
        toast.set_timeout(0);
        toast.set_button_label(Some(&bookie_app_services::tr!("Cancel")));
        let cancel_sender = sender.clone();
        toast.connect_button_clicked(move |_| cancel_sender.input(AppMsg::CancelConnect));
        self.toast_overlay.add_toast(toast.clone());
        self.connect_progress_toast = Some(toast);
    }

    pub(super) fn dismiss_loading_page(&mut self) {
        if let Some(toast) = self.connect_progress_toast.take() {
            toast.dismiss();
        }
    }

    /// Surface app-level failures in an alert; tab-scoped failures use BrowseTab.
    pub(super) fn set_status_page(&self, _kind: super::StatusKind, title: &str, description: &str) {
        self.show_error_alert(title, description);
    }

    pub(super) fn show_toast(&self, msg: &str) {
        self.toast_overlay.add_toast(crate::ui::plain_text::toast(msg));
    }

    pub(super) fn show_error_alert(&self, title: &str, message: &str) {
        let dialog = adw::AlertDialog::new(Some(title), Some(message));
        // "Close" avoids implying acknowledgement of a completed action.
        dialog.add_response("close", &bookie_app_services::tr!("Close"));
        dialog.set_default_response(Some("close"));
        dialog.set_close_response("close");
        dialog.present(Some(&self.window));
    }

    pub(super) fn on_show_history(&mut self, sender: ComponentSender<Self>) {
        if self.history_paned.end_child().is_some() {
            self.hide_history_panel();
            return;
        }
        let Some(history) = self.history.clone() else {
            self.show_toast(&bookie_app_services::tr!("Query history is unavailable"));
            return;
        };
        let panel = HistoryDialog::builder()
            .launch(HistoryDialogInit {
                history,
                database: self.database.clone(),
            })
            .forward(sender.input_sender(), |out| match out {
                HistoryDialogOutput::OpenInNewTab(text) => AppMsg::OpenHistoryQuery(text),
                HistoryDialogOutput::ReplaceCurrentTabQuery(text) => AppMsg::ReplaceActiveTabQuery(text),
                HistoryDialogOutput::Close => AppMsg::CloseHistory,
            });
        crate::ui::leak_probe::track("history_panel", panel.widget());
        self.history_paned.set_end_child(Some(panel.widget()));
        self.history_paned.set_position(self.window.default_height() / 2);
        self.history_dialog = Some(panel);
    }

    pub(super) fn hide_history_panel(&mut self) {
        self.history_paned.set_end_child(None::<&gtk::Widget>);
        self.history_dialog = None;
    }

    pub(super) fn on_show_shortcuts(&self) {
        let shortcuts = shortcuts::build_shortcuts_dialog();
        crate::ui::leak_probe::track("shortcuts", &shortcuts);
        shortcuts.present(Some(&self.window));
    }

    pub(super) fn on_show_about(&self) {
        let mut builder = adw::AboutDialog::builder()
            .application_name(bookie_app_services::tr!("BookiE"))
            .application_icon(crate::config::APP_ID)
            .developer_name(bookie_app_services::tr!("BookiE contributors"))
            .version(crate::config::VERSION)
            .website("https://github.com/cozyGarage/BookiE")
            .issue_url("https://github.com/cozyGarage/BookiE/issues")
            .support_url("https://github.com/cozyGarage/BookiE/discussions")
            .copyright(bookie_app_services::tr!("© 2025–2026 BookiE contributors"))
            .license_type(gtk::License::Agpl30)
            .comments(bookie_app_services::tr!(
                "A native Linux database client built with GTK4 + libadwaita."
            ));
        if let Some((version, notes)) = super::whats_new::running_release_notes() {
            builder = builder.release_notes_version(version).release_notes(notes);
        }
        let dialog = builder.build();
        dialog.set_developers(ABOUT_DEVELOPERS);
        dialog.set_translator_credits(&bookie_app_services::tr!("translator-credits"));
        dialog.add_link(
            &bookie_app_services::tr!("Releases"),
            "https://github.com/cozyGarage/BookiE/releases",
        );
        crate::ui::leak_probe::track("about", &dialog);
        dialog.present(Some(&self.window));
    }

    pub(super) fn on_explain_active_query(&self) {
        let Some(id) = self.selected_workspace_tab_id() else {
            self.show_toast(&bookie_app_services::tr!("Open an SQL editor tab to explain a query."));
            return;
        };
        let tabs = self.workspace_tabs.borrow();
        let Some(super::WorkspaceTab::Editor(slot)) = tabs.get(&id) else {
            self.show_toast(&bookie_app_services::tr!("Open an SQL editor tab to explain a query."));
            return;
        };
        let sql = slot.query.clone();
        drop(tabs);
        crate::ui::explain_dialog::present(
            &self.window,
            self.resolve_tab_connection(id).ok(),
            &sql,
            &self.database,
            &self.preferences,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::ABOUT_DEVELOPERS;

    #[test]
    fn about_credits_name_only_the_bookie_contributors() {
        assert_eq!(ABOUT_DEVELOPERS.len(), 1);
        assert!(ABOUT_DEVELOPERS[0].starts_with("BookiE contributors "));
        assert!(ABOUT_DEVELOPERS[0].contains("github.com/cozyGarage/BookiE"));
    }
}
