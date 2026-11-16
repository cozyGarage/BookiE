use std::path::PathBuf;

use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use secrecy::{ExposeSecret, SecretString};

use bookie_app_services::ssh_chain_form::{ChainPlan, plan_chain};
use bookie_ssh::{SshAuth, SshConfig};
use bookie_storage::{SavedSshAuth, SavedSshConfig};

use super::ssh_jumps::{SshJumps, chain_error_text};

const SSH_AUTH_PASSWORD: u32 = 0;
const SSH_AUTH_KEY: u32 = 1;
const SSH_AUTH_AGENT: u32 = 2;

/// SSH section uses a single `AdwPreferencesGroup` containing one
/// `AdwExpanderRow`. The expander's enable-switch toggles whether the
/// tunnel is used; expanding it reveals the host / port / user / auth
/// rows. This is the native Adwaita pattern for an optional sub-form
/// (matches GNOME Settings' "Custom Network Settings" expander).
pub struct SshSection {
    pub group: adw::PreferencesGroup,
    pub expander: adw::ExpanderRow,
    pub auth_combo: adw::ComboRow,
    host: adw::EntryRow,
    port: adw::SpinRow,
    local_port: adw::SpinRow,
    user: adw::EntryRow,
    password: adw::PasswordEntryRow,
    key_path: adw::EntryRow,
    agent_key: adw::EntryRow,
    passphrase: adw::PasswordEntryRow,
    system_client: adw::SwitchRow,
    pub jumps: SshJumps,
    hop_id: std::cell::Cell<Option<uuid::Uuid>>,
}

#[derive(Clone)]
pub struct SshInputs {
    pub cfg: SshConfig,
    pub saved: SavedSshConfig,
    pub secret_to_store: SshSecretToStore,
    pub chain: ChainPlan,
}

#[derive(Clone)]
pub enum SshSecretToStore {
    Password(SecretString),
    Passphrase(SecretString),
    None,
}

impl SshSection {
    pub fn build() -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(bookie_app_services::tr!("SSH tunnel"))
            .build();

        let expander = adw::ExpanderRow::builder()
            .title(bookie_app_services::tr!("Use SSH tunnel"))
            .subtitle(bookie_app_services::tr!("Reach the database through a bastion host"))
            .show_enable_switch(true)
            .enable_expansion(false)
            .build();
        group.add(&expander);

        let host = adw::EntryRow::builder().title(bookie_app_services::tr!("Host")).build();
        let port = adw::SpinRow::with_range(1.0, 65535.0, 1.0);
        port.set_title(&bookie_app_services::tr!("Port"));
        port.set_value(22.0);
        let local_port = adw::SpinRow::with_range(0.0, 65535.0, 1.0);
        local_port.set_title(&bookie_app_services::tr!("Local port"));
        local_port.set_subtitle(&bookie_app_services::tr!("0 picks a free port"));
        let user = adw::EntryRow::builder()
            .title(bookie_app_services::tr!("Username"))
            .build();

        let auth_pwd = bookie_app_services::tr!("Password");
        let auth_key = bookie_app_services::tr!("Private key");
        let auth_agent = bookie_app_services::tr!("SSH agent");
        let auth_model = gtk::StringList::new(&[auth_pwd.as_str(), auth_key.as_str(), auth_agent.as_str()]);
        let auth_combo = adw::ComboRow::builder()
            .title(bookie_app_services::tr!("Authentication"))
            .model(&auth_model)
            .selected(SSH_AUTH_PASSWORD)
            .build();

        let password = adw::PasswordEntryRow::builder()
            .title(bookie_app_services::tr!("Password"))
            .build();
        let key_path = adw::EntryRow::builder()
            .title(bookie_app_services::tr!("Private key path"))
            .text(default_ssh_key_path())
            .build();
        attach_key_browse_button(
            &key_path,
            &bookie_app_services::tr!("Browse for private key"),
            &bookie_app_services::tr!("Select SSH private key"),
            &["id_*", "*.pem", "*.key"],
        );
        let agent_key = adw::EntryRow::builder()
            .title(bookie_app_services::tr!("Agent key file (optional)"))
            .build();
        attach_key_browse_button(
            &agent_key,
            &bookie_app_services::tr!("Browse for public key"),
            &bookie_app_services::tr!("Select SSH public key"),
            &["*.pub"],
        );
        let passphrase = adw::PasswordEntryRow::builder()
            .title(bookie_app_services::tr!("Passphrase"))
            .build();
        let system_client = system_client_row();
        let jumps = SshJumps::build(&expander);

