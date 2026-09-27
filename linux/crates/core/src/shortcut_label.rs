#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowseDrawnShortcuts {
    pub previous_cell: &'static str,
    pub next_cell: &'static str,
    pub toggle_boolean: &'static str,
    pub extend_selection: &'static str,
    pub extend_selection_chord: &'static str,
    pub toggle_selection: &'static str,
    pub toggle_selection_chord: &'static str,
}

pub fn browse_drawn_shortcuts() -> BrowseDrawnShortcuts {
    BrowseDrawnShortcuts {
        previous_cell: "KP_Left",
        next_cell: "KP_Right",
        toggle_boolean: "KP_Space",
        extend_selection: "Shift_L",
        extend_selection_chord: "Shift-click",
        toggle_selection: "Control_L",
        toggle_selection_chord: "Ctrl-click",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browse_shortcuts_do_not_draw_a_blank_key_or_a_raw_pointer_name() {
        let drawn = browse_drawn_shortcuts();
        let accelerators = [
            drawn.previous_cell,
            drawn.next_cell,
            drawn.toggle_boolean,
            drawn.extend_selection,
            drawn.toggle_selection,
        ];
        for accelerator in accelerators {
            assert_ne!(accelerator, "Left");
            assert_ne!(accelerator, "Right");
            assert_ne!(accelerator, "space");
            assert!(!accelerator.contains("Pointer_Button"));
        }
        assert_eq!(drawn.extend_selection_chord, "Shift-click");
        assert_eq!(drawn.toggle_selection_chord, "Ctrl-click");
    }
}
