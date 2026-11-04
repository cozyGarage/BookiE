use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use tokio_util::sync::CancellationToken;

pub(crate) fn owner_id(window: &impl IsA<gtk::Window>) -> u64 {
    window.as_ptr() as usize as u64
}

pub(crate) struct CancellableDialog {
    dialog: adw::Dialog,
    cancel: CancellationToken,
    _busy: bookie_app_services::busy_jobs::BusyJob,
}

impl CancellableDialog {
    pub(crate) fn present(parent: &adw::ApplicationWindow, title: &str, body: &[&gtk::Widget]) -> Self {
        let cancel = CancellationToken::new();
        let stop = gtk::Button::with_label(&crate::tr!("Cancel"));
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        content.update_property(&[gtk::accessible::Property::Label(title)]);
        let heading = gtk::Label::builder()
            .label(title)
            .wrap(true)
            .css_classes(["title-3"])
            .build();
        content.append(&heading);
        for widget in body {
            content.append(*widget);
        }
        content.append(&stop);
        let dialog = adw::Dialog::builder()
            .title(title)
            .content_width(400)
            .child(&content)
            .can_close(false)
            .build();
        stop.connect_clicked({
            let cancel = cancel.clone();
            move |button| {
                cancel.cancel();
                button.set_sensitive(false);
            }
        });
        dialog.present(Some(parent));
        let busy = bookie_app_services::busy_jobs::global().begin_owned(owner_id(parent), cancel.clone());
        Self {
            dialog,
            cancel,
            _busy: busy,
        }
    }

    pub(crate) fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    #[cfg(test)]
    pub(crate) fn dialog(&self) -> &adw::Dialog {
        &self.dialog
    }

    pub(crate) fn close(&self) {
        self.dialog.force_close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn progress_dialog_displays_and_names_its_operation() {
        gtk::init().unwrap();
        adw::init().unwrap();
        let application = gtk::Application::new(None::<&str>, gtk::gio::ApplicationFlags::NON_UNIQUE);
        application.register(None::<&gtk::gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&application);
        window.present();
        for title in ["Backing up example", "Restoring example", "Importing into example"] {
            let spinner = gtk::Spinner::new();
            let progress = CancellableDialog::present(&window, title, &[spinner.upcast_ref()]);
            let dialog = progress.dialog();
            let content = dialog.child().unwrap();
            let first = content.first_child().unwrap();
            let heading = first.downcast::<gtk::Label>().expect("visible operation title");
            assert_eq!(heading.label(), title);
            assert!(gtk::test_accessible_has_property(
                &content,
                gtk::AccessibleProperty::Label
            ));
            progress.close();
        }
        window.close();
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn a_progress_dialog_counts_as_running_work_until_it_is_dropped() {
        gtk::init().unwrap();
        adw::init().unwrap();
        let application = gtk::Application::new(None::<&str>, gtk::gio::ApplicationFlags::NON_UNIQUE);
        application.register(None::<&gtk::gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&application);
        window.present();
        let jobs = bookie_app_services::busy_jobs::global();
        assert_eq!(jobs.active(), 0);
        let progress = CancellableDialog::present(&window, "Importing into example", &[]);
        assert_eq!(jobs.active(), 1);
        let token = progress.cancel_token();
        jobs.cancel_all();
        assert!(token.is_cancelled());
        progress.close();
        drop(progress);
        assert_eq!(jobs.active(), 0);
        window.close();
    }
}
