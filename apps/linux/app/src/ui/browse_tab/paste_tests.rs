use super::*;

#[test]
fn paste_text_size_is_bounded_before_parsing() {
    assert!(paste_size_allowed(MAX_PASTE_BYTES));
    assert!(!paste_size_allowed(MAX_PASTE_BYTES + 1));
}

#[test]
fn a_pending_paste_requires_the_same_page_generation_and_target_row_identities() {
    use crate::services::change_tracker::RowKey;
    use bookie_core::Value;

    let first = RowKey::from_pk_values(&[Value::Int(1)]).unwrap();
    let second = RowKey::from_pk_values(&[Value::Int(2)]).unwrap();
    let draft = RowKey::Draft(7);
    let generation = uuid::Uuid::new_v4();
    let snapshot = PasteSnapshot::new(
        Some(generation),
        vec![Some(first.clone()), Some(second.clone()), Some(draft.clone())],
    );
    let unchanged = [Some(first.clone()), Some(second.clone()), Some(draft.clone())];
    let reordered = [Some(second.clone()), Some(first.clone()), Some(draft.clone())];
    let replaced_draft = [Some(first.clone()), Some(second.clone()), Some(RowKey::Draft(8))];
    let missing = [Some(first.clone()), None, Some(draft)];

    assert!(snapshot.matches(Some(generation), &[0, 1, 2], |position| {
        unchanged[position as usize].clone()
    }));
    assert!(!snapshot.matches(Some(uuid::Uuid::new_v4()), &[0], |position| {
        unchanged[position as usize].clone()
    }));
    assert!(!snapshot.matches(Some(generation), &[0, 1], |position| {
        reordered[position as usize].clone()
    }));
    assert!(!snapshot.matches(Some(generation), &[2], |position| {
        replaced_draft[position as usize].clone()
    }));
    assert!(!snapshot.matches(Some(generation), &[0, 1], |position| missing[position as usize].clone()));
}
