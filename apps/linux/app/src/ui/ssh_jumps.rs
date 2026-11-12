use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use bookie_app_services::ssh_chain_form::{ChainFormError, JumpAuth, JumpHopForm, MAX_CHAIN_HOPS};
use bookie_storage::{SavedSshAuth, SavedSshConfig};
use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use secrecy::SecretString;
use uuid::Uuid;

const AUTH_PASSWORD: u32 = 0;
const AUTH_KEY: u32 = 1;
const AUTH_AGENT: u32 = 2;

#[derive(Debug, Clone)]
pub struct JumpPrefill {
    pub saved: SavedSshConfig,
    pub has_password: bool,
    pub has_passphrase: bool,
}

struct HopRow {
    row: adw::ExpanderRow,
    host: adw::EntryRow,
    port: adw::SpinRow,
    user: adw::EntryRow,
    auth: adw::ComboRow,
    password: adw::PasswordEntryRow,
    key_path: adw::EntryRow,
    agent_key: adw::EntryRow,
    passphrase: adw::PasswordEntryRow,
    up: gtk::Button,
    down: gtk::Button,
    remove: gtk::Button,
    hop_id: Cell<Option<Uuid>>,
    stored_password: Cell<bool>,
    stored_passphrase: Cell<bool>,
}

struct Inner {
    expander: adw::ExpanderRow,
    add_row: adw::ActionRow,
    notice: adw::ActionRow,
    rows: RefCell<Vec<Rc<HopRow>>>,
    previous: RefCell<Option<SavedSshConfig>>,
    on_change: RefCell<Option<Rc<dyn Fn()>>>,
    editable: Cell<bool>,
}

#[derive(Clone)]
pub struct SshJumps(Rc<Inner>);

impl SshJumps {
    pub fn build(expander: &adw::ExpanderRow) -> Self {
        let add_row = adw::ActionRow::builder()
            .title(bookie_app_services::tr!("Add jump host"))
            .subtitle(bookie_app_services::tr!(
                "Connect through another host before reaching the database"
            ))
            .activatable(true)
            .build();
        add_row.add_prefix(&gtk::Image::from_icon_name("list-add-symbolic"));
        let notice = adw::ActionRow::builder().use_markup(false).visible(false).build();
        notice.add_css_class("dim-label");
        let jumps = Self(Rc::new(Inner {
            expander: expander.clone(),
            add_row,
            notice,
            rows: RefCell::new(Vec::new()),
            previous: RefCell::new(None),
            on_change: RefCell::new(None),
            editable: Cell::new(true),
        }));
        let weak = Rc::downgrade(&jumps.0);
        jumps.0.add_row.connect_activated(move |_| {
            if let Some(inner) = weak.upgrade() {
                SshJumps(inner).add_hop(None);
            }
        });
        jumps
    }

    pub fn attach(&self) {
        self.rebuild();
    }

    pub fn follow_system_client(&self, system_client: bool) {
        let reason = if system_client {
            bookie_app_services::tr!(
                "Jump hosts need the built-in SSH client. Turn off “Use system OpenSSH” to edit them."
            )
        } else {
            String::new()
        };
        self.set_editable(!system_client, &reason);
    }

