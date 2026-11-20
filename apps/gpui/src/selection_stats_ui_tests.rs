use super::stats_line;

#[test]
fn one_line_per_column_is_joined_for_the_status_bar() {
    assert_eq!(stats_line("a: count 1\nb: count 2"), "a: count 1  ·  b: count 2");
    assert_eq!(stats_line("only"), "only");
}
