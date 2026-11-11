use gtk::prelude::*;
use relm4::{ComponentSender, gtk};

use super::{App, AppMsg};
use crate::services::open_request::{LaunchArgument, OpenRejection, OpenablePath, classify_openable, file_connection};
use crate::ui::editor::open_file::read_sql_file;

pub(super) fn forward_open_signal(sender: &relm4::Sender<AppMsg>) {
    let sender = sender.clone();
    relm4::main_application().connect_open(move |app, files, _hint| {
        let arguments = files
            .iter()
            .map(|file| match file.path() {
                Some(path) => LaunchArgument::Path(path),
                None => LaunchArgument::Uri(file.uri().to_string()),
            })
            .collect();
        let _ = sender.send(AppMsg::OpenLaunchArguments(arguments));
        if let Some(window) = app.active_window() {
            window.present();
        }
    });
}

impl App {
    pub(super) fn on_open_launch_arguments(&mut self, arguments: Vec<LaunchArgument>, sender: ComponentSender<Self>) {
        let mut arguments = arguments.into_iter();
        let Some(first) = arguments.next() else { return };
        if arguments.next().is_some() {
            self.show_toast(&bookie_app_services::tr!("BookiE opens one file at a time. Opening the first."));
        }
        match first {
            LaunchArgument::Uri(uri) => self.present_connection_url_dialog(sender, Some(&uri)),
            LaunchArgument::Path(path) => match classify_openable(&path) {
                Ok(OpenablePath::Database(file)) => self.open_database_file(file, sender),
                Ok(OpenablePath::SqlScript(path)) => Self::open_sql_script_path(path, sender),
                Err(OpenRejection::Missing(path)) => self
                    .show_toast(&bookie_app_services::tr!("{path} does not exist.").replace("{path}", &path.display().to_string())),
                Err(OpenRejection::NotADatabase(path)) => self.show_toast(
                    &bookie_app_services::tr!("{path} is not a SQLite or DuckDB database file.")
                        .replace("{path}", &path.display().to_string()),
                ),
                Err(OpenRejection::NonUtf8Path(_)) => self.show_toast(&bookie_app_services::tr!(
                    "BookiE cannot open this path because it contains non-UTF-8 bytes."
                )),
            },
        }
    }

    fn open_sql_script_path(path: std::path::PathBuf, sender: ComponentSender<Self>) {
        std::thread::spawn(move || {
            sender.input(match read_sql_file(&path) {
                Ok(file) => AppMsg::SqlFileOpened(file),
                Err(message) => AppMsg::SqlFileFailed(message),
            });
        });
    }

    fn open_database_file(&self, file: crate::services::open_request::DatabaseFile, sender: ComponentSender<Self>) {
        if self.registry.get(file.driver_id()).is_none() {
            self.show_toast(
                &bookie_app_services::tr!("This BookiE build has no {driver} driver.").replace("{driver}", file.driver_id()),
            );
            return;
        }
        let reply = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let database = file.database_path().to_owned();
                    let driver_id = file.driver_id();
                    let existing = bookie_storage::load_connections().await.unwrap_or_default();
                    let saved = match existing
                        .into_iter()
                        .find(|saved| saved.driver_id == driver_id && saved.database == database)
                    {
                        Some(saved) => saved,
                        None => {
                            let saved = file_connection(&file);
                            if let Err(error) = crate::services::connection_save_turn::run(
                                bookie_storage::save_connections(std::slice::from_ref(&saved)),
                            )
                            .await
                            {
                                tracing::warn!(%error, "saving the opened database file failed");
                                reply.input(AppMsg::ImportConnectionUrlFailed);
                                return;
                            }
                            reply.input(AppMsg::ReloadConnections);
                            saved
                        }
                    };
                    reply.input(AppMsg::OpenSaved(saved));
                })
                .drop_on_shutdown()
        });
    }
}
