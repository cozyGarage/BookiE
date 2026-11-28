use gpui::TestAppContext;

use crate::ui_tests::open;

#[gpui::test]
fn the_test_platform_builds_no_accessibility_tree_yet(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx);
    cx.run_until_parked();
    let tree = cx.update(|window, _| (window.is_a11y_active(), window.debug_a11y_tree_json()));
    assert_eq!(
        tree,
        (false, None),
        "gpui now builds the accessibility tree in tests: add the unnamed-control check from \
         bookie-research proposals/gpui-test-adoption-from-permissive-peers-2026-11-28.md and replace this test"
    );
}
