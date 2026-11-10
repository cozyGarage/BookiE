use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Command {
    Palette,
    Filter,
    CopyCells,
    CopyCellsWithHeaders,
    ToggleSidebar,
    NewQuery,
    RunQuery,
    ToggleDetail,
}

impl Command {
    pub const ALL: [Command; 8] = [
        Command::Palette,
        Command::Filter,
        Command::CopyCells,
        Command::CopyCellsWithHeaders,
        Command::ToggleSidebar,
        Command::NewQuery,
        Command::RunQuery,
        Command::ToggleDetail,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Command::Palette => "palette",
            Command::Filter => "filter",
            Command::CopyCells => "copy",
            Command::CopyCellsWithHeaders => "copy-with-headers",
            Command::ToggleSidebar => "toggle-sidebar",
            Command::NewQuery => "new-query",
            Command::RunQuery => "run-query",
            Command::ToggleDetail => "toggle-detail",
        }
    }

    fn parse(name: &str) -> Option<Command> {
        Command::ALL.into_iter().find(|command| command.name() == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Chord {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: String,
}

impl Chord {
    pub fn new(ctrl: bool, shift: bool, alt: bool, key: &str) -> Chord {
        Chord {
            ctrl,
            shift,
            alt,
            key: key.to_ascii_lowercase(),
        }
    }

    pub fn parse(text: &str) -> Result<Chord, String> {
        let mut chord = Chord::new(false, false, false, "");
        let parts: Vec<&str> = text.split('-').collect();
        let (key, modifiers) = parts.split_last().ok_or("empty shortcut")?;
        for modifier in modifiers {
            match modifier.to_ascii_lowercase().as_str() {
                "ctrl" => chord.ctrl = true,
                "shift" => chord.shift = true,
                "alt" => chord.alt = true,
                other => return Err(format!("unknown modifier {other}")),
            }
        }
        if key.is_empty() {
            return Err("a shortcut needs a key".into());
        }
        chord.key = key.to_ascii_lowercase();
        let function_key = chord.key.len() >= 2
            && chord.key.starts_with('f')
            && chord.key[1..].chars().all(|digit| digit.is_ascii_digit());
        if !(chord.ctrl || chord.alt || function_key) {
            return Err("a shortcut needs ctrl, alt or a function key so typing is not captured".into());
        }
        Ok(chord)
    }

    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("ctrl".to_string());
        }
        if self.shift {
            parts.push("shift".to_string());
        }
        if self.alt {
            parts.push("alt".to_string());
        }
        parts.push(self.key.clone());
        parts.join("-")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebindError {
    Taken(Command),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    bindings: BTreeMap<Command, Chord>,
}

impl Default for Keymap {
    fn default() -> Self {
        let chord = |ctrl, shift, key| Chord::new(ctrl, shift, false, key);
        Keymap {
            bindings: BTreeMap::from([
                (Command::Palette, chord(true, true, "p")),
                (Command::Filter, chord(true, false, "f")),
                (Command::CopyCells, chord(true, false, "c")),
                (Command::CopyCellsWithHeaders, chord(true, true, "c")),
                (Command::ToggleSidebar, chord(true, false, "b")),
                (Command::NewQuery, chord(true, false, "t")),
                (Command::RunQuery, chord(true, false, "enter")),
                (Command::ToggleDetail, chord(false, false, "f3")),
            ]),
        }
    }
}

impl Keymap {
    pub fn resolve(&self, chord: &Chord) -> Option<Command> {
        self.bindings
            .iter()
            .find(|(_, bound)| *bound == chord)
            .map(|(command, _)| *command)
    }

    pub fn binding(&self, command: Command) -> Option<&Chord> {
        self.bindings.get(&command)
    }

    pub fn rebind(&mut self, command: Command, chord: Chord) -> Result<(), RebindError> {
        if let Some(owner) = self.resolve(&chord).filter(|owner| *owner != command) {
            return Err(RebindError::Taken(owner));
        }
        self.bindings.insert(command, chord);
        Ok(())
    }

    pub fn from_overrides(json: &str) -> Result<Keymap, String> {
        let overrides: BTreeMap<String, String> = serde_json::from_str(json).map_err(|error| error.to_string())?;
        let mut keymap = Keymap::default();
        let mut wanted = Vec::new();
        for (name, text) in overrides {
            let command = Command::parse(&name).ok_or_else(|| format!("unknown command {name}"))?;
            wanted.push((command, Chord::parse(&text)?));
        }
        for (command, _) in &wanted {
            keymap.bindings.remove(command);
        }
        for (command, chord) in wanted {
            keymap
                .rebind(command, chord.clone())
                .map_err(|RebindError::Taken(owner)| {
                    format!("{} is already used by {}", chord.label(), owner.name())
                })?;
        }
        Ok(keymap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_map_resolves_every_command_to_a_distinct_chord() {
        let keymap = Keymap::default();
        for command in Command::ALL {
            let chord = keymap.binding(command).expect("bound");
            assert_eq!(keymap.resolve(chord), Some(command));
        }
        assert_eq!(
            keymap.resolve(&Chord::parse("ctrl-shift-p").unwrap()),
            Some(Command::Palette)
        );
        assert_eq!(
            keymap.resolve(&Chord::parse("f3").unwrap()),
            Some(Command::ToggleDetail)
        );
    }

    #[test]
    fn chords_parse_and_refuse_keys_that_would_capture_typing() {
        assert_eq!(Chord::parse("Ctrl-Alt-K").unwrap(), Chord::new(true, false, true, "k"));
        assert!(Chord::parse("k").is_err());
        assert!(Chord::parse("shift-k").is_err());
        assert!(Chord::parse("ctrl-").is_err());
        assert!(Chord::parse("hyper-k").is_err());
        assert_eq!(Chord::parse("ctrl-shift-p").unwrap().label(), "ctrl-shift-p");
    }

    #[test]
    fn rebinding_moves_a_command_and_refuses_a_taken_chord() {
        let mut keymap = Keymap::default();
        keymap
            .rebind(Command::ToggleSidebar, Chord::parse("alt-b").unwrap())
            .unwrap();
        assert_eq!(keymap.resolve(&Chord::parse("ctrl-b").unwrap()), None);
        assert_eq!(
            keymap.resolve(&Chord::parse("alt-b").unwrap()),
            Some(Command::ToggleSidebar)
        );
        assert_eq!(
            keymap.rebind(Command::NewQuery, Chord::parse("ctrl-f").unwrap()),
            Err(RebindError::Taken(Command::Filter))
        );
        assert_eq!(
            keymap.binding(Command::NewQuery),
            Some(&Chord::parse("ctrl-t").unwrap())
        );
    }

    #[test]
    fn overrides_swap_bindings_and_bad_files_are_errors() {
        let swapped = Keymap::from_overrides(r#"{"filter":"ctrl-b","toggle-sidebar":"ctrl-f"}"#).unwrap();
        assert_eq!(swapped.resolve(&Chord::parse("ctrl-b").unwrap()), Some(Command::Filter));
        assert_eq!(
            swapped.resolve(&Chord::parse("ctrl-f").unwrap()),
            Some(Command::ToggleSidebar)
        );
        assert!(Keymap::from_overrides(r#"{"nope":"ctrl-b"}"#).is_err());
        assert!(Keymap::from_overrides(r#"{"filter":"ctrl-t"}"#).is_err());
        assert!(Keymap::from_overrides(r#"{"filter":"x"}"#).is_err());
        assert!(Keymap::from_overrides("not json").is_err());
        assert_eq!(Keymap::from_overrides("{}").unwrap(), Keymap::default());
    }
}
