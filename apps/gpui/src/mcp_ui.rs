use std::sync::Arc;

use bookie_app_services::mcp_service::{
    AppConnectionProvider, McpBridge, McpToken, RunningMcpServer, TokenPermissions,
};
use bookie_gpui::theme::Theme;
use gpui::{Context, Div, Entity, Stateful, div, prelude::*};
use gpui_kit::component::setting::{SettingField, SettingGroup, SettingItem};

use crate::GridView;
use crate::chrome::{ghost, primary_button};
use crate::ui_theme::ThemeExt;

#[derive(Default)]
pub(crate) struct McpState {
    connections: Arc<AppConnectionProvider>,
    server: Option<RunningMcpServer>,
    plaintext: Option<String>,
    status: Option<String>,
}

impl McpState {
    pub(crate) fn register(&self, database: &Arc<bookie_app_services::database_service::DatabaseService>) {
        self.connections.register_database(database);
    }

    pub(crate) fn bridge(&self) -> Option<Arc<McpBridge>> {
        self.server.as_ref().map(|server| server.bridge.clone())
    }

    pub(crate) fn enabled(&self) -> bool {
        self.server.is_some()
    }

    pub(crate) fn group(&self, view: Entity<GridView>) -> SettingGroup {
        let read = view.clone();
        let write = view.clone();
        let enabled = SettingField::switch(
            move |cx| read.read(cx).mcp.enabled(),
            move |enabled, cx| {
                write.update(cx, |this, cx| this.set_mcp_enabled(enabled, cx));
            },
        );
        let actions_view = view.clone();
        let actions = SettingField::render(move |_, _, cx| {
            let theme = Theme::current(cx);
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(button(
                    "mcp-issue-read",
                    "Issue read-only token",
                    actions_view.clone(),
                    TokenPermissions::ReadOnly,
                    theme,
                ))
                .child(button(
                    "mcp-issue-write",
                    "Issue read-write token",
                    actions_view.clone(),
                    TokenPermissions::ReadWrite,
                    theme,
                ))
        });
        let status_view = view.clone();
        let status = SettingField::render(move |_, _, cx| {
            let value = status_view.read(cx).mcp.status.clone();
            div().id("mcp-status").when_some(value, |this, value| this.child(value))
        });
        let token_view = view.clone();
        let tokens = SettingField::render(move |_, _, cx| token_list(token_view.clone(), Theme::current(cx), cx));
        let secret_view = view;
        let secret = SettingField::render(move |_, _, cx| {
            let plaintext = secret_view.read(cx).mcp.plaintext.clone();
            let Some(plaintext) = plaintext else { return div().id("mcp-issued-token") };
            let hide_view = secret_view.clone();
            div()
                .id("mcp-issued-token")
                .flex()
                .items_center()
                .gap_2()
                .child(div().child(plaintext))
                .child(
                    ghost("mcp-hide-issued-token", "Hide", Theme::current(cx))
                        .on_click(move |_, _, cx| {
                            hide_view.update(cx, |this, cx| {
                                this.mcp.plaintext = None;
                                cx.notify();
                            });
                        }),
                )
        });
        let mut group = SettingGroup::new()
            .item(SettingItem::new("MCP server", enabled).description("Use BookiE's governed local MCP endpoint."))
            .item(
                SettingItem::new("Issue token", actions)
                    .description("Tokens are limited to the active saved connection; plaintext is shown once."),
            );
        if self.plaintext.is_some() {
            group = group.item(SettingItem::new("Issued token", secret));
        }
        if self.status.is_some() {
            group = group.item(SettingItem::new("Token status", status));
        }
        group.item(SettingItem::new("Active tokens", tokens))
    }
}

fn button(
    id: &'static str,
    label: &'static str,
    view: Entity<GridView>,
    permissions: TokenPermissions,
    theme: Theme,
) -> Stateful<Div> {
    primary_button(id, label, label, theme)
        .debug_selector(move || id.into())
        .on_click(move |_, _, cx| {
            view.update(cx, |this, cx| this.issue_mcp_token(permissions, cx));
        })
}

