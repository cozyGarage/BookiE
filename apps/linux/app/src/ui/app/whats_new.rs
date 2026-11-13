use bookie_app_services::release_updates::{self, LatestRelease, UpdateCheckOutcome, WhatsNewDecision};
use gtk4::prelude::*;
use relm4::ComponentSender;
use relm4::adw;
use relm4::adw::prelude::*;

use super::{App, AppMsg, UpdateCheckSource};
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

pub(super) fn start_update_check(app: &App, source: UpdateCheckSource, sender: ComponentSender<App>) {
    if source == UpdateCheckSource::Manual {
        app.show_toast(&bookie_app_services::tr!("Checking for updates…"));
    }
    let running = crate::config::VERSION.to_owned();
    relm4::spawn(async move {
        let result = tokio::task::spawn_blocking(move || update_check::check_for_updates(&running))
            .await
            .unwrap_or(Err(update_check::UpdateCheckError::Network));
        sender.input(AppMsg::UpdateCheckFinished { source, result });
    });
}

pub(super) fn present_update_check_result(
    app: &mut App,
    source: UpdateCheckSource,
    result: update_check::UpdateCheckResult,
    sender: ComponentSender<App>,
) {
    match result {
        Ok(UpdateCheckOutcome::UpToDate { latest }) => {
            app.clear_update_banner();
            if source == UpdateCheckSource::Manual {
                let message = bookie_app_services::tr!("BookiE is up to date ({version}).")
                    .replace("{version}", &latest.version.display());
                app.show_toast(&message);
            }
        }
        Ok(UpdateCheckOutcome::UpdateAvailable { latest }) => {
            let show_banner = release_updates::should_show_update_banner(
                crate::config::VERSION,
                &latest,
                launch_version::dismissed_update_version().as_deref(),
            );
            if show_banner {
                app.reveal_update_banner(&latest);
            } else {
                app.clear_update_banner();
            }
            if source == UpdateCheckSource::Manual {
                present_update_dialog(app, &latest, sender, false);
            }
        }
        Ok(UpdateCheckOutcome::InvalidLatest) | Err(update_check::UpdateCheckError::InvalidResponse) => {
            if source == UpdateCheckSource::Manual {
                app.show_toast(&bookie_app_services::tr!(
                    "Could not read the latest release information."
                ));
            }
        }
        Err(update_check::UpdateCheckError::Network) => {
            if source == UpdateCheckSource::Manual {
                app.show_toast(&bookie_app_services::tr!(
                    "Could not check for updates. Check your network connection."
                ));
            }
        }
    }
}

pub(super) fn review_available_update(app: &App, sender: ComponentSender<App>) {
    let Some(latest) = app.available_update.clone() else {
        return;
    };
    present_update_dialog(app, &latest, sender, true);
}

pub(super) fn dismiss_available_update(app: &mut App) {
    let Some(latest) = app.available_update.clone() else {
        app.clear_update_banner();
        return;
    };
    let _ = launch_version::store_dismissed_update_version(&latest.version.display());
    app.clear_update_banner();
}

fn present_update_dialog(app: &App, latest: &LatestRelease, sender: ComponentSender<App>, dismiss_on_not_now: bool) {
    let version = latest.version.display();
    let body = bookie_app_services::tr!("BookiE {version} is available. Open the release page to download it.")
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
        } else if dismiss_on_not_now {
            sender.input(AppMsg::DismissAvailableUpdate);
        }
    });
    dialog.present(Some(&app.window));
}

fn open_uri(parent: &impl IsA<gtk4::Window>, uri: &str) {
    let launcher = gtk4::UriLauncher::new(uri);
    launcher.launch(Some(parent), gtk4::gio::Cancellable::NONE, |_| {});
}

impl App {
    pub(super) fn reveal_update_banner(&mut self, latest: &bookie_app_services::release_updates::LatestRelease) {
        let title =
            bookie_app_services::tr!("BookiE {version} is available").replace("{version}", &latest.version.display());
        self.update_banner.set_title(&title);
        self.update_banner.set_revealed(true);
        self.available_update = Some(latest.clone());
    }

    pub(super) fn clear_update_banner(&mut self) {
        self.update_banner.set_revealed(false);
        self.available_update = None;
    }
}
