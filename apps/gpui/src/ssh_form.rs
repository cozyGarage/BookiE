use bookie_app_services::connection_save::{SshAuthKind, SshDraft, SshHopDraft};
use gpui::{Context, Div, Entity, prelude::*};
use secrecy::SecretString;

use crate::GridView;
use crate::connection_form::{choice_chip, choice_row, field, note};
use crate::text_input::TextInput;
use bookie_gpui::theme::Theme;

pub(crate) struct HopForm {
    pub hop_id: Option<uuid::Uuid>,
    pub host: Entity<TextInput>,
    pub port: Entity<TextInput>,
    pub user: Entity<TextInput>,
    pub key: Entity<TextInput>,
    pub identity: Entity<TextInput>,
    pub secret: Entity<TextInput>,
    pub auth: SshAuthKind,
    pub stored: bool,
}

impl HopForm {
    pub fn new(cx: &mut Context<GridView>) -> Self {
        let input = |placeholder: &str, cx: &mut Context<GridView>| cx.new(|cx| TextInput::new(placeholder, cx));
        Self {
            hop_id: None,
            host: input("Host", cx),
            port: input("22", cx),
            user: input("User", cx),
            key: input("Private key file", cx),
            identity: input("Agent key file (optional)", cx),
            secret: cx.new(|cx| TextInput::new("Password or key passphrase", cx).masked()),
            auth: SshAuthKind::Password,
            stored: false,
        }
    }

    fn draft(&self, cx: &gpui::App) -> SshHopDraft {
        let text = |input: &Entity<TextInput>| input.read(cx).content().to_string();
        SshHopDraft {
            hop_id: self.hop_id,
            host: text(&self.host),
            port: text(&self.port).trim().parse().unwrap_or(22),
            username: text(&self.user),
            auth: self.auth,
            key_path: text(&self.key),
            agent_identity: text(&self.identity),
            secret: SecretString::from(text(&self.secret)),
            stored_secret: self.stored,
        }
    }

    fn fill(&mut self, hop: &SshHopDraft, cx: &mut Context<GridView>) {
        self.hop_id = hop.hop_id;
        self.auth = hop.auth;
        self.stored = hop.stored_secret;
        let port = hop.port.to_string();
        for (input, text) in [
            (&self.host, hop.host.as_str()),
            (&self.port, port.as_str()),
            (&self.user, hop.username.as_str()),
            (&self.key, hop.key_path.as_str()),
            (&self.identity, hop.agent_identity.as_str()),
            (&self.secret, ""),
        ] {
            input.update(cx, |input, cx| input.set_content(text, cx));
        }
    }
}

pub(crate) struct SshForm {
    pub on: bool,
    pub openssh: bool,
    pub head: HopForm,
    pub jumps: Vec<HopForm>,
}

impl SshForm {
    pub fn new(cx: &mut Context<GridView>) -> Self {
        Self {
            on: false,
            openssh: false,
            head: HopForm::new(cx),
            jumps: Vec::new(),
        }
    }

    pub fn draft(&self, cx: &gpui::App) -> Option<SshDraft> {
        self.on.then(|| SshDraft {
            head: self.head.draft(cx),
            openssh: self.openssh,
            jumps: self.jumps.iter().map(|jump| jump.draft(cx)).collect(),
        })
    }

    pub fn fill(&mut self, draft: Option<&SshDraft>, cx: &mut Context<GridView>) {
        self.on = draft.is_some();
        self.jumps.clear();
        let Some(draft) = draft else {
            return;
        };
        self.openssh = draft.openssh;
        self.head.fill(&draft.head, cx);
        for hop in &draft.jumps {
            let mut jump = HopForm::new(cx);
            jump.fill(hop, cx);
            self.jumps.push(jump);
        }
    }
}

#[derive(Clone, Copy)]
enum Which {
    Head,
    Jump(usize),
}

