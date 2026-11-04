use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;

use super::dialog::present;
use super::selection::candidates;
use super::tests::{result, rows_outcome};
use crate::ui::editor::SqlEditorInput;

fn descendants(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut found = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        let mut child = widget.first_child();
        while let Some(next) = child {
            child = next.next_sibling();
            pending.push(next);
        }
        found.push(widget);
    }
    found
}

fn pump() {
    let context = gtk::glib::MainContext::default();
    for _ in 0..20 {
        while context.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn settle() {
    let context = gtk::glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(600);
    while std::time::Instant::now() < deadline {
        while context.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn button(root: &gtk::Widget, label: &str) -> gtk::Button {
    descendants(root)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.label().is_some_and(|text| text == label))
        .unwrap_or_else(|| panic!("{label} button is missing"))
}

fn checks(root: &gtk::Widget) -> Vec<gtk::CheckButton> {
    descendants(root)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::CheckButton>().ok())
        .collect()
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_compare_dialog_offers_shared_columns_as_keys_and_sends_the_chosen_request() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let window = adw::Window::new();
    window.present();
    let outcomes = [
        rows_outcome("SELECT a", vec![result(&["id", "name"], 3, false)]),
        rows_outcome("SELECT b", vec![result(&["name", "id", "extra"], 2, false)]),
    ];
    let (sender, receiver) = relm4::channel();
    present(&window, candidates(&outcomes), sender);
    pump();
    let root: gtk::Widget = window.clone().upcast();
    let keys = checks(&root);
    let names: Vec<String> = keys
        .iter()
        .filter_map(|check| check.label().map(|label| label.to_string()))
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["id", "name"]);
    keys[0].set_active(true);
    button(&root, "Compare").emit_clicked();
    let Some(SqlEditorInput::CompareResults(_)) = receiver.recv_sync() else {
        panic!("the dialog did not send a comparison request");
    };
    assert!(!button(&root, "Compare").is_sensitive());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_compare_dialog_refuses_the_same_result_on_both_sides() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let window = adw::Window::new();
    window.present();
    let outcomes = [
        rows_outcome("SELECT a", vec![result(&["id"], 3, false)]),
        rows_outcome("SELECT b", vec![result(&["id"], 2, false)]),
    ];
    let (sender, _receiver) = relm4::channel();
    present(&window, candidates(&outcomes), sender);
    pump();
    let root: gtk::Widget = window.clone().upcast();
    let dropdowns: Vec<gtk::DropDown> = descendants(&root)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::DropDown>().ok())
        .collect();
    assert_eq!(dropdowns.len(), 2);
    for dropdown in &dropdowns {
        dropdown.set_selected(0);
    }
    button(&root, "Compare").emit_clicked();
    pump();
    assert!(button(&root, "Compare").is_sensitive());
    let notes: Vec<String> = descendants(&root)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Label>().ok())
        .map(|label| label.text().to_string())
        .collect();
    assert!(
        notes.iter().any(|text| text == "Choose two different results"),
        "{notes:?}"
    );
}

fn compare_buttons(root: &gtk::Widget) -> usize {
    descendants(root)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .filter(|button| button.label().is_some_and(|text| text == "Compare"))
        .count()
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_failed_comparison_keeps_the_dialog_and_its_choices_and_a_success_closes_it() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let window = adw::Window::new();
    window.present();
    let outcomes = [
        rows_outcome("SELECT a", vec![result(&["id"], 3, false)]),
        rows_outcome("SELECT b", vec![result(&["id"], 2, false)]),
    ];
    let (sender, receiver) = relm4::channel();
    present(&window, candidates(&outcomes), sender);
    settle();
    let root: gtk::Widget = window.clone().upcast();
    let keys = checks(&root);
    keys[0].set_active(true);
    button(&root, "Compare").emit_clicked();
    let Some(SqlEditorInput::CompareResults(request)) = receiver.recv_sync() else {
        panic!("the dialog did not send a comparison request");
    };
    request.ticket.fail("A key value appears more than once".into());
    settle();
    assert!(button(&root, "Compare").is_sensitive());
    assert!(checks(&root)[0].is_active());
    let notes: Vec<String> = descendants(&root)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Label>().ok())
        .map(|label| label.text().to_string())
        .collect();
    assert!(
        notes.iter().any(|text| text == "A key value appears more than once"),
        "{notes:?}"
    );
    button(&root, "Compare").emit_clicked();
    let Some(SqlEditorInput::CompareResults(request)) = receiver.recv_sync() else {
        panic!("the dialog did not send a second request");
    };
    request.ticket.finish("Equal 1".into());
    settle();
    assert_eq!(compare_buttons(&root), 0);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn wide_result_keys_and_actions_remain_reachable_in_a_short_window() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let window = adw::Window::builder().default_width(640).default_height(500).build();
    window.present();
    let names: Vec<String> = (0..80).map(|index| format!("column_{index:03}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let outcomes = [
        rows_outcome("SELECT a", vec![result(&names, 1, false)]),
        rows_outcome("SELECT b", vec![result(&names, 1, false)]),
    ];
    let (sender, _receiver) = relm4::channel();
    present(&window, candidates(&outcomes), sender);
    settle();
    let root: gtk::Widget = window.clone().upcast();
    let keys = checks(&root);
    assert_eq!(keys.len(), 80);
    let last = keys
        .iter()
        .find(|key| key.label().as_deref() == Some("column_079"))
        .unwrap();
    let css = gtk::CssProvider::new();
    gtk::style_context_add_provider_for_display(&root.display(), &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    for style in ["", "label, button, checkbutton { font-size: 20px; }"] {
        css.load_from_string(style);
        settle();
        for widget in [
            last.upcast_ref::<gtk::Widget>(),
            button(&root, "Compare").upcast_ref(),
            button(&root, "Cancel").upcast_ref(),
        ] {
            assert!(widget.grab_focus());
            settle();
            let bounds = widget.compute_bounds(&root).unwrap();
            assert!(
                bounds.y() >= 0.0 && bounds.y() + bounds.height() <= root.height() as f32,
                "focused control outside window: {bounds:?}; height {}",
                root.height()
            );
        }
    }
    gtk::style_context_remove_provider_for_display(&root.display(), &css);
    window.close();
}
