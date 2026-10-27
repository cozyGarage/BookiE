use super::*;

#[test]
#[ignore = "requires an isolated GTK display"]
fn closed_edit_popovers_release_their_widgets() {
    relm4::adw::init().unwrap();
    let window = gtk::Window::new();
    let cell = CellEditor::new();
    window.set_child(Some(&cell));
    window.present();
    let (sender, _receiver) = relm4::channel();
    let context = glib::MainContext::default();
    while context.iteration(false) {}
    for kind in 0..6 {
        match kind {
            0 | 1 => super::super::choice_editor::show_choice_popover(
                &cell,
                0,
                &sender,
                vec!["one".into(), "two".into()],
                kind == 1,
            ),
            2 => show_calendar_popover(&cell, 0, &sender),
            3 => show_exact_text_popover(&cell, 0, &sender),
            4 => show_spin_button_popover(&cell, 0, &sender, false, 1.0),
            _ => show_json_popover(&cell, 0, &sender),
        }
        while context.iteration(false) {}
        let popover = POPOVER_SLOT.cloned(&cell).unwrap();
        let retired = popover.downgrade();
        popover.popdown();
        drop(popover);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while retired.upgrade().is_some() && std::time::Instant::now() < deadline {
            context.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(
            retired.upgrade().is_none(),
            "closed editor popover {kind} remains alive"
        );
    }
    window.close();
}
