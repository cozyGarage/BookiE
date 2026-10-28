use super::*;
use crate::ui::editor::bottom_panel::PanelAction;

#[test]
#[ignore = "requires an isolated GTK display"]
fn maximizing_and_hiding_restore_the_split_editor_and_divider() {
    let fixture = EditorFixture::new();
    let window = gtk::Window::builder()
        .default_width(1000)
        .default_height(800)
        .child(fixture.controller.widget())
        .build();
    fixture
        .controller
        .sender()
        .send(SqlEditorInput::SplitToggled(true))
        .unwrap();
    window.present();
    spin_until(|| fixture.controller.model().split_view.secondary_view().is_mapped());
    let secondary = fixture.controller.model().split_view.secondary_view().clone();
    assert!(secondary.grab_focus());
    let paned = descendants(fixture.controller.widget().upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Paned>().ok())
        .find(|paned| paned.orientation() == gtk::Orientation::Vertical)
        .unwrap();
    paned.set_position(300);
    let before = paned.position();
    let maximize = descendants(fixture.controller.widget().upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.tooltip_text().as_deref() == Some("Maximize results"))
        .unwrap();
    maximize.grab_focus();
    maximize.emit_clicked();
    spin_until(|| !secondary.is_mapped());
    fixture
        .controller
        .sender()
        .send(SqlEditorInput::Panel(PanelAction::Maximize))
        .unwrap();
    spin_until(|| secondary.is_mapped());
    assert_eq!(paned.position(), before);
    assert_eq!(
        gtk::prelude::RootExt::focus(&window).as_ref(),
        Some(secondary.upcast_ref())
    );
    fixture
        .controller
        .sender()
        .send(SqlEditorInput::Panel(PanelAction::Maximize))
        .unwrap();
    spin_until(|| !secondary.is_mapped());
    fixture
        .controller
        .sender()
        .send(SqlEditorInput::Panel(PanelAction::Toggle))
        .unwrap();
    spin_until(|| secondary.is_mapped());
    assert_eq!(
        gtk::prelude::RootExt::focus(&window).as_ref(),
        Some(secondary.upcast_ref())
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    window.destroy();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_result_switcher_groups_results_and_errors_in_one_linked_strip() {
    let fixture = EditorFixture::new();
    let widgets = descendants(fixture.controller.widget().upcast_ref());
    assert!(
        widgets
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .all(|button| button.tooltip_text().as_deref() != Some("Show result sets"))
    );
    let switcher: Vec<gtk::ToggleButton> = widgets
        .iter()
        .filter_map(|widget| widget.clone().downcast::<gtk::ToggleButton>().ok())
        .filter(|toggle| matches!(toggle.label().as_deref(), Some("Results" | "Errors")))
        .collect();
    assert_eq!(switcher.len(), 2);
    let strip = switcher[0].parent().unwrap();
    assert_eq!(switcher[1].parent().as_ref(), Some(&strip));
    assert!(strip.has_css_class("linked"));
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_switcher_shows_the_error_count_only_when_there_are_errors_and_marks_the_active_view() {
    let fixture = EditorFixture::new();
    let toggle = |label: &str| {
        descendants(fixture.controller.widget().upcast_ref())
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::ToggleButton>().ok())
            .find(|toggle| toggle.label().as_deref() == Some(label))
    };
    let errors = toggle("Errors").unwrap();
    let results = toggle("Results").unwrap();
    assert!(results.is_active());
    assert!(!errors.is_active());
    fixture.controller.model().run_button.emit_clicked();
    let query = fixture.next_query();
    fixture.finish(query, Err(DriverError::Unsupported("fixture failure".into())));
    assert_eq!(errors.label().as_deref(), Some("Errors (1)"));
    errors.emit_clicked();
    fixture.process_inputs();
    assert!(errors.is_active());
    assert!(!results.is_active());
    results.emit_clicked();
    fixture.process_inputs();
    assert!(results.is_active());
    assert!(!errors.is_active());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_results_button_shows_results_after_hiding_errors() {
    let fixture = EditorFixture::new();
    let toggle = |label: &str| {
        descendants(fixture.controller.widget().upcast_ref())
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::ToggleButton>().ok())
            .find(|toggle| toggle.label().as_deref() == Some(label))
    };
    let errors = toggle("Errors").unwrap();
    let results = toggle("Results").unwrap();
    fixture.controller.model().run_button.emit_clicked();
    let query = fixture.next_query();
    fixture.finish(query, Err(DriverError::Unsupported("fixture failure".into())));
    errors.emit_clicked();
    fixture.process_inputs();
    assert!(errors.is_active());
    fixture
        .controller
        .sender()
        .send(SqlEditorInput::Panel(PanelAction::Toggle))
        .unwrap();
    fixture.process_inputs();
    results.emit_clicked();
    fixture.process_inputs();
    assert!(results.is_active());
    assert!(!errors.is_active());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_error_view_retains_results_and_clears_failures_after_success() {
    let fixture = EditorFixture::new();
    fixture.controller.model().run_button.emit_clicked();
    let query = fixture.next_query();
    fixture.finish(query, Err(DriverError::Unsupported("fixture failure".into())));
    let errors = descendants(fixture.controller.widget().upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::ToggleButton>().ok())
        .find(|button| button.label().as_deref() == Some("Errors (1)"))
        .unwrap();
    let original = fixture.controller.model().results_holder.first_child().unwrap();
    errors.emit_clicked();
    fixture.process_inputs();
    assert_eq!(fixture.controller.model().results_holder.first_child(), Some(original));
    fixture.controller.model().run_button.emit_clicked();
    let query = fixture.next_query();
    fixture.finish(query, success());
    assert_eq!(errors.label().as_deref(), Some("Errors"));
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 2);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn hiding_results_falls_back_when_the_remembered_sql_split_was_closed() {
    let fixture = EditorFixture::new();
    let window = gtk::Window::builder()
        .default_width(1000)
        .default_height(700)
        .child(fixture.controller.widget())
        .build();
    fixture
        .controller
        .sender()
        .send(SqlEditorInput::SplitToggled(true))
        .unwrap();
    window.present();
    spin_until(|| fixture.controller.model().split_view.secondary_view().is_mapped());
    fixture.controller.model().split_view.secondary_view().grab_focus();
    fixture
        .controller
        .sender()
        .send(SqlEditorInput::Panel(PanelAction::Errors))
        .unwrap();
    fixture
        .controller
        .sender()
        .send(SqlEditorInput::SplitToggled(false))
        .unwrap();
    spin_until(|| !fixture.controller.model().split_view.secondary_view().is_mapped());
    fixture
        .controller
        .sender()
        .send(SqlEditorInput::Panel(PanelAction::Toggle))
        .unwrap();
    spin_until(|| {
        gtk::prelude::RootExt::focus(&window).as_ref() == Some(fixture.controller.model().source_view.upcast_ref())
    });
    window.destroy();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_editor_toolbar_is_one_wrapping_bar_with_folding_in_a_menu() {
    let fixture = EditorFixture::new();
    let model = fixture.controller.model();
    let actions = model.run_button.parent().unwrap();
    assert!(actions.is::<adw::WrapBox>());
    assert_eq!(model.pin_button.parent().as_ref(), Some(&actions));
    let fold = descendants(fixture.controller.widget().upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.label().as_deref() == Some("Fold block"))
        .unwrap();
    assert!(fold.ancestor(gtk::Popover::static_type()).is_some());
}
