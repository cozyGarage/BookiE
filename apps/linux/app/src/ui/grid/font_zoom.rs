use std::cell::{Cell, RefCell};

use gtk4::prelude::*;
use gtk4::{self as gtk, gio, glib};

pub(super) const CSS_CLASS: &str = "tp-result-grid-font";
pub(super) const DEFAULT_PT: u32 = 11;
pub(super) const MIN_PT: u32 = 8;
pub(super) const MAX_PT: u32 = 24;

thread_local! {
    static SESSION_PT: Cell<u32> = const { Cell::new(DEFAULT_PT) };
    static PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    static SKIP_WIDTH_SAVE: Cell<bool> = const { Cell::new(false) };
    static LIVE: RefCell<Vec<glib::WeakRef<gtk::ColumnView>>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn width_save_suppressed() -> bool {
    SKIP_WIDTH_SAVE.get()
}

pub(super) fn session_pt() -> u32 {
    SESSION_PT.get()
}

pub(super) fn clamp_pt(pt: u32) -> u32 {
    pt.clamp(MIN_PT, MAX_PT)
}

pub(super) fn step_in(pt: u32) -> u32 {
    clamp_pt(pt.saturating_add(1))
}

pub(super) fn step_out(pt: u32) -> u32 {
    clamp_pt(pt.saturating_sub(1))
}

pub(super) fn grid_font_css(pt: u32) -> String {
    let pt = clamp_pt(pt);
    format!(".{CSS_CLASS}, .{CSS_CLASS} label {{ font-size: {pt}pt; }}")
}

pub(super) fn scaled_fixed_width(width: i32, from_pt: u32, to_pt: u32) -> i32 {
    if width <= 0 || from_pt == 0 || from_pt == to_pt {
        return width;
    }
    let scaled = f64::from(width) * f64::from(to_pt) / f64::from(from_pt);
    scaled.round().clamp(1.0, f64::from(i32::MAX)) as i32
}

pub(super) fn install(view: &gtk::ColumnView, actions: &gio::SimpleActionGroup) {
    view.add_css_class(CSS_CLASS);
    register_live(view);
    let pt = session_pt();
    apply_css(pt);
    if pt != DEFAULT_PT {
        SKIP_WIDTH_SAVE.set(true);
        scale_columns(view, DEFAULT_PT, pt);
        SKIP_WIDTH_SAVE.set(false);
    }
    add_action(actions, view, "zoom-in", ZoomStep::In);
    add_action(actions, view, "zoom-out", ZoomStep::Out);
    add_action(actions, view, "zoom-reset", ZoomStep::Reset);
    let controller = gtk::ShortcutController::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    for (trigger, action) in [
        ("<Primary>plus", "grid.zoom-in"),
        ("<Primary>equal", "grid.zoom-in"),
        ("<Primary>KP_Add", "grid.zoom-in"),
        ("<Primary>minus", "grid.zoom-out"),
        ("<Primary>KP_Subtract", "grid.zoom-out"),
        ("<Primary>0", "grid.zoom-reset"),
        ("<Primary>KP_0", "grid.zoom-reset"),
    ] {
        controller.add_shortcut(
            gtk::Shortcut::builder()
                .trigger(&crate::ui::shortcut::parse(trigger))
                .action(&gtk::NamedAction::new(action))
                .build(),
        );
    }
    view.add_controller(controller);
}

#[derive(Clone, Copy)]
enum ZoomStep {
    In,
    Out,
    Reset,
}

fn add_action(actions: &gio::SimpleActionGroup, view: &gtk::ColumnView, name: &str, step: ZoomStep) {
    let action = gio::SimpleAction::new(name, None);
    let weak = view.downgrade();
    action.connect_activate(move |_, _| {
        if weak.upgrade().filter(|view| view.is_mapped()).is_none() {
            return;
        }
        apply_step(step);
    });
    actions.add_action(&action);
}

fn apply_step(step: ZoomStep) {
    let before = session_pt();
    let after = match step {
        ZoomStep::In => step_in(before),
        ZoomStep::Out => step_out(before),
        ZoomStep::Reset => DEFAULT_PT,
    };
    if after == before {
        return;
    }
    SESSION_PT.set(after);
    apply_css(after);
    remeasure_live(before, after);
}

fn apply_css(pt: u32) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    PROVIDER.with(|cell| {
        if let Some(prev) = cell.borrow_mut().take() {
            gtk::style_context_remove_provider_for_display(&display, &prev);
        }
        let provider = gtk::CssProvider::new();
        provider.load_from_string(&grid_font_css(pt));
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        *cell.borrow_mut() = Some(provider);
    });
}

