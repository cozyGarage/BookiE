use bookie_gpui::keymap::{Chord, Command};
use bookie_gpui::theme::Theme;
use gpui::{Context, Entity, Keystroke, Window, div, prelude::*};
use gpui_kit::component::setting::{SettingField, SettingGroup, SettingItem};

use crate::GridView;
use crate::chrome::ghost;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsPage {
    Appearance,
    Fonts,
    Keyboard,
    Mcp,
}

const MODIFIER_KEYS: [&str; 6] = ["shift", "control", "alt", "platform", "function", "capslock"];

impl GridView {
    pub(crate) fn open_settings_page(&mut self, page: SettingsPage) {
        self.tabs.open_settings();
        self.settings_page = (page, self.settings_page.1 + 1);
        self.sync_grid_owner();
    }

    pub(crate) fn start_recording_shortcut(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.recording_shortcut = Some(command);
        self.status = format!("Press the new shortcut for {}, or Escape to cancel", command.title());
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn record_shortcut_key(&mut self, stroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        let Some(command) = self.recording_shortcut else {
            return false;
        };
        let key = stroke.key.as_str();
        if MODIFIER_KEYS.contains(&key) {
            return true;
        }
        self.recording_shortcut = None;
        if key == "escape" {
            self.status = "Shortcut unchanged".into();
            cx.notify();
            return true;
        }
        let modifiers = stroke.modifiers;
        let control = modifiers.control || (cfg!(target_os = "macos") && modifiers.platform);
        let chord = Chord::new(control, modifiers.shift, modifiers.alt, key);
        let displaced = self.keymap.rebind_taking(command, chord.clone());
        self.status = if displaced.is_empty() {
            format!("{} is now {}", command.title(), chord.label())
        } else {
            let names: Vec<&str> = displaced.iter().map(|owner| owner.title()).collect();
            format!(
                "{} is now {}; it was used by {}, which now has no shortcut",
                command.title(),
                chord.label(),
                names.join(", ")
            )
        };
        self.save_keymap();
        cx.notify();
        true
    }

    pub(crate) fn remove_shortcut(&mut self, command: Command, cx: &mut Context<Self>) {
        self.keymap.unbind(command);
        self.status = format!("{} has no shortcut", command.title());
        self.save_keymap();
        cx.notify();
    }

    pub(crate) fn reset_shortcut(&mut self, command: Command, cx: &mut Context<Self>) {
        self.keymap.reset(command);
        self.status = match self.keymap.binding(command) {
            Some(chord) => format!("{} is back to {}", command.title(), chord.label()),
            None => format!("{} has no default shortcut", command.title()),
        };
        self.save_keymap();
        cx.notify();
    }

    fn save_keymap(&mut self) {
        if cfg!(test) {
            return;
        }
        let Some(path) = bookie_app_services::config_io::xdg_config_path("gpui-keymap.json") else {
            return;
        };
        if let Err(error) = bookie_app_services::config_io::atomic_write_bytes(&path, self.keymap.overrides_json().as_bytes()) {
            tracing::warn!(%error, "could not save keyboard shortcuts");
            self.status = format!("Could not save keyboard shortcuts: {error}");
        }
    }

    pub(crate) fn keyboard_group(&self, view: Entity<Self>, theme: Theme) -> SettingGroup {
        Command::ALL.into_iter().fold(
            SettingGroup::new().title("Keyboard shortcuts"),
            |group, command| {
                let view = view.clone();
                group.item(SettingItem::new(
                    command.title(),
                    SettingField::render(move |_, _, cx| {
                        let current = view.read(cx);
                        let recording = current.recording_shortcut == Some(command);
                        let label = if recording {
                            "Press keys…".to_string()
                        } else {
                            current
                                .keymap
                                .binding(command)
                                .map_or_else(|| "None".to_string(), Chord::label)
                        };
                        let (record, remove, reset) = (view.clone(), view.clone(), view.clone());
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .debug_selector(move || format!("shortcut-{}", command.name()))
                                    .min_w(gpui::px(bookie_gpui::theme::size::SHORTCUT_CHORD_WIDTH))
                                    .child(label),
                            )
                            .child(ghost(("record-shortcut", command as usize), "Record", theme).on_click(
                                move |_, window, cx| {
                                    record.update(cx, |this, cx| this.start_recording_shortcut(command, window, cx));
                                },
                            ))
                            .child(ghost(("remove-shortcut", command as usize), "Remove", theme).on_click(
                                move |_, _, cx| remove.update(cx, |this, cx| this.remove_shortcut(command, cx)),
                            ))
                            .child(ghost(("reset-shortcut", command as usize), "Reset", theme).on_click(
                                move |_, _, cx| reset.update(cx, |this, cx| this.reset_shortcut(command, cx)),
                            ))
                    }),
                ))
            },
        )
    }
}
