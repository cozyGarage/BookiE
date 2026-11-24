use super::*;

fn draft(sql: &str) -> ClosedDraft {
    ClosedDraft {
        sql: sql.into(),
        connection: None,
    }
}

#[test]
fn the_newest_closed_draft_comes_back_first_and_blank_or_repeated_drafts_are_skipped() {
    let store = ClosedDraftStore::memory();
    store.remember(draft("SELECT 1")).unwrap();
    store.remember(draft("   \n  ")).unwrap();
    store.remember(draft("SELECT 2")).unwrap();
    store.remember(draft("SELECT 2")).unwrap();
    assert_eq!(store.len(), 2);
    assert_eq!(store.take_latest(), Some(draft("SELECT 2")));
    assert_eq!(store.take_latest(), Some(draft("SELECT 1")));
    assert_eq!(store.take_latest(), None);
}

#[test]
fn the_store_keeps_a_bounded_number_of_drafts_and_skips_oversized_ones() {
    let store = ClosedDraftStore::memory();
    for index in 0..(MAX_DRAFTS + 5) {
        store.remember(draft(&format!("SELECT {index}"))).unwrap();
    }
    assert_eq!(store.len(), MAX_DRAFTS);
    assert_eq!(store.take_latest(), Some(draft(&format!("SELECT {}", MAX_DRAFTS + 4))));
    store.remember(draft(&"x".repeat(MAX_DRAFT_BYTES + 1))).unwrap();
    assert_eq!(store.len(), MAX_DRAFTS - 1, "an oversized draft is not kept");
}

#[test]
fn drafts_survive_a_reload_from_the_same_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("closed-drafts.json");
    let store = ClosedDraftStore::at(path.clone());
    store
        .remember(ClosedDraft {
            sql: "UPDATE t SET a = 1".into(),
            connection: Some(uuid::Uuid::nil()),
        })
        .unwrap();
    store.flush().unwrap();
    drop(store);
    let reloaded = ClosedDraftStore::at(path);
    assert_eq!(
        reloaded.take_latest(),
        Some(ClosedDraft {
            sql: "UPDATE t SET a = 1".into(),
            connection: Some(uuid::Uuid::nil()),
        })
    );
}

#[test]
fn reopening_a_draft_in_the_same_session_drops_only_its_stored_copy() {
    let store = ClosedDraftStore::memory();
    store.remember(draft("SELECT 1")).unwrap();
    store.remember(draft("SELECT 2")).unwrap();
    store.forget_latest_if("SELECT 1");
    assert_eq!(store.len(), 2, "only the newest draft can be the one reopened");
    store.forget_latest_if("SELECT 2");
    assert_eq!(store.take_latest(), Some(draft("SELECT 1")));
}