fn token_list(view: Entity<GridView>, theme: Theme, cx: &gpui::App) -> Div {
    let Some(bridge) = view.read(cx).mcp.bridge() else {
        return div().child("Start the MCP server to manage tokens.");
    };
    let mut tokens = bridge.tokens().list();
    tokens.sort_by(|left, right| left.name.cmp(&right.name));
    let mut list = div().flex().flex_col().gap_1();
    for token in tokens {
        list = list.child(token_row(view.clone(), bridge.clone(), token, theme));
    }
    list
}

fn token_row(view: Entity<GridView>, bridge: Arc<McpBridge>, token: McpToken, theme: Theme) -> Div {
    let permissions = match token.permissions {
        TokenPermissions::ReadOnly => "read-only",
        TokenPermissions::ReadWrite => "read-write",
    };
    let revoked = token.revoked;
    let id = token.id;
    let label = format!(
        "{} · {permissions}{}",
        token.name,
        if revoked { " · revoked" } else { "" }
    );
    div()
        .flex()
        .items_center()
        .justify_between()
        .child(label)
        .when(!revoked, |row| {
            let view = view.clone();
            let bridge = bridge.clone();
            row.child(
                ghost(gpui::ElementId::Name(format!("mcp-revoke-{id}").into()), "Revoke", theme)
                    .on_click(move |_, _, cx| {
                        let bridge = bridge.clone();
                        view.update(cx, |this, cx| this.revoke_mcp_token(bridge, id, cx));
                    }),
            )
        })
}

impl GridView {
    pub(crate) fn set_mcp_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if enabled == self.mcp.enabled() {
            return;
        }
        if enabled {
            let Some(database) = self.live.as_ref().map(|live| live.database_service().clone()) else {
                self.mcp.status = Some("Connect to a saved database before enabling MCP.".into());
                cx.notify();
                return;
            };
            self.mcp.server = bookie_app_services::mcp_service::start_background_with_provider(
                database,
                self.mcp.connections.clone(),
            );
            self.mcp.status = Some(if self.mcp.enabled() {
                "MCP server enabled.".into()
            } else {
                "MCP server could not start; check the audit and policy configuration.".into()
            });
        } else {
            if let Some(server) = self.mcp.server.take() {
                server.shutdown();
            }
            self.mcp.status = Some("MCP server disabled.".into());
        }
        cx.notify();
    }

    fn revoke_mcp_token(&mut self, bridge: Arc<McpBridge>, id: uuid::Uuid, cx: &mut Context<Self>) {
        let view = cx.entity();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    crate::resolve_ui::blocking(async move {
                        bookie_app_services::mcp_service::revoke_token(Some(&bridge), id).await
                    })
                })
                .await;
            view.update(cx, |this, cx| {
                if result.is_ok() {
                    this.mcp.plaintext = None;
                }
                this.mcp.status = Some(result.map_or_else(|error| error, |_| "Token revoked.".into()));
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn issue_mcp_token(&mut self, permissions: TokenPermissions, cx: &mut Context<Self>) {
        let Some(bridge) = self.mcp.bridge() else {
            self.mcp.status = Some("Enable the MCP server before issuing a token.".into());
            cx.notify();
            return;
        };
        let Some(connection_id) = self
            .live
            .as_ref()
            .and_then(|live| live.saved.as_ref())
            .map(|(id, _)| *id)
        else {
            self.mcp.status = Some("A saved database connection is required to issue a token.".into());
            cx.notify();
            return;
        };
        let view = cx.entity();
        self.mcp.status = Some("Issuing token…".into());
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    crate::resolve_ui::blocking(async move {
                        bookie_app_services::mcp_service::issue_token(
                            Some(&bridge),
                            "GPUI client".into(),
                            permissions,
                            vec![connection_id],
                            None,
                            bookie_app_services::mcp_service::expiry_for_choice(1, chrono::Utc::now()),
                        )
                        .await
                    })
                })
                .await;
            view.update(cx, |this, cx| {
                match result {
                    Ok(issued) => {
                        this.mcp.plaintext = Some(issued.plaintext);
                        this.mcp.status = Some(if issued.keyring_saved {
                            "Token issued and saved to the keyring.".into()
                        } else {
                            "Token issued; keyring storage was unavailable.".into()
                        });
                    }
                    Err(error) => this.mcp.status = Some(format!("Could not issue token: {error}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
#[path = "mcp_ui_tests.rs"]
mod tests;
