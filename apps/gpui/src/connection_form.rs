use bookie_app_services::connection_save::{self, ConnectionDraft};
use bookie_core::{Environment, TlsMode};
use gpui::{Context, Div, Entity, Stateful, div, prelude::*, px, rgb};
use secrecy::SecretString;

use crate::GridView;
use crate::text_input::TextInput;
use bookie_gpui::theme::{Theme, size};

pub(crate) struct DriverChoice {
    pub id: &'static str,
    pub label: &'static str,
    pub port: u16,
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
    pub editing: Option<uuid::Uuid>,
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
            editing: None,
        }
    }

    pub fn selected(&self) -> Option<&DriverChoice> {
        self.drivers.get(self.driver)
    }

    pub fn draft(&self, cx: &gpui::App) -> ConnectionDraft {
        let text = |input: &Entity<TextInput>| input.read(cx).content().to_string();
        let driver = self.selected();
        ConnectionDraft {
            id: self.editing,
            name: text(&self.name),
            driver_id: driver.map(|driver| driver.id.to_string()).unwrap_or_default(),
            host: text(&self.host),
            port: text(&self.port)
                .trim()
                .parse()
                .unwrap_or_else(|_| driver.map_or(0, |driver| driver.port)),
            database: text(&self.database),
            username: text(&self.username),
            password: SecretString::from(text(&self.password)),
            tls_mode: TlsMode::default(),
            read_only: self.read_only,
            environment: Environment::default(),
            keep_stored_password: self.editing.is_some(),
        }
    }
}

impl GridView {
    pub(crate) fn connection_page(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let form = &self.connection_form;
        let field = |label: &'static str, input: &Entity<TextInput>| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().w(px(size::SIDE_PANEL_WIDTH / 2.0)).text_color(rgb(theme.muted_foreground)).child(label))
                .child(
                    div().flex_1().h(px(size::FORM_CONTROL)).child(input.clone()),
                )
        };
        let chips = self.driver_chips(theme, cx);
        let file_based = form.selected().is_some_and(|driver| matches!(driver.id, "sqlite" | "duckdb"));
        div().flex_1().min_h_0().p_4().child(
            div()
                .w(px(size::HOME_WIDTH / 2.0))
                .flex()
                .flex_col()
                .gap_3()
                .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(if form.editing.is_some() {
                    "Edit connection"
                } else {
                    "New connection"
                }))
                .child(div().flex().flex_wrap().gap_1().children(chips))
                .child(field("Name", &form.name))
                .when(!file_based, |page| {
                    page.child(field("Host", &form.host))
                        .child(field("Port", &form.port))
                        .child(field("User", &form.username))
                        .child(field("Password", &form.password))
                })
                .child(field(if file_based { "File" } else { "Database" }, &form.database))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(self.save_button(theme, cx))
                        .child(self.read_only_toggle(theme, cx))
                        .child(div().text_color(rgb(theme.muted_foreground)).child(form.status.clone())),
                ),
        )
    }

    fn driver_chips(&self, theme: Theme, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let form = &self.connection_form;
        form.drivers.iter().enumerate().map(|(index, driver)| {
            let selected = index == form.driver;
            let port = driver.port;
            div()
                .id(("driver", index))
                .flex()
                .items_center()
                .h(px(size::SMALL_CONTROL))
                .px_2()
                .rounded(px(theme.radius))
                .cursor_pointer()
                .border_1()
                .border_color(rgb(if selected { theme.foreground } else { theme.border }))
                .when(selected, |chip| chip.bg(rgb(theme.accent)))
                .child(driver.label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.connection_form.driver = index;
                    let port_text = if port == 0 { String::new() } else { port.to_string() };
                    this.connection_form.port.update(cx, |input, cx| input.set_content(&port_text, cx));
                    cx.notify();
                }))
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
        form.driver = form.drivers.iter().position(|driver| driver.id == draft.driver_id).unwrap_or(form.driver);
        form.read_only = draft.read_only;
        form.status = String::new();
        let port = draft.port.to_string();
        for (input, text) in [
            (&form.name, draft.name.as_str()),
            (&form.host, draft.host.as_str()),
            (&form.port, port.as_str()),
            (&form.database, draft.database.as_str()),
            (&form.username, draft.username.as_str()),
            (&form.password, ""),
        ] {
            input.update(cx, |input, cx| input.set_content(text, cx));
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
                        this.connection_form.password.update(cx, |input, cx| input.set_content("", cx));
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