    pub fn connect_changed(&self, callback: impl Fn() + 'static) {
        *self.0.on_change.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn previous(&self) -> Option<SavedSshConfig> {
        self.0.previous.borrow().clone()
    }

    fn hop_count(&self) -> usize {
        self.0.rows.borrow().len()
    }

    pub fn prefill(&self, saved: &SavedSshConfig, hops: &[JumpPrefill]) {
        *self.0.previous.borrow_mut() = Some(saved.clone());
        self.clear_rows();
        for hop in hops {
            self.add_hop(Some(hop));
        }
        self.rebuild();
    }

    pub fn set_editable(&self, editable: bool, reason: &str) {
        self.0.editable.set(editable);
        self.0.notice.set_title(reason);
        self.0
            .notice
            .set_visible(!editable && (self.hop_count() > 0 || !reason.is_empty()));
        self.rebuild();
    }

    pub fn collect(&self) -> Vec<JumpHopForm> {
        self.0.rows.borrow().iter().map(|row| row.form()).collect()
    }

    fn clear_rows(&self) {
        for row in self.0.rows.borrow_mut().drain(..) {
            if row.row.parent().is_some() {
                self.0.expander.remove(&row.row);
            }
        }
    }

    fn add_hop(&self, prefill: Option<&JumpPrefill>) {
        if self.hop_count() + 1 >= MAX_CHAIN_HOPS {
            return;
        }
        let row = Rc::new(HopRow::build(prefill));
        self.wire(&row);
        self.0.rows.borrow_mut().push(row);
        self.rebuild();
        self.changed();
    }

    fn wire(&self, row: &Rc<HopRow>) {
        let weak = Rc::downgrade(&self.0);
        let target = Rc::downgrade(row);
        row.up.connect_clicked(self.mover(weak.clone(), target.clone(), -1));
        row.down.connect_clicked(self.mover(weak.clone(), target.clone(), 1));
        row.remove.connect_clicked(move |_| {
            if let (Some(inner), Some(row)) = (weak.upgrade(), target.upgrade()) {
                SshJumps(inner).remove_hop(&row);
            }
        });
        let notify = Rc::downgrade(&self.0);
        row.connect_edited(move || {
            if let Some(inner) = notify.upgrade() {
                SshJumps(inner).changed();
            }
        });
    }

    fn mover(&self, weak: Weak<Inner>, target: Weak<HopRow>, step: isize) -> impl Fn(&gtk::Button) + 'static {
        move |_| {
            if let (Some(inner), Some(row)) = (weak.upgrade(), target.upgrade()) {
                SshJumps(inner).move_hop(&row, step);
            }
        }
    }

    fn move_hop(&self, row: &Rc<HopRow>, step: isize) {
        let mut rows = self.0.rows.borrow_mut();
        let Some(index) = rows.iter().position(|candidate| Rc::ptr_eq(candidate, row)) else {
            return;
        };
        let Some(target) = index.checked_add_signed(step).filter(|target| *target < rows.len()) else {
            return;
        };
        rows.swap(index, target);
        drop(rows);
        self.rebuild();
        self.changed();
    }

    fn remove_hop(&self, row: &Rc<HopRow>) {
        self.0.rows.borrow_mut().retain(|candidate| !Rc::ptr_eq(candidate, row));
        if row.row.parent().is_some() {
            self.0.expander.remove(&row.row);
        }
        self.rebuild();
        self.changed();
    }

    fn rebuild(&self) {
        let inner = &self.0;
        for widget in [inner.notice.upcast_ref::<gtk::Widget>(), inner.add_row.upcast_ref()] {
            if widget.parent().is_some() {
                inner.expander.remove(widget);
            }
        }
        let rows = inner.rows.borrow();
        for row in rows.iter() {
            if row.row.parent().is_some() {
                inner.expander.remove(&row.row);
            }
        }
        inner.expander.add_row(&inner.notice);
        let last = rows.len().saturating_sub(1);
        let editable = inner.editable.get();
        for (index, row) in rows.iter().enumerate() {
            row.set_position(index, index == 0, index == last, editable);
            inner.expander.add_row(&row.row);
        }
        inner.add_row.set_sensitive(editable && rows.len() + 1 < MAX_CHAIN_HOPS);
        inner.expander.add_row(&inner.add_row);
    }

    fn changed(&self) {
        let callback = self.0.on_change.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
    }
}

impl HopRow {
    fn build(prefill: Option<&JumpPrefill>) -> Self {
        let row = adw::ExpanderRow::builder()
            .use_markup(false)
            .expanded(prefill.is_none())
            .build();
        let port = adw::SpinRow::with_range(1.0, 65535.0, 1.0);
        port.set_title(&bookie_app_services::tr!("Port"));
        port.set_value(22.0);
        let hop = Self {
            up: icon_button(
                "go-up-symbolic",
                &bookie_app_services::tr!("Move this jump host earlier"),
            ),
            down: icon_button(
                "go-down-symbolic",
                &bookie_app_services::tr!("Move this jump host later"),
            ),
            remove: icon_button(
                "user-trash-symbolic",
                &bookie_app_services::tr!("Remove this jump host"),
            ),
            row,
            host: entry_row(&bookie_app_services::tr!("Host")),
            port,
            user: entry_row(&bookie_app_services::tr!("Username")),
            auth: auth_row(),
            password: password_row(&bookie_app_services::tr!("Password")),
            key_path: entry_row(&bookie_app_services::tr!("Private key path")),
            agent_key: entry_row(&bookie_app_services::tr!("Agent key file (optional)")),
            passphrase: password_row(&bookie_app_services::tr!("Passphrase")),
            hop_id: Cell::new(None),
            stored_password: Cell::new(false),
            stored_passphrase: Cell::new(false),
        };
        hop.assemble();
        if let Some(prefill) = prefill {
            hop.apply(prefill);
        }
        hop.refresh_visibility();
        hop
    }

