use bookie_app_services::connection_save::{self, ConnectionDraft};
use bookie_core::{AuthMode, Environment, TlsMode};
use gpui::{Context, Div, Entity, Stateful, div, prelude::*, px, rgb};
use secrecy::SecretString;

use crate::GridView;
use crate::text_input::TextInput;
use bookie_gpui::theme::{Theme, size};

pub(crate) struct DriverChoice {
    pub id: &'static str,
    pub label: &'static str,
    pub port: u16,
    pub file_based: bool,
}

pub(crate) struct ConnectionForm {
    pub name: Entity<TextInput>,
    pub host: Entity<TextInput>,
    pub port: Entity<TextInput>,
    pub database: Entity<TextInput>,
    pub username: Entity<TextInput>,
    pub password: Entity<TextInput>,
    pub drivers: Vec<DriverChoice>,
    pub driver: usize,
    pub read_only: bool,
    pub status: String,
    pub saving: bool,
    pub testing: bool,
    pub confirm_delete: bool,
    pub editing: Option<uuid::Uuid>,
    pub kerberos: bool,
    pub tls: u32,
    pub tls_root: Entity<TextInput>,
    pub tls_cert: Entity<TextInput>,
    pub tls_key: Entity<TextInput>,
    pub socket: Entity<TextInput>,
    pub ssh: crate::ssh_form::SshForm,
}

impl ConnectionForm {
    pub fn new(cx: &mut Context<GridView>) -> Self {
        let input = |placeholder: &str, cx: &mut Context<GridView>| cx.new(|cx| TextInput::new(placeholder, cx));
        let mut drivers: Vec<DriverChoice> = drivers_registry::default_registry()
            .iter()
            .map(|driver| DriverChoice {
                id: driver.id(),
                label: driver.display_name(),
                port: driver.default_port(),
                file_based: driver.capabilities().is_file_based,
            })
            .collect();
        drivers.sort_by_key(|driver| driver.label);
        let driver = drivers.iter().position(|driver| driver.id == "postgres").unwrap_or(0);
        let port = input("Port", cx);
        if let Some(driver) = drivers.get(driver).filter(|driver| driver.port != 0) {
            let text = driver.port.to_string();
            port.update(cx, |input, cx| input.set_content(&text, cx));
        }
        Self {
            name: input("Name (optional)", cx),
            host: input("Host", cx),
            port,
            database: input("Database or file", cx),
            username: input("User", cx),
            password: cx.new(|cx| TextInput::new("Password", cx).masked()),
            drivers,
            driver,
            read_only: true,
            status: String::new(),
            saving: false,
            testing: false,
            confirm_delete: false,
            editing: None,
            kerberos: false,
            tls: TlsMode::default().index(),
            tls_root: input("CA certificate file (optional)", cx),
            tls_cert: input("Client certificate file (optional)", cx),
            tls_key: input("Client key file (optional)", cx),
            socket: input("Socket directory or file (optional)", cx),
            ssh: crate::ssh_form::SshForm::new(cx),
        }
    }

    pub fn selected(&self) -> Option<&DriverChoice> {
        self.drivers.get(self.driver)
    }

    fn socket_kind(&self) -> Option<bookie_app_services::connection_form::LocalSocketPathKind> {
        let driver = drivers_registry::default_registry().get(self.selected()?.id)?;
        bookie_app_services::connection_form::local_socket_path_kind(driver.capabilities())
    }

