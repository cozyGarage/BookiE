use bookie_app_services::connection_save::{self, ConnectionDraft};
use bookie_app_services::service_runtime::block_on;
use bookie_core::{AuthMode, DriverCapabilities, Environment, TlsMode};
use gpui::{Context, Div, Entity, Focusable, Stateful, Window, div, prelude::*, px, rgb};
use secrecy::SecretString;

use crate::GridView;
use crate::surfaces::{ButtonKind, button, section_header, segment, segmented, unavailable_button};
use crate::text_input::TextInput;
use bookie_gpui::theme::{Theme, size};

pub(crate) struct DriverChoice {
    pub id: &'static str,
    pub label: &'static str,
    pub port: u16,
    pub capabilities: DriverCapabilities,
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
    pub environment: Environment,
    pub color: Option<String>,
    pub status: String,
    validation_error: Option<(gpui::EntityId, String)>,
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
    pub security_open: bool,
    pub ssh_open: bool,
    pub open: bool,
    pub focus_on_open: bool,
    pub focus_validation: Option<gpui::FocusHandle>,
    pub restore_focus: bool,
}

impl ConnectionForm {
    pub fn new(cx: &mut Context<GridView>) -> Self {
        let input = |placeholder: &str, cx: &mut Context<GridView>| {
            let input = cx.new(|cx| TextInput::new(placeholder, cx));
            cx.subscribe(&input, |this, _, event: &crate::text_input::InputEvent, cx| {
                if matches!(event, crate::text_input::InputEvent::Changed) {
                    this.clear_resolved_connection_error(cx);
                    cx.notify();
                }
            })
            .detach();
            input
        };
        let mut drivers: Vec<DriverChoice> = drivers_registry::default_registry()
            .iter()
            .map(|driver| DriverChoice {
                id: driver.id(),
                label: driver.display_name(),
                port: driver.default_port(),
                capabilities: driver.capabilities(),
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
            environment: Environment::default(),
            color: None,
            status: String::new(),
            validation_error: None,
            saving: false,
            testing: false,
            confirm_delete: false,
            editing: None,
            kerberos: false,
            tls: TlsMode::default().index(),
            security_open: false,
            ssh_open: false,
            open: false,
            focus_on_open: false,
            focus_validation: None,
            restore_focus: false,
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

    pub(crate) fn validation_field(
        &self,
        theme: Theme,
        label: impl Into<gpui::SharedString>,
        input: &Entity<TextInput>,
        cx: &gpui::App,
    ) -> Div {
        let error = self.validation_error.as_ref().filter(|(id, message)| {
            *id == input.entity_id()
                && connection_save::validate(&self.draft(cx))
                    .map_err(|error| error.to_string())
                    .as_ref()
                    .err()
                    == Some(message)
        });
        div()
            .flex()
            .flex_col()
            .gap_1()
            .when(error.is_some(), |row| {
                row.border_1().border_color(rgb(theme.destructive))
            })
            .child(field(theme, label, input))
            .children(error.map(|(_, message)| div().text_color(rgb(theme.destructive)).child(message.clone())))
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
            capabilities: driver.map_or_else(DriverCapabilities::default, |driver| driver.capabilities),
            host: text(&self.host),
            port: text(&self.port).trim().parse().unwrap_or(0),
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
            environment: self.environment,
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

pub(crate) fn section(theme: Theme, caption: &str, rows: Vec<Div>) -> Div {
    let last = rows.len().saturating_sub(1);
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(section_header(caption.to_string(), theme))
        .child(
            div()
                .flex()
                .flex_col()
                .rounded(px(size::SURFACE_RADIUS))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.background))
                .children(rows.into_iter().enumerate().map(|(index, row)| {
                    let row = row.px_3().py_2();
                    if index < last {
                        row.border_b_1().border_color(rgb(theme.border))
                    } else {
                        row
                    }
                })),
        )
}

pub(crate) fn collapsible_section(
    theme: Theme,
    id: &'static str,
    caption: &str,
    summary: String,
    rows: Option<Vec<Div>>,
    on_toggle: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> Div {
    let open = rows.is_some();
    let header = div()
        .id(format!("{id}-toggle"))
        .debug_selector(move || format!("{id}-toggle"))
        .flex()
        .items_center()
        .gap_1()
        .cursor_pointer()
        .text_size(px(size::TEXT_SMALL))
        .text_color(rgb(theme.muted_foreground))
        .child(crate::assets::icon(
            if open { "chevron-down" } else { "chevron-right" },
            theme.muted_foreground,
        ))
        .child(div().font_weight(gpui::FontWeight::MEDIUM).child(caption.to_string()))
        .child(div().child(format!("· {summary}")))
        .on_click(on_toggle);
    let body = rows.map(|rows| {
        let last = rows.len().saturating_sub(1);
        div()
            .debug_selector(move || format!("{id}-fields"))
            .flex()
            .flex_col()
            .rounded(px(size::SURFACE_RADIUS))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.background))
            .children(rows.into_iter().enumerate().map(|(index, row)| {
                let row = row.px_3().py_2();
                if index < last {
                    row.border_b_1().border_color(rgb(theme.border))
                } else {
                    row
                }
            }))
    });
    div().flex().flex_col().gap_1().child(header).children(body)
}

pub(crate) fn labelled_row(theme: Theme, label: impl Into<gpui::SharedString>, control: impl IntoElement) -> Div {
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
        .child(div().flex_1().min_w_0().flex().child(control))
}

pub(crate) fn choice_row(
    theme: Theme,
    label: impl Into<gpui::SharedString>,
    choices: impl IntoIterator<Item = Stateful<Div>>,
) -> Div {
    labelled_row(
        theme,
        label,
        segmented(choices, theme).flex_shrink(1.0).flex_wrap().h_auto(),
    )
}

impl GridView {
    pub(crate) fn open_connection_dialog(&mut self, cx: &mut Context<Self>) {
        self.connection_form.open = true;
        self.connection_form.focus_on_open = true;
        cx.notify();
    }

    pub(crate) fn close_connection_dialog(&mut self, cx: &mut Context<Self>) {
        if self.connection_form.saving {
            return;
        }
        self.connection_form.open = false;
        self.connection_form.confirm_delete = false;
        self.connection_form.focus_on_open = false;
        self.connection_form.restore_focus = true;
        cx.notify();
    }

    pub(crate) fn connection_dialog_title(&self, cx: &gpui::App) -> String {
        if self.connection_form.editing.is_none() {
            return "New connection".into();
        }
        let name = self.connection_form.name.read(cx).content().trim().to_string();
        if name.is_empty() {
            "Edit connection".into()
        } else {
            format!("Edit {name}")
        }
    }

    pub(crate) fn connection_dialog(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        crate::surfaces::scrim(theme)
            .id("connection-dialog-scrim")
            .occlude()
            .p_4()
            .child(self.connection_card(theme, cx))
    }

    fn connection_card(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let form = &self.connection_form;
        let file_based = form.selected().is_some_and(|driver| driver.capabilities.is_file_based);
        let title = self.connection_dialog_title(cx);
        let driver = form
            .selected()
            .map(|driver| driver.label.to_string())
            .unwrap_or_default();
        let mut connection = vec![
            segmented(self.driver_chips(theme, cx), theme).flex_wrap().h_auto(),
            field(theme, "Name", &form.name),
        ];
        if !file_based {
            connection.extend(self.network_fields(theme, cx));
        }
        connection.push(form.validation_field(theme, if file_based { "File" } else { "Database" }, &form.database, cx));
        let mut sections = vec![section(theme, "Behavior", self.behaviour_fields(theme, cx))];
        if !file_based {
            sections.push(self.security_section(theme, cx));
            sections.push(self.ssh_section(theme, cx));
        }
        crate::surfaces::dialog_surface(size::HOME_WIDTH * 1.5, theme)
            .id("connection-dialog")
            .debug_selector(|| "connection-dialog".into())
            .max_h_full()
            .min_h_0()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .debug_selector(|| "connection-card".into())
                    .size_full()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .px_4()
                            .pt_4()
                            .pb_2()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(title))
                                    .child(
                                        div()
                                            .text_size(px(size::TEXT_SMALL))
                                            .text_color(rgb(theme.muted_foreground))
                                            .child(driver),
                                    ),
                            )
                            .child(
                                crate::surfaces::icon_button("close-connection", "x", "Close", theme)
                                    .on_click(cx.listener(|this, _, _, cx| this.close_connection_dialog(cx))),
                            ),
                    )
                    .child(
                        div()
                            .id("connection-page")
                            .debug_selector(|| "connection-page".into())
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px_4()
                            .pb_4()
                            .flex()
                            .flex_wrap()
                            .gap_4()
                            .child(
                                div()
                                    .debug_selector(|| "connection-main-section".into())
                                    .w(px(size::HOME_WIDTH / 2.0))
                                    .max_w_full()
                                    .flex_grow(1.0)
                                    .child(section(theme, "Connection", connection)),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "connection-options-section".into())
                                    .w(px(size::HOME_WIDTH / 2.0))
                                    .max_w_full()
                                    .flex_grow(1.0)
                                    .flex()
                                    .flex_col()
                                    .gap_4()
                                    .children(sections),
                            ),
                    )
                    .child(self.connection_actions(theme, cx)),
            )
    }

    fn security_section(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let form = &self.connection_form;
        let mut summary = TlsMode::from_index(form.tls).label().to_string();
        if form.kerberos {
            summary.push_str(" · Kerberos");
        }
        let rows = form.security_open.then(|| self.security_fields(theme, cx));
        collapsible_section(
            theme,
            "connection-security",
            "Security",
            summary,
            rows,
            cx.listener(|this, _, _, cx| {
                this.connection_form.security_open = !this.connection_form.security_open;
                cx.notify();
            }),
        )
    }

    fn ssh_section(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let form = &self.connection_form;
        let summary = match (form.ssh.on, form.ssh.jumps.len()) {
            (false, _) => "Off".to_string(),
            (true, 0) => "1 hop".to_string(),
            (true, jumps) => format!("{} hops", jumps + 1),
        };
        let rows = form.ssh_open.then(|| self.ssh_fields(theme, cx));
        collapsible_section(
            theme,
            "connection-ssh",
            "SSH tunnel",
            summary,
            rows,
            cx.listener(|this, _, _, cx| {
                this.connection_form.ssh_open = !this.connection_form.ssh_open;
                cx.notify();
            }),
        )
    }

    pub(crate) fn fill_from_connection_url(&mut self, text: &str, cx: &mut Context<Self>) {
        let registry = drivers_registry::default_registry();
        match connection_save::draft_from_url(&registry, text) {
            Ok((mut draft, password)) => {
                draft.id = self.connection_form.editing;
                self.fill_connection_form(draft, cx);
                if let Some(password) = password {
                    use secrecy::ExposeSecret;
                    self.connection_form
                        .password
                        .update(cx, |input, cx| input.set_content(password.expose_secret(), cx));
                }
                self.connection_form.status = "Filled from the connection URL. Review it, then save".into();
            }
            Err(message) => self.connection_form.status = format!("Could not read the connection URL: {message}"),
        }
        cx.notify();
    }

    pub(crate) fn connection_url_for_form(&self, cx: &gpui::App) -> Option<String> {
        connection_save::draft_url(&self.connection_form.draft(cx))
    }

    fn url_buttons(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .gap_2()
            .child(
                button(ButtonKind::Ghost, "paste-connection-url", "Paste URL", theme).on_click(cx.listener(
                    |this, _, _, cx| match cx.read_from_clipboard().and_then(|item| item.text()) {
                        Some(text) => this.fill_from_connection_url(&text, cx),
                        None => {
                            this.connection_form.status = "The clipboard has no connection URL".into();
                            cx.notify();
                        }
                    },
                )),
            )
            .child(
                button(ButtonKind::Ghost, "copy-connection-url", "Copy as URL", theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.connection_form.status = match this.connection_url_for_form(cx) {
                            Some(url) => {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(url));
                                "Copied the connection URL without its password".into()
                            }
                            None => "This connection type has no URL form".into(),
                        };
                        cx.notify();
                    },
                )),
            )
    }

    fn connection_actions(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let form = &self.connection_form;
        let status = (!form.status.is_empty()).then(|| {
            div()
                .id("connection-form-status")
                .debug_selector(|| "connection-form-status".into())
                .w_full()
                .max_h(px(size::FORM_CONTROL * 2.0))
                .overflow_y_scroll()
                .text_color(rgb(theme.muted_foreground))
                .child(form.status.clone())
        });
        let auxiliary = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(self.test_button(theme, cx))
            .child(self.url_buttons(theme, cx));
        let commit = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(self.save_button(theme, cx))
            .children(form.editing.is_none().then(|| self.connect_button(theme, cx)))
            .child(if form.saving {
                unavailable_button(
                    ButtonKind::Ghost,
                    "cancel-connection",
                    "Cancel",
                    "Saving the connection",
                    theme,
                )
            } else {
                button(ButtonKind::Ghost, "cancel-connection", "Cancel", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.close_connection_dialog(cx)))
            })
            .children(form.editing.map(|_| self.delete_button(theme, cx)));
        div()
            .debug_selector(|| "connection-actions".into())
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(rgb(theme.border))
            .children(status)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(auxiliary)
                    .child(commit),
            )
    }

    fn behaviour_fields(&self, theme: Theme, cx: &mut Context<Self>) -> Vec<Div> {
        let form = &self.connection_form;
        let environment = choice_row(
            theme,
            "Environment",
            [
                Environment::Local,
                Environment::Dev,
                Environment::Staging,
                Environment::Prod,
            ]
            .into_iter()
            .enumerate()
            .map(|(index, environment)| {
                segment(
                    ("environment", index),
                    environment.display_name(),
                    form.environment == environment,
                    theme,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.connection_form.environment = environment;
                    cx.notify();
                }))
            }),
        );
        let colour = self.connection_color_row(theme, cx);
        vec![
            environment,
            colour,
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(size::SIDE_PANEL_WIDTH / 2.0))
                        .text_color(rgb(theme.muted_foreground))
                        .child("Access"),
                )
                .child(self.read_only_toggle(theme, cx)),
        ]
    }

    fn connection_color_row(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        use gpui_component::button::{Button, ButtonVariants};
        use gpui_kit::component::{Selectable as _, Sizable as _};

        let swatches = std::iter::once(None).chain(
            bookie_app_services::read_session::CONNECTION_COLORS
                .iter()
                .copied()
                .map(Some),
        );
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(size::SIDE_PANEL_WIDTH / 2.0))
                    .text_color(rgb(theme.muted_foreground))
                    .child("Color"),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .flex_wrap()
                    .gap_1()
                    .children(swatches.enumerate().map(|(index, name)| {
                        let label = name.unwrap_or("No color");
                        let swatch = match name.and_then(bookie_app_services::read_session::connection_color_rgb) {
                            Some(color) => crate::chrome::connection_dot(color, "connection-swatch").into_any_element(),
                            None => crate::assets::icon("x", theme.muted_foreground).into_any_element(),
                        };
                        Button::new(("connection-colour", index))
                            .debug_selector(move || format!("swatch-{label}"))
                            .ghost()
                            .small()
                            .compact()
                            .w(px(size::SMALL_CONTROL))
                            .selected(self.connection_form.color.as_deref() == name)
                            .accessibility_label(label)
                            .tooltip(label)
                            .child(swatch)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.connection_form.color = name.map(str::to_owned);
                                cx.notify();
                            }))
                    })),
            )
    }

    fn network_fields(&self, theme: Theme, cx: &mut Context<Self>) -> Vec<Div> {
        let form = &self.connection_form;
        let mut rows = vec![
            form.validation_field(theme, "Host", &form.host, cx),
            form.validation_field(theme, "Port", &form.port, cx),
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
                    segment(("auth", index), label, selected, theme).on_click(cx.listener(move |this, _, _, cx| {
                        this.connection_form.kerberos = index == 1;
                        cx.notify();
                    }))
                }),
        );
        let tls = choice_row(
            theme,
            "TLS",
            TlsMode::ALL.into_iter().map(|mode| {
                segment(
                    ("tls", mode.index() as usize),
                    mode.label(),
                    form.tls == mode.index(),
                    theme,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.connection_form.tls = mode.index();
                    this.clear_resolved_connection_error(cx);
                    cx.notify();
                }))
            }),
        );
        let mut rows = vec![auth, tls];
        if TlsMode::from_index(form.tls) != TlsMode::Disabled {
            rows.push(field(theme, "CA file", &form.tls_root));
            rows.push(form.validation_field(theme, "Client cert", &form.tls_cert, cx));
            rows.push(form.validation_field(theme, "Client key", &form.tls_key, cx));
        }
        if form.socket_kind().is_some() {
            rows.push(form.validation_field(theme, "Socket", &form.socket, cx));
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
                segment(("driver", index), driver.label, index == form.driver, theme).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.connection_form.driver = index;
                        this.clear_resolved_connection_error(cx);
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
        if self.connection_form.saving {
            return unavailable_button(
                ButtonKind::Primary,
                "save-connection",
                "Saving…",
                "The connection is being saved",
                theme,
            );
        }
        let kind = if self.connection_form.editing.is_none() {
            ButtonKind::Ghost
        } else {
            ButtonKind::Primary
        };
        button(kind, "save-connection", "Save", theme).on_click(cx.listener(|this, _, _, cx| {
            this.save_connection(cx);
        }))
    }

    fn connect_button(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        if self.connection_form.testing || self.connection_form.saving {
            return unavailable_button(
                ButtonKind::Primary,
                "connect-connection",
                "Connect",
                "Wait for the current connection action to finish",
                theme,
            );
        }
        button(ButtonKind::Primary, "connect-connection", "Connect", theme)
            .on_click(cx.listener(|this, _, _, cx| this.save_connection_with_open(true, cx)))
    }

    fn test_button(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        if self.connection_form.saving {
            return unavailable_button(
                ButtonKind::Ghost,
                "test-connection",
                "Test",
                "Saving the connection",
                theme,
            );
        }
        if self.connection_form.testing {
            return unavailable_button(
                ButtonKind::Ghost,
                "test-connection",
                "Testing…",
                "The connection is being tested",
                theme,
            );
        }
        button(ButtonKind::Ghost, "test-connection", "Test", theme)
            .on_click(cx.listener(|this, _, _, cx| this.test_connection(cx)))
    }

    fn delete_button(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let confirm = self.connection_form.confirm_delete;
        button(
            ButtonKind::Destructive,
            "delete-connection",
            if confirm { "Confirm delete" } else { "Delete" },
            theme,
        )
        .on_click(cx.listener(|this, _, _, cx| this.delete_connection(cx)))
    }

    fn read_only_toggle(&self, _theme: Theme, cx: &mut Context<Self>) -> Div {
        let on = self.connection_form.read_only;
        let view = cx.entity();
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                gpui_kit::component::switch::Switch::new("connection-read-only")
                    .accessibility_label("Read-only")
                    .checked(on)
                    .on_click(move |on, _, cx| {
                        view.update(cx, |this, cx| {
                            this.connection_form.read_only = *on;
                            cx.notify();
                        });
                    }),
            )
            .child("Read-only")
    }

    pub(crate) fn edit_connection(&mut self, id: uuid::Uuid, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let draft = cx
                .background_executor()
                .spawn(async move {
                    let registry = drivers_registry::default_registry();
                    block_on(connection_save::load_draft(&registry, id))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match draft {
                    Ok(Some(draft)) => this.fill_connection_form(draft, cx),
                    Ok(None) => this.connection_form.status = "That connection no longer exists".into(),
                    Err(error) => this.connection_form.status = error.to_string(),
                }
                this.open_connection_dialog(cx);
            });
        })
        .detach();
    }

    pub(crate) fn fill_connection_form(&mut self, draft: ConnectionDraft, cx: &mut Context<Self>) {
        let form = &mut self.connection_form;
        form.editing = draft.id;
        form.color = draft
            .id
            .and_then(|id| self.connections.iter().find(|connection| connection.id == id))
            .and_then(|connection| connection.color.clone());
        form.driver = form
            .drivers
            .iter()
            .position(|driver| driver.id == draft.driver_id)
            .unwrap_or(form.driver);
        form.read_only = draft.read_only;
        form.environment = draft.environment;
        form.status = String::new();
        form.kerberos = draft.auth_mode == AuthMode::Kerberos;
        form.tls = draft.tls_mode.index();
        form.ssh.fill(draft.ssh.as_ref(), cx);
        form.ssh_open = draft.ssh.is_some();
        form.security_open = draft.auth_mode == AuthMode::Kerberos
            || !draft.tls_root_cert.trim().is_empty()
            || !draft.tls_client_cert.trim().is_empty()
            || !draft.tls_client_key.trim().is_empty()
            || !draft.socket_endpoint.trim().is_empty()
            || matches!(draft.tls_mode, TlsMode::Prefer | TlsMode::Require | TlsMode::VerifyCa);
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

    pub(crate) fn clear_resolved_connection_error(&mut self, cx: &mut Context<Self>) {
        if let Some((_, message)) = &self.connection_form.validation_error {
            if connection_save::validate(&self.connection_form.draft(cx))
                .map_err(|error| error.to_string())
                .as_ref()
                .err()
                != Some(message)
            {
                self.connection_form.validation_error = None;
                self.connection_form.status.clear();
            }
        }
    }

    fn validate_connection(&mut self, draft: &ConnectionDraft, cx: &mut Context<Self>) -> bool {
        self.connection_form.validation_error = None;
        self.connection_form.focus_validation = None;
        if let Err(error) = connection_save::validate(draft) {
            let message = error.to_string();
            if matches!(
                &error,
                connection_save::ConnectionValidationError::SocketMustBeAbsolute
                    | connection_save::ConnectionValidationError::ClientCertificatePair
            ) {
                self.connection_form.security_open = true;
            }
            if matches!(
                &error,
                connection_save::ConnectionValidationError::SshHeadFieldRequired { .. }
                    | connection_save::ConnectionValidationError::SshJumpFieldRequired { .. }
                    | connection_save::ConnectionValidationError::SshPortRequired { .. }
                    | connection_save::ConnectionValidationError::SshPrivateKeyRequired { .. }
            ) {
                self.connection_form.ssh_open = true;
            }
            let input = match &error {
                connection_save::ConnectionValidationError::DatabaseFileRequired => {
                    Some(&self.connection_form.database)
                }
                connection_save::ConnectionValidationError::HostRequired => Some(&self.connection_form.host),
                connection_save::ConnectionValidationError::SocketMustBeAbsolute => Some(&self.connection_form.socket),
                connection_save::ConnectionValidationError::PortRequired => Some(&self.connection_form.port),
                connection_save::ConnectionValidationError::ClientCertificatePair => {
                    Some(if draft.tls_client_cert.trim().is_empty() {
                        &self.connection_form.tls_cert
                    } else {
                        &self.connection_form.tls_key
                    })
                }
                connection_save::ConnectionValidationError::SshHeadFieldRequired { field } => Some(match field {
                    connection_save::SshField::Host => &self.connection_form.ssh.head.host,
                    connection_save::SshField::User => &self.connection_form.ssh.head.user,
                }),
                connection_save::ConnectionValidationError::SshJumpFieldRequired { index, field } => self
                    .connection_form
                    .ssh
                    .jumps
                    .get(index.saturating_sub(1))
                    .map(|hop| match field {
                        connection_save::SshField::Host => &hop.host,
                        connection_save::SshField::User => &hop.user,
                    }),
                connection_save::ConnectionValidationError::SshPortRequired { index } => {
                    if *index == 0 {
                        Some(&self.connection_form.ssh.head.port)
                    } else {
                        self.connection_form.ssh.jumps.get(index - 1).map(|hop| &hop.port)
                    }
                }
                connection_save::ConnectionValidationError::SshPrivateKeyRequired { index } => {
                    if *index == 0 {
                        Some(&self.connection_form.ssh.head.key)
                    } else {
                        self.connection_form.ssh.jumps.get(index - 1).map(|hop| &hop.key)
                    }
                }
                connection_save::ConnectionValidationError::DriverRequired => None,
            };
            if let Some(input) = input {
                self.connection_form.validation_error = Some((input.entity_id(), message.clone()));
                self.connection_form.focus_validation = Some(input.read(cx).focus_handle(cx));
            }
            self.connection_form.status = message;
            cx.notify();
            return false;
        }
        true
    }

    pub(crate) fn test_connection(&mut self, cx: &mut Context<Self>) {
        if self.connection_form.testing {
            return;
        }
        let draft = self.connection_form.draft(cx);
        if !self.validate_connection(&draft, cx) {
            return;
        }
        self.connection_form.testing = true;
        self.connection_form.status = "Testing…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    let registry = drivers_registry::default_registry();
                    let audit = bookie_app_services::database_service::DatabaseService::new().transport_audit_factory();
                    block_on(connection_save::test(
                        &registry,
                        &draft,
                        tokio_util::sync::CancellationToken::new(),
                        Some(&audit),
                        bookie_gpui::host_key::prompter(),
                    ))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.connection_form.testing = false;
                this.connection_form.status = outcome.unwrap_or_else(|error| error.to_string());
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
            self.connection_form.status =
                "Click Delete again to remove this connection and its stored passwords".into();
            cx.notify();
            return;
        }
        cx.spawn(async move |this, cx| {
            let (outcome, connections) = cx
                .background_executor()
                .spawn(async move {
                    let outcome = block_on(connection_save::delete(id));
                    let connections = outcome.is_ok().then(bookie_gpui::live::saved_connections);
                    (outcome, connections)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(Ok(connections)) = connections {
                    this.connections = connections;
                }
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
                    "Deleted; some stored credentials could not be removed".into()
                } else {
                    "Deleted".into()
                };
            }
            Err(message) => self.connection_form.status = message,
        }
    }

    pub(crate) fn save_connection(&mut self, cx: &mut Context<Self>) {
        self.save_connection_with_open(false, cx);
    }

    fn save_connection_with_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.connection_form.saving || (open && self.connection_form.testing) {
            return;
        }
        let draft = self.connection_form.draft(cx);
        if !self.validate_connection(&draft, cx) {
            return;
        }
        let color = self.connection_form.color.clone();
        self.connection_form.saving = true;
        self.connection_form.status = "Saving…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let (result, connections) = cx
                .background_executor()
                .spawn(async move {
                    let result = block_on(async move {
                        let saved = connection_save::save(draft).await.map_err(|error| error.to_string())?;
                        let color_saved =
                            bookie_app_services::read_session::set_connection_color(saved.id, color.as_deref())
                                .await
                                .map_err(|error| {
                                    tracing::warn!(%error, "the connection colour was not saved");
                                })
                                .is_ok();
                        let hops = saved.ssh.as_ref().map_or_else(Vec::new, |ssh| {
                            ssh.flatten_hops().iter().map(|hop| hop.hop_id).collect()
                        });
                        Ok::<_, String>((saved.id, saved.name, hops, color_saved))
                    });
                    let connections = result.is_ok().then(bookie_gpui::live::saved_connections);
                    (result, connections)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(Ok(connections)) = connections {
                    this.connections = connections;
                }
                let saved = this.finish_connection_save(result, cx);
                if let Some(id) = saved.filter(|_| open) {
                    this.connect(bookie_gpui::live::Origin::Saved(id.to_string()), cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn finish_connection_save(
        &mut self,
        result: Result<(uuid::Uuid, String, Vec<uuid::Uuid>, bool), String>,
        cx: &mut Context<Self>,
    ) -> Option<uuid::Uuid> {
        self.connection_form.saving = false;
        match result {
            Ok((id, name, hops, color_saved)) => {
                self.connection_form.editing = Some(id);
                if let Some((head, jumps)) = hops.split_first() {
                    self.connection_form.ssh.head.hop_id = Some(*head);
                    for (form, id) in self.connection_form.ssh.jumps.iter_mut().zip(jumps) {
                        form.hop_id = Some(*id);
                    }
                }
                self.connection_form.status = if color_saved {
                    format!("Saved {name}")
                } else {
                    format!("Saved {name}, but the color was not saved. Try saving again")
                };
                if color_saved {
                    self.status = self.connection_form.status.clone();
                    self.close_connection_dialog(cx);
                }
                self.connection_form
                    .password
                    .update(cx, |input, cx| input.set_content("", cx));
                color_saved.then_some(id)
            }
            Err(message) => {
                self.connection_form.status = message;
                None
            }
        }
    }
}