    fn assemble(&self) {
        for button in [&self.up, &self.down, &self.remove] {
            self.row.add_suffix(button);
        }
        self.row.add_row(&self.host);
        self.row.add_row(&self.port);
        self.row.add_row(&self.user);
        self.row.add_row(&self.auth);
        self.row.add_row(&self.password);
        self.row.add_row(&self.key_path);
        self.row.add_row(&self.agent_key);
        self.row.add_row(&self.passphrase);
        let row = self.row.clone();
        self.host
            .connect_changed(move |entry| row.set_subtitle(entry.text().as_str()));
    }

    fn apply(&self, prefill: &JumpPrefill) {
        let saved = &prefill.saved;
        self.hop_id.set(Some(saved.hop_id));
        self.host.set_text(&saved.host);
        self.port.set_value(f64::from(saved.port));
        self.user.set_text(&saved.username);
        self.stored_password.set(prefill.has_password);
        self.stored_passphrase.set(prefill.has_passphrase);
        let kept = bookie_app_services::tr!("Saved in the keyring. Leave empty to keep it.");
        match &saved.auth {
            _ if saved.agent => {
                self.auth.set_selected(AUTH_AGENT);
                if let Some(identity) = &saved.agent_identity {
                    self.agent_key.set_text(&identity.to_string_lossy());
                }
            }
            SavedSshAuth::Password => {
                self.auth.set_selected(AUTH_PASSWORD);
                if prefill.has_password {
                    self.password.set_title(&kept);
                }
            }
            SavedSshAuth::PrivateKey { path, .. } => {
                self.auth.set_selected(AUTH_KEY);
                self.key_path.set_text(&path.to_string_lossy());
                if prefill.has_passphrase {
                    self.passphrase.set_title(&kept);
                }
            }
        }
    }

    fn refresh_visibility(&self) {
        let selected = self.auth.selected();
        self.password.set_visible(selected == AUTH_PASSWORD);
        self.key_path.set_visible(selected == AUTH_KEY);
        self.passphrase.set_visible(selected == AUTH_KEY);
        self.agent_key.set_visible(selected == AUTH_AGENT);
    }

    fn set_position(&self, index: usize, first: bool, last: bool, editable: bool) {
        self.row
            .set_title(&bookie_app_services::tr!("Jump host {number}").replace("{number}", &(index + 2).to_string()));
        self.up.set_sensitive(editable && !first);
        self.down.set_sensitive(editable && !last);
        self.remove.set_sensitive(editable);
        self.row.set_sensitive(editable);
    }

    fn connect_edited(self: &Rc<Self>, callback: impl Fn() + 'static) {
        let callback: Rc<dyn Fn()> = Rc::new(callback);
        let weak = Rc::downgrade(self);
        let on_auth = callback.clone();
        self.auth.connect_selected_notify(move |_| {
            if let Some(row) = weak.upgrade() {
                row.refresh_visibility();
            }
            on_auth();
        });
        for entry in [&self.host, &self.user, &self.key_path, &self.agent_key] {
            let on_edit = callback.clone();
            entry.connect_changed(move |_| on_edit());
        }
        for entry in [&self.password, &self.passphrase] {
            let on_edit = callback.clone();
            entry.connect_changed(move |_| on_edit());
        }
        let on_port = callback;
        self.port.connect_value_notify(move |_| on_port());
    }