impl GridView {
    pub(crate) fn ssh_fields(&self, theme: Theme, cx: &mut Context<Self>) -> Vec<Div> {
        let ssh = &self.connection_form.ssh;
        let toggle = choice_row(
            theme,
            "SSH tunnel",
            [choice_chip("ssh-on", if ssh.on { "On" } else { "Off" }, ssh.on, theme).on_click(cx.listener(
                |this, _, _, cx| {
                    this.connection_form.ssh.on = !this.connection_form.ssh.on;
                    cx.notify();
                },
            ))],
        );
        let mut rows = vec![toggle];
        if !ssh.on {
            return rows;
        }
        rows.extend(self.hop_rows(&ssh.head, Which::Head, "SSH", theme, cx));
        rows.push(choice_row(
            theme,
            "Client",
            [choice_chip(
                "ssh-openssh",
                if ssh.openssh { "System OpenSSH" } else { "Built in" },
                ssh.openssh,
                theme,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.connection_form.ssh.openssh = !this.connection_form.ssh.openssh;
                cx.notify();
            }))],
        ));
        for (index, jump) in ssh.jumps.iter().enumerate() {
            rows.extend(self.hop_rows(jump, Which::Jump(index), &format!("Jump {}", index + 1), theme, cx));
        }
        if ssh.openssh && !ssh.jumps.is_empty() {
            rows.push(note(theme, "The system OpenSSH client cannot use saved jump hosts."));
        }
        rows.push(choice_row(
            theme,
            "Jump hosts",
            [choice_chip("ssh-add-jump", "Add jump host", false, theme).on_click(cx.listener(|this, _, _, cx| {
                let jump = HopForm::new(cx);
                this.connection_form.ssh.jumps.push(jump);
                cx.notify();
            }))],
        ));
        rows
    }

    fn hop_rows(&self, hop: &HopForm, which: Which, label: &str, theme: Theme, cx: &mut Context<Self>) -> Vec<Div> {
        let (id, header) = match which {
            Which::Head => ("hop-head".to_owned(), None),
            Which::Jump(index) => (format!("hop-{index}"), Some(index)),
        };
        let name = |suffix: &str| gpui::SharedString::from(format!("{label} {suffix}"));
        let mut rows = Vec::new();
        if let Some(index) = header {
            rows.push(choice_row(
                theme,
                gpui::SharedString::from(label.to_owned()),
                [choice_chip(("hop-remove", index), "Remove", false, theme).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.connection_form.ssh.jumps.remove(index);
                        cx.notify();
                    },
                ))],
            ));
        }
        rows.push(field(theme, name("host"), &hop.host));
        rows.push(field(theme, name("port"), &hop.port));
        rows.push(field(theme, name("user"), &hop.user));
        rows.push(self.auth_chips(hop.auth, which, &id, theme, cx));
        match hop.auth {
            SshAuthKind::Password => rows.push(field(theme, name("password"), &hop.secret)),
            SshAuthKind::KeyFile => {
                rows.push(field(theme, name("key file"), &hop.key));
                rows.push(field(theme, name("passphrase"), &hop.secret));
            }
            SshAuthKind::Agent => rows.push(field(theme, name("agent key"), &hop.identity)),
        }
        rows
    }

    fn auth_chips(&self, current: SshAuthKind, which: Which, id: &str, theme: Theme, cx: &mut Context<Self>) -> Div {
        let kinds = [
            (SshAuthKind::Password, "Password"),
            (SshAuthKind::KeyFile, "Key file"),
            (SshAuthKind::Agent, "Agent"),
        ];
        let chips = kinds.into_iter().enumerate().map(|(index, (kind, text))| {
            let chip_id = gpui::ElementId::from((gpui::SharedString::from(id.to_owned()), index));
            choice_chip(chip_id, text, current == kind, theme).on_click(cx.listener(move |this, _, _, cx| {
                let ssh = &mut this.connection_form.ssh;
                let hop = match which {
                    Which::Head => &mut ssh.head,
                    Which::Jump(position) => &mut ssh.jumps[position],
                };
                hop.auth = kind;
                cx.notify();
            }))
        });
        choice_row(theme, "Sign in", chips)
    }
}
