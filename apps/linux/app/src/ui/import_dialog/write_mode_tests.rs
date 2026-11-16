use super::*;

fn keys() -> Vec<ConflictKey> {
    vec![
        ConflictKey {
            label: "Primary key".into(),
            columns: vec!["id".into()],
            primary: true,
        },
        ConflictKey {
            label: "people_email_key".into(),
            columns: vec!["email".into()],
            primary: false,
        },
    ]
}

fn capabilities(style: ImportUpsertStyle) -> DriverCapabilities {
    DriverCapabilities {
        import_upsert: style,
        ..DriverCapabilities::default()
    }
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn an_engine_without_upsert_does_not_offer_the_mode() {
    adw::init().unwrap();
    assert!(WriteModeSection::new(&capabilities(ImportUpsertStyle::Unsupported), &keys()).is_none());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn insert_is_the_default_and_update_uses_the_chosen_key() {
    adw::init().unwrap();
    let section = WriteModeSection::new(&capabilities(ImportUpsertStyle::OnConflict), &keys()).unwrap();
    assert_eq!(section.selection(), Some(WriteMode::Insert));
    assert!(!section.key.is_sensitive());
    section.mode.set_selected(1);
    assert!(section.key.is_sensitive());
    assert_eq!(
        section.selection(),
        Some(WriteMode::UpdateOnConflict(vec!["id".into()]))
    );
    section.key.set_selected(1);
    assert_eq!(
        section.selection(),
        Some(WriteMode::UpdateOnConflict(vec!["email".into()]))
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_table_without_a_key_refuses_the_update_mode() {
    adw::init().unwrap();
    let section = WriteModeSection::new(&capabilities(ImportUpsertStyle::OnDuplicateKey), &[]).unwrap();
    assert!(!section.mode.is_sensitive());
    assert_eq!(section.selection(), Some(WriteMode::Insert));
    section.mode.set_selected(1);
    assert_eq!(section.selection(), None);
}
