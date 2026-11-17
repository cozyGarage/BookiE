use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{self as gtk, gdk, glib};

pub(crate) const MIN_LEVEL: i32 = -3;
pub(crate) const MAX_LEVEL: i32 = 8;

pub(crate) fn step(level: i32, delta: i32) -> i32 {
    level.saturating_add(delta).clamp(MIN_LEVEL, MAX_LEVEL)
}

pub(crate) fn css_class(level: i32) -> Option<String> {
    match level {
        0 => None,
        level if level < 0 => Some(format!("tp-zoom-m{}", level.abs())),
        level => Some(format!("tp-zoom-p{level}")),
    }
}

fn apply(root: &gtk::Widget, from: i32, to: i32) {
    if let Some(class) = css_class(from) {
        root.remove_css_class(&class);
    }
    if let Some(class) = css_class(to) {
        root.add_css_class(&class);
    }
}

fn key_delta(key: gdk::Key) -> Option<Option<i32>> {
    match key {
        gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => Some(Some(1)),
        gdk::Key::minus | gdk::Key::KP_Subtract => Some(Some(-1)),
        gdk::Key::_0 | gdk::Key::KP_0 => Some(None),
        _ => None,
    }
}

pub(crate) fn install(root: &impl IsA<gtk::Widget>) {
    let root = root.clone().upcast::<gtk::Widget>();
    let level = Rc::new(Cell::new(0));
    let change = {
        let root = root.clone();
        let level = level.clone();
        move |delta: Option<i32>| {
            let before = level.get();
            let after = delta.map_or(0, |delta| step(before, delta));
            if after != before {
                level.set(after);
                apply(&root, before, after);
            }
        }
    };

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let on_key = change.clone();
    keys.connect_key_pressed(move |_, key, _, state| {
        if !state.contains(gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        match key_delta(key) {
            Some(delta) => {
                on_key(delta);
                glib::Propagation::Stop
            }
            None => glib::Propagation::Proceed,
        }
    });
    root.add_controller(keys);

    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    let on_scroll = change;
    scroll.connect_scroll(move |controller, _, dy| {
        if !controller
            .current_event_state()
            .contains(gdk::ModifierType::CONTROL_MASK)
            || dy == 0.0
        {
            return glib::Propagation::Proceed;
        }
        on_scroll(Some(if dy < 0.0 { 1 } else { -1 }));
        glib::Propagation::Stop
    });
    root.add_controller(scroll);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_stay_inside_the_supported_range() {
        assert_eq!(step(0, 1), 1);
        assert_eq!(step(MAX_LEVEL, 1), MAX_LEVEL);
        assert_eq!(step(MIN_LEVEL, -1), MIN_LEVEL);
        assert_eq!(step(i32::MAX, 5), MAX_LEVEL);
    }

    #[test]
    fn every_level_has_one_class_except_the_default() {
        assert_eq!(css_class(0), None);
        assert_eq!(css_class(-2).as_deref(), Some("tp-zoom-m2"));
        assert_eq!(css_class(5).as_deref(), Some("tp-zoom-p5"));
        for level in MIN_LEVEL..=MAX_LEVEL {
            assert_eq!(css_class(level).is_none(), level == 0);
        }
    }

    #[test]
    fn plus_minus_and_zero_keys_map_to_zoom_changes() {
        assert_eq!(key_delta(gdk::Key::plus), Some(Some(1)));
        assert_eq!(key_delta(gdk::Key::equal), Some(Some(1)));
        assert_eq!(key_delta(gdk::Key::minus), Some(Some(-1)));
        assert_eq!(key_delta(gdk::Key::_0), Some(None));
        assert_eq!(key_delta(gdk::Key::a), None);
    }
}
