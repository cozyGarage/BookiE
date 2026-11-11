use super::*;
use relm4::{Component, ComponentController};

const COLLATIONS: [&str; 2] = ["utf8mb4_bin", "utf8mb4_unicode_ci"];

fn new_table_tab(driver_id: &str) -> relm4::Controller<StructureTab> {
    adw::init().unwrap();
    StructureTab::builder()
        .launch(StructureTabInit {
            structure_trackers: Default::default(),
            tab_id: Uuid::new_v4(),
            schema: None,
            table: String::new(),
            mode: StructureMode::New,
            driver_id: driver_id.into(),
            column_comment_capability: Default::default(),
            defer_initial_fetch: true,
        })
        .detach()
}

fn drain() {
    let context = gtk::glib::MainContext::default();
    while context.iteration(false) {}
}

fn send(controller: &relm4::Controller<StructureTab>, input: StructureTabInput) {
    let _ = controller.sender().send(input);
    drain();
}

fn load_collations(controller: &relm4::Controller<StructureTab>, supported: bool) {
    send(
        controller,
        StructureTabInput::CollationsLoaded {
            collations: COLLATIONS.iter().map(|name| (*name).to_owned()).collect(),
            table_default_supported: supported,
        },
    );
}

fn preview(controller: &relm4::Controller<StructureTab>) -> String {
    let model = controller.model();
    let (start, end) = (model.sql_buffer.start_iter(), model.sql_buffer.end_iter());
    model.sql_buffer.text(&start, &end, false).to_string()
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_new_table_tab_offers_a_default_collation_when_the_driver_supports_one() {
    let controller = new_table_tab("mysql");
    assert!(!controller.model().table_collation_row.is_visible());

    load_collations(&controller, true);

    let model = controller.model();
    assert!(model.table_collation_row.is_visible());
    let items = model.table_collation_row.model().expect("a model of choices").n_items();
    assert_eq!(items as usize, COLLATIONS.len() + 1, "the server default plus every collation");
    assert_eq!(model.table_collation_row.selected(), 0, "the server default is preselected");
    assert_eq!(*model.default_collation.borrow(), None);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn choosing_a_default_collation_puts_it_in_the_create_table_statement() {
    let controller = new_table_tab("mysql");
    load_collations(&controller, true);
    send(&controller, StructureTabInput::TableNameEdited("orders".into()));
    send(&controller, StructureTabInput::AddColumn);
    send(
        &controller,
        StructureTabInput::ColumnEdited {
            index: 0,
            field: ColumnField::Name("label".into()),
        },
    );
    send(
        &controller,
        StructureTabInput::ColumnEdited {
            index: 0,
            field: ColumnField::Type("varchar(20)".into()),
        },
    );
    assert!(!preview(&controller).contains("COLLATE"), "{}", preview(&controller));

    controller.model().table_collation_row.set_selected(2);
    drain();

    assert_eq!(
        controller.model().default_collation.borrow().as_deref(),
        Some("utf8mb4_unicode_ci")
    );
    let sql = preview(&controller);
    assert!(sql.contains("CREATE TABLE `orders`"), "{sql}");
    assert!(sql.contains("DEFAULT COLLATE utf8mb4_unicode_ci"), "{sql}");

    controller.model().table_collation_row.set_selected(0);
    drain();
    assert_eq!(*controller.model().default_collation.borrow(), None);
    assert!(!preview(&controller).contains("DEFAULT COLLATE"), "{}", preview(&controller));
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_driver_without_table_level_collations_never_shows_the_default_collation_row() {
    let controller = new_table_tab("sqlite");

    load_collations(&controller, false);

    assert!(!controller.model().table_collation_row.is_visible());
}