    pub fn draft(&self, cx: &gpui::App) -> ConnectionDraft {
        let text = |input: &Entity<TextInput>| input.read(cx).content().to_string();
        let driver = self.selected();
        ConnectionDraft {
            id: self.editing,
            name: text(&self.name),
            driver_id: driver.map(|driver| driver.id.to_string()).unwrap_or_default(),
            file_based: driver.is_some_and(|driver| driver.file_based),
            host: text(&self.host),
            port: text(&self.port)
                .trim()
                .parse()
                .unwrap_or_else(|_| driver.map_or(0, |driver| driver.port)),
            database: text(&self.database),
            username: text(&self.username),
            password: SecretString::from(text(&self.password)),
            auth_mode: if self.kerberos {
                AuthMode::Kerberos
            } else {
                AuthMode::Password
            },
            tls_mode: TlsMode::from_index(self.tls),
            tls_root_cert: text(&self.tls_root),
            tls_client_cert: text(&self.tls_cert),
            tls_client_key: text(&self.tls_key),
            socket_kind: self.socket_kind(),
            socket_endpoint: text(&self.socket),
            ssh: self.ssh.draft(cx),
            read_only: self.read_only,
            environment: Environment::default(),
            keep_stored_password: self.editing.is_some(),
        }
    }
}

pub(crate) fn field(theme: Theme, label: impl Into<gpui::SharedString>, input: &Entity<TextInput>) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(size::SIDE_PANEL_WIDTH / 2.0))
                .text_color(rgb(theme.muted_foreground))
                .child(gpui::SharedString::from(label.into())),
        )
        .child(div().flex_1().h(px(size::FORM_CONTROL)).child(input.clone()))
}

fn outline_button(id: &'static str, label: &'static str, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(size::SMALL_CONTROL))
        .px_3()
        .rounded(px(theme.radius))
        .border_1()
        .border_color(rgb(theme.border))
        .cursor_pointer()
        .child(label)
}

pub(crate) fn note(theme: Theme, text: &str) -> Div {
    div().text_color(rgb(theme.muted_foreground)).child(text.to_string())
}

pub(crate) fn choice_chip(id: impl Into<gpui::ElementId>, label: &'static str, selected: bool, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(size::SMALL_CONTROL))
        .px_2()
        .rounded(px(theme.radius))
        .cursor_pointer()
        .border_1()
        .border_color(rgb(if selected { theme.foreground } else { theme.border }))
        .when(selected, |chip| chip.bg(rgb(theme.accent)))
        .child(label)
}

pub(crate) fn choice_row(
    theme: Theme,
    label: impl Into<gpui::SharedString>,
    chips: impl IntoIterator<Item = Stateful<Div>>,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(size::SIDE_PANEL_WIDTH / 2.0))
                .text_color(rgb(theme.muted_foreground))
                .child(gpui::SharedString::from(label.into())),
        )
        .child(div().flex().flex_wrap().gap_1().children(chips))
}

