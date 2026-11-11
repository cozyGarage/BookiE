//! MCP pairing / token issuance page for Preferences.

use relm4::adw::prelude::*;
use relm4::{adw, gtk};

use std::sync::Arc;

use bookie_mcp::{McpBridge, TokenPermissions};

use crate::services::mcp_service;
use bookie_app_services::tr;

pub fn build_page(
    database: &crate::services::database_service::DatabaseService,
    mcp_bridge: Option<Arc<McpBridge>>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(tr!("MCP"))
        .icon_name("network-server-symbolic")
        .build();

    let status_group = status_group(database);
    let issue = issue_group();
    let tokens_group = adw::PreferencesGroup::builder().title(tr!("Active tokens")).build();
    refresh_token_list(&tokens_group, mcp_bridge.clone());

    let name_for_issue = issue.name_row.clone();
    let scope_for_issue = issue.scope_combo.clone();
    let tokens_for_issue = tokens_group.clone();
    let page_for_dialog = page.clone();
    issue.button.connect_clicked(move |_| {
        let name = name_for_issue.text().to_string();
        if name.trim().is_empty() {
            return;
        }
        let permissions = match scope_for_issue.selected() {
            1 => TokenPermissions::ReadWrite,
            _ => TokenPermissions::ReadOnly,
        };
        glib::spawn_future_local(issue_token_flow(
            page_for_dialog.clone(),
            tokens_for_issue.clone(),
            mcp_bridge.clone(),
            name,
            permissions,
        ));
    });

    page.add(&status_group);
    page.add(&issue.group);
    page.add(&tokens_group);
    page
}

struct IssueGroup {
    group: adw::PreferencesGroup,
    name_row: adw::EntryRow,
    scope_combo: adw::ComboRow,
    button: gtk::Button,
}

fn status_group(database: &crate::services::database_service::DatabaseService) -> adw::PreferencesGroup {
    let policy_reload = database.reload_policy();
    let status_group = adw::PreferencesGroup::builder()
        .title(tr!("Agent access"))
        .description(tr!(
            "Loopback MCP server on 127.0.0.1:17432. Tokens gate which connections agents may use."
        ))
        .build();

    if let Err(error) = policy_reload {
        tracing::warn!(error = %error, "policy reload failed; keeping the previous policy");
        let advice = if database.policy_available() {
            tr!("The previous policy is still in use. Fix the policy file and reopen Preferences.")
        } else {
            tr!("Database operations are disabled. Fix policy.toml and reopen Preferences or restart BookiE.")
        };
        let row = adw::ActionRow::builder()
            .use_markup(false)
            .title(tr!("Policy file could not be reloaded"))
            .subtitle(advice)
            .build();
        status_group.add(&row);
    }

    let endpoint_row = adw::ActionRow::builder()
        .title(tr!("Endpoint"))
        .subtitle("http://127.0.0.1:17432/mcp")
        .build();
    status_group.add(&endpoint_row);
    status_group
}

fn issue_group() -> IssueGroup {
    let group = adw::PreferencesGroup::builder()
        .title(tr!("Issue token"))
        .description(tr!(
            "Plaintext is shown once. Keyring storage is best-effort; the confirmation states whether it was saved."
        ))
        .build();

    let name_row = adw::EntryRow::builder().title(tr!("Client name")).build();
    name_row.set_text("Cursor");

    let scope_combo = adw::ComboRow::builder().title(tr!("Scope")).build();
    let scope_model = gtk::StringList::new(&["Read only", "Read and write"]);
    scope_combo.set_model(Some(&scope_model));
    scope_combo.set_selected(0);

    let button = gtk::Button::builder()
        .label(tr!("Issue token"))
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("suggested-action");
    let issue_row = adw::ActionRow::builder().title(tr!("Create pairing token")).build();
    issue_row.add_suffix(&button);

    group.add(&name_row);
    group.add(&scope_combo);
    group.add(&issue_row);
    IssueGroup {
        group,
        name_row,
        scope_combo,
        button,
    }
}

fn present_issue_error(parent: &impl IsA<gtk::Widget>, message: &str) {
    let alert = adw::AlertDialog::new(Some(&tr!("Could not issue token")), Some(message));
    alert.add_response("ok", &tr!("OK"));
    alert.set_default_response(Some("ok"));
    alert.present(Some(parent));
}

async fn issue_token_flow(
    parent: adw::PreferencesPage,
    tokens_group: adw::PreferencesGroup,
    bridge: Option<Arc<McpBridge>>,
    name: String,
    permissions: TokenPermissions,
) {
    let allowlist = match bookie_storage::load_connections().await {
        Ok(list) => list.into_iter().map(|c| c.id).collect::<Vec<_>>(),
        Err(e) => {
            present_issue_error(&parent, &format!("could not load connections: {e}"));
            return;
        }
    };
    if allowlist.is_empty() {
        present_issue_error(
            &parent,
            &tr!(
                "Save at least one connection before issuing an MCP token. Tokens are scoped to the connections that exist at issue time."
            ),
        );
        return;
    }
    match mcp_service::issue_token(bridge.as_ref(), name, permissions, allowlist).await {
        Ok(issued) => {
            show_issued_token(&parent, &issued.plaintext, issued.keyring_saved);
            clear_and_refresh(&tokens_group, bridge.clone());
        }
        Err(e) => {
            tracing::warn!(error = %e, "MCP token issue failed");
            present_issue_error(&parent, &e);
        }
    }
}