fn register_live(view: &gtk::ColumnView) {
    LIVE.with(|cell| {
        let mut live = cell.borrow_mut();
        live.retain(|weak| weak.upgrade().is_some());
        live.push(view.downgrade());
    });
}

fn remeasure_live(from_pt: u32, to_pt: u32) {
    let views = LIVE.with(|cell| {
        let mut live = cell.borrow_mut();
        live.retain(|weak| weak.upgrade().is_some());
        live.iter().filter_map(|weak| weak.upgrade()).collect::<Vec<_>>()
    });
    SKIP_WIDTH_SAVE.set(true);
    for view in views {
        scale_columns(&view, from_pt, to_pt);
    }
    SKIP_WIDTH_SAVE.set(false);
}

fn scale_columns(view: &gtk::ColumnView, from_pt: u32, to_pt: u32) {
    let columns = view.columns();
    for position in 0..columns.n_items() {
        let Some(column) = columns
            .item(position)
            .and_then(|item| item.downcast::<gtk::ColumnViewColumn>().ok())
        else {
            continue;
        };
        let width = column.fixed_width();
        let scaled = scaled_fixed_width(width, from_pt, to_pt);
        if scaled != width {
            column.set_fixed_width(scaled);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_steps_clamp_between_eight_and_twenty_four_points() {
        assert_eq!(step_out(MIN_PT), MIN_PT);
        assert_eq!(step_in(MAX_PT), MAX_PT);
        assert_eq!(step_in(10), 11);
        assert_eq!(step_out(10), 9);
        assert_eq!(clamp_pt(3), MIN_PT);
        assert_eq!(clamp_pt(40), MAX_PT);
    }

    #[test]
    fn grid_font_css_scopes_the_session_point_size() {
        let css = grid_font_css(14);
        assert!(css.contains(&format!(".{CSS_CLASS}")));
        assert!(css.contains("font-size: 14pt;"));
        assert!(!css.contains("textview"));
    }

    #[test]
    fn fixed_widths_scale_with_the_font_ratio_and_leave_expanding_columns() {
        assert_eq!(scaled_fixed_width(120, 10, 20), 240);
        assert_eq!(scaled_fixed_width(120, 20, 10), 60);
        assert_eq!(scaled_fixed_width(-1, 10, 20), -1);
        assert_eq!(scaled_fixed_width(0, 10, 20), 0);
        assert_eq!(scaled_fixed_width(120, 11, 11), 120);
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn zooming_the_grid_font_rescales_fixed_column_widths_without_saving() {
        gtk::init().unwrap();
        SESSION_PT.set(DEFAULT_PT);
        let view = gtk::ColumnView::new(None::<gtk::MultiSelection>);
        let column = gtk::ColumnViewColumn::new(Some("value"), None::<gtk::SignalListItemFactory>);
        column.set_fixed_width(100);
        view.append_column(&column);
        let actions = gio::SimpleActionGroup::new();
        install(&view, &actions);
        view.insert_action_group("grid", Some(&actions));
        let window = gtk::Window::builder().child(&view).build();
        window.present();
        let context = glib::MainContext::default();
        while !view.is_mapped() {
            context.iteration(true);
        }
        assert!(view.activate_action("grid.zoom-in", None).is_ok());
        assert_eq!(session_pt(), step_in(DEFAULT_PT));
        assert_eq!(column.fixed_width(), scaled_fixed_width(100, DEFAULT_PT, session_pt()));
        assert!(view.activate_action("grid.zoom-reset", None).is_ok());
        assert_eq!(session_pt(), DEFAULT_PT);
        assert_eq!(column.fixed_width(), 100);
    }
}