    fn form(&self) -> JumpHopForm {
        let secret = |entry: &adw::PasswordEntryRow| {
            let text = entry.text();
            (!text.is_empty()).then(|| SecretString::new(text.to_string().into()))
        };
        JumpHopForm {
            hop_id: self.hop_id.get(),
            host: self.host.text().to_string(),
            port: self.port.value() as u16,
            username: self.user.text().to_string(),
            auth: match self.auth.selected() {
                AUTH_KEY => JumpAuth::PrivateKey,
                AUTH_AGENT => JumpAuth::Agent,
                _ => JumpAuth::Password,
            },
            key_path: self.key_path.text().to_string(),
            agent_identity: self.agent_key.text().to_string(),
            password: secret(&self.password),
            passphrase: secret(&self.passphrase),
            stored_password: self.stored_password.get(),
            stored_passphrase: self.stored_passphrase.get(),
        }
    }
}

fn entry_row(title: &str) -> adw::EntryRow {
    adw::EntryRow::builder().title(title).build()
}

fn password_row(title: &str) -> adw::PasswordEntryRow {
    adw::PasswordEntryRow::builder().title(title).build()
}

fn auth_row() -> adw::ComboRow {
    let methods = [
        bookie_app_services::tr!("Password"),
        bookie_app_services::tr!("Private key"),
        bookie_app_services::tr!("SSH agent"),
    ];
    adw::ComboRow::builder()
        .title(bookie_app_services::tr!("Authentication"))
        .model(&gtk::StringList::new(&[&methods[0], &methods[1], &methods[2]]))
        .build()
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("flat");
    button.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    button
}

pub fn chain_error_text(error: &ChainFormError) -> String {
    match error {
        ChainFormError::EmptyHost { position } => {
            numbered(bookie_app_services::tr!("SSH host {number} is required"), *position)
        }
        ChainFormError::InvalidPort { position } => numbered(
            bookie_app_services::tr!("SSH host {number} needs a port between 1 and 65535"),
            *position,
        ),
        ChainFormError::EmptyUsername { position } => numbered(
            bookie_app_services::tr!("SSH host {number} needs a username"),
            *position,
        ),
        ChainFormError::EmptyKeyPath { position } => numbered(
            bookie_app_services::tr!("SSH host {number} needs a private key path"),
            *position,
        ),
        ChainFormError::PasswordRequired { position } => numbered(
            bookie_app_services::tr!("SSH host {number} needs a password"),
            *position,
        ),
        ChainFormError::RepeatedHop { position } => numbered(
            bookie_app_services::tr!("SSH host {number} repeats a host that is already in the chain"),
            *position,
        ),
        ChainFormError::TooManyHops { limit } => {
            bookie_app_services::tr!("An SSH chain can have at most {limit} hosts")
                .replace("{limit}", &limit.to_string())
        }
        ChainFormError::OpenSshRefusesJumpHops => bookie_app_services::tr!(
            "The system OpenSSH client cannot use saved jump hosts. Remove them or turn the client off."
        ),
    }
}

