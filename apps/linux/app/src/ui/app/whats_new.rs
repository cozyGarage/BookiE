use bookie_app_services::release_updates::{self, UpdateCheckOutcome, WhatsNewDecision};
use gtk4::prelude::*;
use relm4::adw;
use relm4::adw::prelude::*;
use relm4::ComponentSender;

use super::{App, AppMsg};
use crate::services::{launch_version, update_check};

const METAINFO: &str = include_str!("../../../../flatpak/com.bookie.linux.metainfo.xml");

pub(super) fn running_release_notes() -> Option<(String, String)> {
    let version = crate::config::VERSION.to_owned();
    let notes = release_updates::metainfo_release_notes(METAINFO, &version)?;
    Some((version, notes))
}

pub(super) fn maybe_show_whats_new(app: &App, sender: ComponentSender<App>) {
    let running = crate::config::VERSION;
    let stored = launch_version::last_launched_version();
    match release_updates::should_show_whats_new(stored.as_deref(), running) {
        WhatsNewDecision::Show => present_whats_new(app, running, sender),
        WhatsNewDecision::SkipAndRemember => {
            let _ = launch_version::store_last_launched_version(running);
        }
        WhatsNewDecision::SkipKeepStored => {}
    }
}

fn present_whats_new(app: &App, running: &str, sender: ComponentSender<App>) {
    let notes = release_updates::metainfo_release_notes(METAINFO, running)
        .unwrap_or_else(|| format!("<p>BookiE {running}</p>"));
    let heading = bookie_app_services::tr!("What's new in BookiE {version}").replace("{version}", running);
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body_use_markup(true)
        .body(notes)
        .default_response("close")
        .close_response("close")
        .build();
    dialog.add_response("close", &bookie_app_services::tr!("Close"));
    let version = running.to_owned();
    dialog.connect_response(None, move |_, _| {
        let _ = launch_version::store_last_launched_version(&version);
        sender.input(AppMsg::WhatsNewDismissed);
    });
    dialog.present(Some(&app.window));
}

pub(super) fn start_update_check(app: &App, sender: ComponentSender<App>) {
    app.show_toast(&bookie_app_services::tr!("Checking for updates…"));
    let running = crate::config::VERSION.to_owned();
    relm4::spawn(async move {
        let result = tokio::task::spawn_blocking(move || update_check::check_for_updates(&running))
            .await
            .unwrap_or(Err(update_check::UpdateCheckError::Network));
        sender.input(AppMsg::UpdateCheckFinished(result));
    });
}

pub(super) fn present_update_check_result(app: &App, result: update_check::UpdateCheckResult) {
    match result {
        Ok(UpdateCheckOutcome::UpToDate { latest }) => {
            let message = bookie_app_services::tr!("BookiE is up to date ({version}).")
                .replace("{version}", &latest.version.display());
            app.show_toast(&message);
        }
        Ok(UpdateCheckOutcome::UpdateAvailable { latest }) => {
            let version = latest.version.display();
            let body = bookie_app_services::tr!(
                "BookiE {version} is available. Open the release page to download it."
            )
            .replace("{version}", &version);
            let dialog = adw::AlertDialog::builder()
                .heading(bookie_app_services::tr!("Update available"))
                .body(body)
                .default_response("open")
                .close_response("close")
                .build();
            dialog.add_response("close", &bookie_app_services::tr!("Not now"));
            dialog.add_response("open", &bookie_app_services::tr!("Open release"));
            dialog.set_response_appearance("open", adw::ResponseAppearance::Suggested);
            let url = latest.html_url.clone();
            let window = app.window.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "open" {
                    open_uri(&window, &url);
                }
            });
            dialog.present(Some(&app.window));
        }
        Ok(UpdateCheckOutcome::InvalidLatest) | Err(update_check::UpdateCheckError::InvalidResponse) => {
            app.show_toast(&bookie_app_services::tr!(
                "Could not read the latest release information."
            ));
        }
        Err(update_check::UpdateCheckError::Network) => {
            app.show_toast(&bookie_app_services::tr!(
                "Could not check for updates. Check your network connection."
            ));
        }
    }
}

fn open_uri(parent: &impl IsA<gtk4::Window>, uri: &str) {
    let launcher = gtk4::UriLauncher::new(uri);
    launcher.launch(Some(parent), gtk4::gio::Cancellable::NONE, |_| {});
}
