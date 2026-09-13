use super::{BrowsePageRequest, PageRequestTracker, RowCountRequestTracker};
use uuid::Uuid;

#[test]
fn only_the_latest_browse_page_request_is_accepted() {
    let tracker = PageRequestTracker::default();
    let older = tracker.begin(0);
    let newer = tracker.begin(0);

    assert!(!tracker.accepts(older, 0));
    assert!(tracker.accepts(newer, 0));
}

#[test]
fn only_the_latest_row_count_request_is_accepted() {
    let tracker = RowCountRequestTracker::default();
    let older = tracker.begin();
    let newer = tracker.begin();

    assert!(!tracker.accepts(older));
    assert!(tracker.accepts(newer));
}

#[test]
fn browse_page_response_must_match_the_current_offset() {
    let tracker = PageRequestTracker::default();
    let request = tracker.begin(100);
    let same_id_wrong_offset = BrowsePageRequest {
        id: request.id,
        offset: 100,
    };

    assert!(!tracker.accepts(same_id_wrong_offset, 200));
    assert_ne!(request.id, Uuid::nil());
}