        expander.add_row(&host);
        expander.add_row(&port);
        expander.add_row(&local_port);
        expander.add_row(&user);
        expander.add_row(&auth_combo);
        expander.add_row(&password);
        expander.add_row(&key_path);
        expander.add_row(&agent_key);
        expander.add_row(&passphrase);
        expander.add_row(&system_client);
        jumps.attach();
        let jumps_for_client = jumps.clone();
        system_client.connect_active_notify(move |row| jumps_for_client.follow_system_client(row.is_active()));

        let section = Self {
            group,
            expander,
            auth_combo,
            host,
            port,
            local_port,
            user,
            password,
            key_path,
            agent_key,
            passphrase,
            system_client,
            jumps,
            hop_id: std::cell::Cell::new(None),
        };
        section.refresh_auth_visibility();
        section
    }

    pub fn prefill(&self, saved: &SavedSshConfig, password: Option<&SecretString>, passphrase: Option<&SecretString>) {
        self.hop_id.set(Some(saved.hop_id));
        self.host.set_text(&saved.host);
        self.port.set_value(f64::from(saved.port));
        self.local_port.set_value(f64::from(saved.local_port.unwrap_or(0)));
        self.user.set_text(&saved.username);
        self.system_client
            .set_active(saved.client == bookie_storage::SshClient::OpenSsh);
        match &saved.auth {
            _ if saved.agent => {
                self.auth_combo.set_selected(SSH_AUTH_AGENT);
                if let Some(identity) = &saved.agent_identity {
                    self.agent_key.set_text(&identity.to_string_lossy());
                }
            }
            SavedSshAuth::Password => self.auth_combo.set_selected(SSH_AUTH_PASSWORD),
            SavedSshAuth::PrivateKey { path, .. } => {
                self.auth_combo.set_selected(SSH_AUTH_KEY);
                self.key_path.set_text(&path.to_string_lossy());
            }
        }
        if let Some(password) = password {
            self.password.set_text(password.expose_secret());
        }
        if let Some(passphrase) = passphrase {
            self.passphrase.set_text(passphrase.expose_secret());
        }
        self.refresh_auth_visibility();
        self.expander.set_enable_expansion(true);
    }

    pub fn set_visible(&self, visible: bool) {
        self.group.set_visible(visible);
        if !visible {
            self.expander.set_enable_expansion(false);
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.expander.enables_expansion()
    }

    pub fn refresh_auth_visibility(&self) {
        let selected = self.auth_combo.selected();
        self.password.set_visible(selected == SSH_AUTH_PASSWORD);
        self.key_path.set_visible(selected == SSH_AUTH_KEY);
        self.passphrase.set_visible(selected == SSH_AUTH_KEY);
        self.agent_key.set_visible(selected == SSH_AUTH_AGENT);
    }

    fn collect_auth(&self, agent_auth: SshAuth) -> Result<(SshAuth, SavedSshAuth, SshSecretToStore), String> {
        Ok(match self.auth_combo.selected() {
            SSH_AUTH_AGENT => (
                agent_auth,
                SavedSshAuth::PrivateKey {
                    path: PathBuf::from(self.key_path.text().as_str()),
                    has_passphrase: false,
                },
                SshSecretToStore::None,
            ),
            SSH_AUTH_KEY => {
                let path = self.key_path.text().to_string();
                if path.trim().is_empty() {
                    return Err(bookie_app_services::tr!("Private key path is required"));
                }
                let path_buf = PathBuf::from(path);
                let raw_passphrase = self.passphrase.text().to_string();
                let has_passphrase = !raw_passphrase.is_empty();
                let auth = SshAuth::PrivateKey {
                    path: path_buf.clone(),
                    passphrase: if has_passphrase {
                        Some(SecretString::new(raw_passphrase.clone().into()))
                    } else {
                        None
                    },
                };
                let saved_auth = SavedSshAuth::PrivateKey {
                    path: path_buf,
                    has_passphrase,
                };
                let secret = if has_passphrase {
                    SshSecretToStore::Passphrase(SecretString::new(raw_passphrase.into()))
                } else {
                    SshSecretToStore::None
                };
                (auth, saved_auth, secret)
            }
            _ => {
                let raw_password = self.password.text().to_string();
                let auth = SshAuth::Password {
                    password: SecretString::new(raw_password.clone().into()),
                };
                let saved_auth = SavedSshAuth::Password;
                let secret = SshSecretToStore::Password(SecretString::new(raw_password.into()));
                (auth, saved_auth, secret)
            }
        })
    }

    pub fn collect(&self) -> Result<SshInputs, String> {
        let host = self.host.text().to_string();
        if host.trim().is_empty() {
            return Err(bookie_app_services::tr!("SSH host is required"));
        }
        let port: u16 = self.port.value() as u16;
        let username = self.user.text().to_string();
        if username.trim().is_empty() {
            return Err(bookie_app_services::tr!("SSH username is required"));
        }

        let agent = self.auth_combo.selected() == SSH_AUTH_AGENT;
        let (agent_auth, agent_identity) = agent_authentication(self.agent_key.text().as_str());
        let (auth, saved_auth, secret) = self.collect_auth(agent_auth)?;
        let head = SavedSshConfig {
            hop_id: self.hop_id.get().unwrap_or_else(uuid::Uuid::new_v4),
            credential_revision: 0,
            host: host.clone(),
            port,
            username: username.clone(),
            auth: saved_auth,
            jump: None,
            agent,
            client: if self.system_client.is_active() {
                bookie_storage::SshClient::OpenSsh
            } else {
                bookie_storage::SshClient::Builtin
            },
            agent_identity: if agent { agent_identity } else { None },
            local_port: local_port_setting(self.local_port.value()),
        };
        let chain = plan_chain(head, &self.jumps.collect(), self.jumps.previous().as_ref())
            .map_err(|error| chain_error_text(&error))?;
        Ok(SshInputs {
            cfg: SshConfig {
                host,
                port,
                username,
                auth,
                local_port: local_port_setting(self.local_port.value()),
            },
            saved: chain.head.clone(),
            secret_to_store: secret,
            chain,
        })
    }
}