fn numbered(template: String, position: usize) -> String {
    template.replace("{number}", &position.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_app_services::ssh_chain_form::plan_chain;
    use bookie_storage::SshClient;
    use secrecy::ExposeSecret;

    fn hop(host: &str, revision: u64, auth: SavedSshAuth) -> SavedSshConfig {
        SavedSshConfig {
            hop_id: Uuid::new_v4(),
            credential_revision: revision,
            host: host.into(),
            port: 22,
            username: "ops".into(),
            auth,
            jump: None,
            client: SshClient::Builtin,
            agent: false,
            agent_identity: None,
            local_port: None,
        }
    }

    fn editor() -> (adw::ExpanderRow, SshJumps) {
        relm4::adw::init().unwrap();
        let expander = adw::ExpanderRow::new();
        let jumps = SshJumps::build(&expander);
        jumps.attach();
        (expander, jumps)
    }

    fn three_hop_chain() -> (SavedSshConfig, Vec<JumpPrefill>) {
        let mut head = hop("edge", 0, SavedSshAuth::Password);
        let middle = hop(
            "middle",
            1,
            SavedSshAuth::PrivateKey {
                path: "/home/me/.ssh/id_ed25519".into(),
                has_passphrase: true,
            },
        );
        let inner = hop("inner", 2, SavedSshAuth::Password);
        let hops = vec![
            JumpPrefill {
                saved: middle.clone(),
                has_password: false,
                has_passphrase: true,
            },
            JumpPrefill {
                saved: inner.clone(),
                has_password: true,
                has_passphrase: false,
            },
        ];
        head.jump = Some(Box::new(SavedSshConfig {
            jump: Some(Box::new(inner)),
            ..middle
        }));
        (head, hops)
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn a_saved_chain_loads_into_the_editor_and_saves_back_unchanged_without_showing_secrets() {
        let (_expander, jumps) = editor();
        let (head, hops) = three_hop_chain();
        jumps.prefill(&head, &hops);
        let forms = jumps.collect();
        assert_eq!(forms.len(), 2);
        assert!(
            forms
                .iter()
                .all(|form| form.password.is_none() && form.passphrase.is_none())
        );
        assert!(forms[0].stored_passphrase && forms[1].stored_password);
        let plan = plan_chain(head.clone(), &forms, jumps.previous().as_ref()).unwrap();
        assert_eq!(plan.head, head);
        assert!(plan.writes.is_empty() && plan.obsolete.is_empty());
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn hosts_can_be_added_reordered_and_removed_and_the_order_is_the_chain_order() {
        let (_expander, jumps) = editor();
        jumps.add_hop(None);
        jumps.add_hop(None);
        let rows: Vec<Rc<HopRow>> = jumps.0.rows.borrow().iter().cloned().collect();
        rows[0].host.set_text("first");
        rows[1].host.set_text("second");
        jumps.move_hop(&rows[1], -1);
        let hosts: Vec<String> = jumps.collect().into_iter().map(|form| form.host).collect();
        assert_eq!(hosts, vec!["second", "first"]);
        jumps.remove_hop(&rows[1]);
        let hosts: Vec<String> = jumps.collect().into_iter().map(|form| form.host).collect();
        assert_eq!(hosts, vec!["first"]);
        assert_eq!(jumps.0.rows.borrow()[0].row.title().as_str(), "Jump host 2");
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn a_typed_secret_is_collected_for_storage_and_an_untouched_row_collects_none() {
        let (_expander, jumps) = editor();
        let (head, hops) = three_hop_chain();
        jumps.prefill(&head, &hops);
        let rows: Vec<Rc<HopRow>> = jumps.0.rows.borrow().iter().cloned().collect();
        rows[1].password.set_text("rotated");
        let forms = jumps.collect();
        assert!(forms[0].passphrase.is_none());
        assert_eq!(forms[1].password.as_ref().unwrap().expose_secret(), "rotated");
        let plan = plan_chain(head, &forms, jumps.previous().as_ref()).unwrap();
        assert_eq!(plan.writes.len(), 1);
        assert_eq!(plan.writes[0].secret.revision, 3);
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn the_system_openssh_client_disables_editing_with_a_reason_and_restores_it() {
        let (_expander, jumps) = editor();
        let (head, hops) = three_hop_chain();
        jumps.prefill(&head, &hops);
        jumps.follow_system_client(true);
        assert!(!jumps.0.add_row.is_sensitive());
        assert!(jumps.0.notice.is_visible());
        assert!(!jumps.0.notice.title().is_empty());
        assert!(jumps.0.rows.borrow().iter().all(|row| !row.row.is_sensitive()));
        jumps.follow_system_client(false);
        assert!(jumps.0.add_row.is_sensitive());
        assert!(!jumps.0.notice.is_visible());
        assert!(jumps.0.rows.borrow().iter().all(|row| row.row.is_sensitive()));
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn adding_stops_at_the_chain_limit() {
        let (_expander, jumps) = editor();
        for _ in 0..MAX_CHAIN_HOPS + 2 {
            jumps.add_hop(None);
        }
        assert_eq!(jumps.hop_count() + 1, MAX_CHAIN_HOPS);
        assert!(!jumps.0.add_row.is_sensitive());
    }

    #[test]
    fn each_chain_error_has_a_readable_message_naming_its_host() {
        let text = chain_error_text(&ChainFormError::PasswordRequired { position: 3 });
        assert!(text.contains('3'));
        assert!(!text.contains('{'));
        assert!(!chain_error_text(&ChainFormError::OpenSshRefusesJumpHops).is_empty());
    }
}