fn clear_and_refresh(group: &adw::PreferencesGroup, mcp_bridge: Option<Arc<McpBridge>>) {
    while let Some(child) = group.first_child() {
        group.remove(&child);
    }
    refresh_token_list(group, mcp_bridge);
}

fn refresh_token_list(group: &adw::PreferencesGroup, mcp_bridge: Option<Arc<McpBridge>>) {
    let tokens = mcp_service::list_tokens(mcp_bridge.as_ref());
    if tokens.is_empty() {
        let empty = adw::ActionRow::builder()
            .title(tr!("No tokens yet"))
            .subtitle(tr!("Issue a token above to pair an MCP client."))
            .build();
        group.add(&empty);
        return;
    }
    for token in tokens {
        if token.revoked {
            continue;
        }
        let row = adw::ActionRow::builder()
            .use_markup(false)
            .title(&token.name)
            .subtitle(format!(
                "{} · {}",
                permissions_label(token.permissions),
                token.created_at.format("%Y-%m-%d")
            ))
            .build();
        let revoke = gtk::Button::builder()
            .label(tr!("Revoke"))
            .valign(gtk::Align::Center)
            .build();
        revoke.add_css_class("destructive-action");
        revoke.add_css_class("flat");
        let id = token.id;
        let group_c = group.clone();
        let bridge_for_revoke = mcp_bridge.clone();
        revoke.connect_clicked(move |button| {
            if let Err(e) = mcp_service::revoke_token(bridge_for_revoke.as_ref(), id) {
                tracing::warn!(error = %e, "revoke failed");
                let alert = adw::AlertDialog::new(Some(&tr!("Token not revoked")), Some(&e));
                alert.add_response("close", &tr!("Close"));
                alert.present(Some(button));
            }
            clear_and_refresh(&group_c, bridge_for_revoke.clone());
        });
        row.add_suffix(&revoke);
        group.add(&row);
    }
}

fn permissions_label(p: TokenPermissions) -> &'static str {
    match p {
        TokenPermissions::ReadOnly => "read-only",
        TokenPermissions::ReadWrite => "read-write",
    }
}

fn show_issued_token(parent: &impl IsA<gtk::Widget>, plaintext: &str, keyring_saved: bool) {
    issued_token_dialog(plaintext, keyring_saved).present(Some(parent));
}

fn issued_token_dialog(plaintext: &str, keyring_saved: bool) -> adw::AlertDialog {
    let alert = adw::AlertDialog::new(
        Some(&tr!("Token issued")),
        Some(&issued_token_message(plaintext, keyring_saved)),
    );
    alert.add_response("ok", &tr!("Done"));
    alert.set_default_response(Some("ok"));
    alert.set_close_response("ok");
    alert.set_extra_child(Some(&copy_token_button(plaintext)));
    alert
}

fn copy_token_button(plaintext: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .label(tr!("Copy Token"))
        .halign(gtk::Align::Center)
        .css_classes(["pill"])
        .build();
    let token = plaintext.to_owned();
    button.connect_clicked(move |button| {
        button.clipboard().set_text(&token);
        button.set_label(&tr!("Copied"));
    });
    button
}

fn issued_token_message(plaintext: &str, keyring_saved: bool) -> String {
    let storage_notice = if keyring_saved {
        tr!("This token is saved in the system keyring.")
    } else {
        tr!(
            "This token was not saved in the system keyring. Copy it now and add it to your MCP client configuration; closing this dialog loses it."
        )
    };
    format!(
        "{}\n\n{}\n\n{}\n\n{}",
        tr!("Copy this token now. It will not be shown again in the UI."),
        plaintext,
        tr!("Endpoint: http://127.0.0.1:17432/mcp"),
        storage_notice,
    )
}

#[cfg(test)]
mod copy_tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn the_copy_button_puts_the_token_on_the_clipboard_and_keeps_the_dialog_open() {
        adw::init().unwrap();
        let button = copy_token_button("tok-abc-123");
        button.emit_clicked();
        let content = button.clipboard().content().unwrap();
        let value = content.value(gtk::glib::Type::STRING).unwrap();
        assert_eq!(value.get::<String>().unwrap(), "tok-abc-123");
        assert_eq!(button.label().as_deref(), Some("Copied"));
        let dialog = issued_token_dialog("tok-abc-123", true);
        assert!(
            dialog.extra_child().is_some(),
            "the button lives in the dialog body, so copying does not dismiss it"
        );
        assert!(!dialog.has_response("copy"));
    }
}