impl GridView {
    pub(crate) fn connection_page(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let form = &self.connection_form;
        let file_based = form.selected().is_some_and(|driver| driver.file_based);
        let title = if form.editing.is_some() {
            "Edit connection"
        } else {
            "New connection"
        };
        div().flex_1().min_h_0().flex().flex_col().child(
            div()
                .id("connection-page")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p_4()
                .child(
                    div()
                        .w(px(size::HOME_WIDTH / 2.0))
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(title))
                        .child(div().flex().flex_wrap().gap_1().children(self.driver_chips(theme, cx)))
                        .child(field(theme, "Name", &form.name))
                        .when(!file_based, |page| page.children(self.network_fields(theme, cx)))
                        .child(field(
                            theme,
                            if file_based { "File" } else { "Database" },
                            &form.database,
                        ))
                        .when(!file_based, |page| page.children(self.security_fields(theme, cx)))
                        .when(!file_based, |page| page.children(self.ssh_fields(theme, cx)))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(self.save_button(theme, cx))
                                .child(self.test_button(theme, cx))
                                .children(form.editing.map(|_| self.delete_button(theme, cx)))
                                .child(self.read_only_toggle(theme, cx))
                                .child(div().text_color(rgb(theme.muted_foreground)).child(form.status.clone())),
                        ),
                ),
        )
    }

    fn network_fields(&self, theme: Theme, _cx: &mut Context<Self>) -> Vec<Div> {
        let form = &self.connection_form;
        let mut rows = vec![
            field(theme, "Host", &form.host),
            field(theme, "Port", &form.port),
            field(theme, "User", &form.username),
        ];
        if !form.kerberos {
            rows.push(field(theme, "Password", &form.password));
        }
        rows
    }

    fn security_fields(&self, theme: Theme, cx: &mut Context<Self>) -> Vec<Div> {
        let form = &self.connection_form;
        let auth = choice_row(
            theme,
            "Sign in",
            [("Password", !form.kerberos), ("Kerberos", form.kerberos)]
                .into_iter()
                .enumerate()
                .map(|(index, (label, selected))| {
                    choice_chip(("auth", index), label, selected, theme).on_click(cx.listener(move |this, _, _, cx| {
                        this.connection_form.kerberos = index == 1;
                        cx.notify();
                    }))
                }),
        );
        let tls = choice_row(
            theme,
            "TLS",
            TlsMode::ALL.into_iter().map(|mode| {
                choice_chip(
                    ("tls", mode.index() as usize),
                    mode.label(),
                    form.tls == mode.index(),
                    theme,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.connection_form.tls = mode.index();
                    cx.notify();
                }))
            }),
        );
        let mut rows = vec![auth, tls];
        if TlsMode::from_index(form.tls) != TlsMode::Disabled {
            rows.push(field(theme, "CA file", &form.tls_root));
            rows.push(field(theme, "Client cert", &form.tls_cert));
            rows.push(field(theme, "Client key", &form.tls_key));
        }
        if form.socket_kind().is_some() {
            rows.push(field(theme, "Socket", &form.socket));
        }
        rows
    }

    fn driver_chips(&self, theme: Theme, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let form = &self.connection_form;
        form.drivers
            .iter()
            .enumerate()
            .map(|(index, driver)| {
                let port = driver.port;
                choice_chip(("driver", index), driver.label, index == form.driver, theme).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.connection_form.driver = index;
                        let port_text = if port == 0 { String::new() } else { port.to_string() };
                        this.connection_form
                            .port
                            .update(cx, |input, cx| input.set_content(&port_text, cx));
                        cx.notify();
                    },
                ))
            })
            .collect()
    }

    fn save_button(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let busy = self.connection_form.saving;
        div()
            .id("save-connection")
            .flex()
            .items_center()
            .h(px(size::SMALL_CONTROL))
            .px_3()
            .rounded(px(theme.radius))
            .bg(rgb(theme.foreground))
            .text_color(rgb(theme.background))
            .when(!busy, |button| button.cursor_pointer())
            .child(if busy { "Saving…" } else { "Save" })
            .on_click(cx.listener(|this, _, _, cx| {
                this.save_connection(cx);
            }))
    }

    fn test_button(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let busy = self.connection_form.testing;
        outline_button("test-connection", if busy { "Testing…" } else { "Test" }, theme)
            .on_click(cx.listener(|this, _, _, cx| this.test_connection(cx)))
    }

    fn delete_button(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let confirm = self.connection_form.confirm_delete;
        outline_button("delete-connection", if confirm { "Confirm delete" } else { "Delete" }, theme)
            .text_color(rgb(theme.destructive))
            .on_click(cx.listener(|this, _, _, cx| this.delete_connection(cx)))
    }

    fn read_only_toggle(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let on = self.connection_form.read_only;
        div()
            .id("connection-read-only")
            .flex()
            .items_center()
            .h(px(size::SMALL_CONTROL))
            .px_2()
            .rounded(px(theme.radius))
            .cursor_pointer()
            .border_1()
            .border_color(rgb(theme.border))
            .when(on, |button| button.bg(rgb(theme.accent)))
            .child("Read-only")
            .on_click(cx.listener(|this, _, _, cx| {
                this.connection_form.read_only = !this.connection_form.read_only;
                cx.notify();
            }))
    }

    pub(crate) fn edit_connection(&mut self, id: uuid::Uuid, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let draft = cx
                .background_executor()
                .spawn(async move {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| error.to_string())?;
                    runtime.block_on(connection_save::load_draft(id))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match draft {
                    Ok(Some(draft)) => this.fill_connection_form(draft, cx),
                    Ok(None) => this.connection_form.status = "That connection no longer exists.".into(),
                    Err(message) => this.connection_form.status = message,
                }
                this.tabs.open_new_connection();
                this.sync_grid_owner();
                cx.notify();
            });
        })
        .detach();
    }

    fn fill_connection_form(&mut self, draft: ConnectionDraft, cx: &mut Context<Self>) {
        let form = &mut self.connection_form;
        form.editing = draft.id;
        form.driver = form
            .drivers
            .iter()
            .position(|driver| driver.id == draft.driver_id)
            .unwrap_or(form.driver);
        form.read_only = draft.read_only;
        form.status = String::new();
        form.kerberos = draft.auth_mode == AuthMode::Kerberos;
        form.tls = draft.tls_mode.index();
        form.ssh.fill(draft.ssh.as_ref(), cx);
        let port = draft.port.to_string();
        for (input, text) in [
            (&form.name, draft.name.as_str()),
            (&form.host, draft.host.as_str()),
            (&form.port, port.as_str()),
            (&form.database, draft.database.as_str()),
            (&form.username, draft.username.as_str()),
            (&form.password, ""),
            (&form.tls_root, draft.tls_root_cert.as_str()),
            (&form.tls_cert, draft.tls_client_cert.as_str()),
            (&form.tls_key, draft.tls_client_key.as_str()),
            (&form.socket, draft.socket_endpoint.as_str()),
        ] {
            input.update(cx, |input, cx| input.set_content(text, cx));
        }
    }

    pub(crate) fn test_connection(&mut self, cx: &mut Context<Self>) {
        if self.connection_form.testing {
            return;
        }
        let draft = self.connection_form.draft(cx);
        self.connection_form.testing = true;
        self.connection_form.status = "Testing…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| error.to_string())?;
                    let registry = drivers_registry::default_registry();
                    let audit = bookie_app_services::database_service::DatabaseService::new().transport_audit_factory();
                    runtime.block_on(connection_save::test(
                        &registry,
                        &draft,
                        tokio_util::sync::CancellationToken::new(),
                        Some(&audit),
                    ))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.connection_form.testing = false;
                this.connection_form.status = outcome.unwrap_or_else(|message| message);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn delete_connection(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.connection_form.editing else {
            return;
        };
        if !std::mem::replace(&mut self.connection_form.confirm_delete, true) {
            self.connection_form.status = "Click Delete again to remove this connection and its stored passwords.".into();
            cx.notify();
            return;
        }
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| error.to_string())?;
                    runtime.block_on(connection_save::delete(id))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_delete(outcome);
                cx.notify();
            });
        })
        .detach();
    }

    fn finish_delete(&mut self, outcome: Result<connection_save::Deleted, String>) {
        self.connection_form.confirm_delete = false;
        match outcome {
            Ok(deleted) => {
                self.connection_form.editing = None;
                self.connection_form.status = if deleted.credential_failure {
                    "Deleted; some stored credentials could not be removed.".into()
                } else {
                    "Deleted.".into()
                };
                if let Ok(list) = bookie_gpui::live::saved_connections() {
                    self.connections = list;
                }
            }
            Err(message) => self.connection_form.status = message,
        }
    }

    pub(crate) fn save_connection(&mut self, cx: &mut Context<Self>) {
        if self.connection_form.saving {
            return;
        }
        let draft = self.connection_form.draft(cx);
        if let Err(message) = connection_save::validate(&draft) {
            self.connection_form.status = message;
            cx.notify();
            return;
        }
        self.connection_form.saving = true;
        self.connection_form.status = "Saving…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| error.to_string())?;
                    runtime.block_on(connection_save::save(draft))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.connection_form.saving = false;
                match result {
                    Ok(saved) => {
                        this.connection_form.status = format!("Saved {}", saved.name);
                        this.connection_form
                            .password
                            .update(cx, |input, cx| input.set_content("", cx));
                        if let Ok(list) = bookie_gpui::live::saved_connections() {
                            this.connections = list;
                        }
                    }
                    Err(message) => this.connection_form.status = message,
                }
                cx.notify();
            });
        })
        .detach();
    }
}
