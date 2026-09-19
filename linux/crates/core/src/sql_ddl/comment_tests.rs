use crate::query::ColumnInfo;

use super::DraftColumn;
use super::types::{CommentPlacement, column_comment_placement};

fn info(comment: Option<&str>) -> ColumnInfo {
    ColumnInfo {
        name: "x".into(),
        data_type: "text".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: comment.map(str::to_string),
    }
}

#[test]
fn every_driver_agrees_with_the_placement_table() {
    assert_eq!(column_comment_placement("mysql"), CommentPlacement::Inline);
    assert_eq!(column_comment_placement("clickhouse"), CommentPlacement::Inline);
    assert_eq!(column_comment_placement("postgres"), CommentPlacement::Separate);
    assert_eq!(column_comment_placement("mssql"), CommentPlacement::Separate);
    for unsupported in ["sqlite", "duckdb", "mongodb", "redis", "unknown"] {
        assert_eq!(column_comment_placement(unsupported), CommentPlacement::Unsupported);
    }
}

#[test]
fn a_draft_starts_from_the_loaded_comment() {
    let draft = DraftColumn::from_info(info(Some("loaded")));
    assert_eq!(draft.comment.as_deref(), Some("loaded"));
    assert!(!draft.differs_beyond_name());
}

#[test]
fn changing_only_the_comment_is_a_change_beyond_the_name() {
    let mut draft = DraftColumn::from_info(info(None));
    draft.comment = Some("added".into());
    assert!(draft.differs_beyond_name());

    let mut cleared = DraftColumn::from_info(info(Some("loaded")));
    cleared.comment = None;
    assert!(cleared.differs_beyond_name());
}