impl SshInputs {
    pub async fn optional_route(
        inputs: Option<&Self>,
        connection_id: Option<uuid::Uuid>,
    ) -> Result<Option<bookie_transport::SshRoute>, String> {
        match inputs {
            Some(inputs) => inputs.route(connection_id).await.map(Some),
            None => Ok(None),
        }
    }

    pub async fn route(&self, connection_id: Option<uuid::Uuid>) -> Result<bookie_transport::SshRoute, String> {
        match self.saved.client {
            bookie_storage::SshClient::Builtin => self.builtin_hops(connection_id).await,
            bookie_storage::SshClient::OpenSsh => bookie_transport::openssh_config_for(&self.cfg)
                .map(bookie_transport::SshRoute::OpenSsh)
                .map_err(|error| error.to_string()),
        }
    }

    async fn builtin_hops(&self, connection_id: Option<uuid::Uuid>) -> Result<bookie_transport::SshRoute, String> {
        let jumps = bookie_app_services::ssh_chain_connect::resolve_jump_hops(connection_id, &self.chain)
            .await
            .map_err(|error| error.to_string())?;
        let mut hops = vec![self.cfg.clone()];
        hops.extend(jumps);
        if hops.len() > 1 {
            let local_port = self.cfg.local_port;
            hops[0].local_port = None;
            if let Some(last) = hops.last_mut() {
                last.local_port = local_port;
            }
        }
        Ok(bookie_transport::SshRoute::Builtin(hops))
    }
}

fn local_port_setting(value: f64) -> Option<u16> {
    match value as u16 {
        0 => None,
        port => Some(port),
    }
}

fn system_client_row() -> adw::SwitchRow {
    adw::SwitchRow::builder()
        .title(bookie_app_services::tr!("Use system OpenSSH"))
        .subtitle(bookie_app_services::tr!(
            "Run the ssh program so ~/.ssh/config, ProxyJump, ssh-agent and host certificates apply"
        ))
        .build()
}

fn default_ssh_key_path() -> String {
    let Some(home) = std::env::var_os("HOME") else {
        return String::new();
    };
    let home = PathBuf::from(home).join(".ssh");
    for candidate in ["id_ed25519", "id_rsa", "id_ecdsa"] {
        let path = home.join(candidate);
        if path.exists() {
            return path.to_string_lossy().into_owned();
        }
    }
    String::new()
}

fn agent_authentication(identity_text: &str) -> (SshAuth, Option<PathBuf>) {
    let identity = identity_text.trim();
    if identity.is_empty() {
        return (SshAuth::Agent, None);
    }
    let public_key = PathBuf::from(identity);
    (
        SshAuth::AgentIdentity {
            public_key: public_key.clone(),
        },
        Some(public_key),
    )
}

fn attach_key_browse_button(key_path: &adw::EntryRow, tooltip: &str, title: &str, patterns: &[&str]) {
    let button = gtk::Button::builder()
        .icon_name("document-open-symbolic")
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("flat");
    let entry = key_path.clone();
    let title = title.to_owned();
    let patterns: Vec<String> = patterns.iter().map(|pattern| (*pattern).to_owned()).collect();
    button.connect_clicked(move |btn| {
        let dialog = gtk::FileDialog::builder().title(title.as_str()).modal(true).build();
        // Filter to common SSH key filenames (id_ed25519, id_rsa,
        // id_ecdsa, *.pem, *.key) so the file picker hides irrelevant
        // entries — matches the pattern GNOME Settings uses for
        // certificate pickers.
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&bookie_app_services::tr!("SSH keys")));
        for pattern in &patterns {
            filter.add_pattern(pattern);
        }
        filter.add_mime_type("application/x-pem-file");
        let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
        dialog.set_default_filter(Some(&filter));

        if let Some(home) = std::env::var_os("HOME") {
            let ssh_dir = std::path::PathBuf::from(home).join(".ssh");
            if ssh_dir.exists() {
                dialog.set_initial_folder(Some(&gtk::gio::File::for_path(&ssh_dir)));
            }
        }
        let entry = entry.clone();
        let parent = btn.root().and_then(|r| r.downcast::<gtk::Window>().ok());
        dialog.open(parent.as_ref(), gtk::gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result
                && let Some(path) = file.path()
            {
                entry.set_text(&path.to_string_lossy());
            }
        });
    });
    key_path.add_suffix(&button);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_agent_key_field_offers_every_agent_key() {
        assert!(matches!(agent_authentication(""), (SshAuth::Agent, None)));
        assert!(matches!(agent_authentication("   "), (SshAuth::Agent, None)));
    }

    #[test]
    fn a_named_agent_key_is_the_only_one_offered_and_is_saved() {
        let (auth, saved) = agent_authentication(" /home/me/.ssh/id_ed25519.pub ");

        assert!(
            matches!(auth, SshAuth::AgentIdentity { public_key } if public_key.as_path() == std::path::Path::new("/home/me/.ssh/id_ed25519.pub"))
        );
        assert_eq!(saved, Some(PathBuf::from("/home/me/.ssh/id_ed25519.pub")));
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn the_agent_key_row_shows_only_for_agent_authentication_and_round_trips() {
        relm4::adw::init().unwrap();
        let section = SshSection::build();
        section.host.set_text("bastion");
        section.user.set_text("me");

        section.auth_combo.set_selected(SSH_AUTH_PASSWORD);
        section.refresh_auth_visibility();
        assert!(!section.agent_key.is_visible());
        section.auth_combo.set_selected(SSH_AUTH_AGENT);
        section.refresh_auth_visibility();
        assert!(section.agent_key.is_visible());

        section.agent_key.set_text("/home/me/.ssh/work.pub");
        let saved = section.collect().unwrap().saved;
        assert!(saved.agent);
        assert_eq!(saved.agent_identity, Some(PathBuf::from("/home/me/.ssh/work.pub")));

        let reopened = SshSection::build();
        reopened.prefill(&saved, None, None);
        assert_eq!(reopened.agent_key.text().as_str(), "/home/me/.ssh/work.pub");
        assert!(reopened.agent_key.is_visible());

        section.agent_key.set_text("");
        assert_eq!(section.collect().unwrap().saved.agent_identity, None);
    }

    #[test]
    fn zero_means_an_automatic_local_port() {
        assert_eq!(local_port_setting(0.0), None);
        assert_eq!(local_port_setting(15432.0), Some(15432));
    }
}
