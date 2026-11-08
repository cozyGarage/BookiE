use bookie_core::sql_syntax::SqlGrammar;
use bookie_core::{ColumnInfo, TableInfo};

use super::{
    ResultLock, ResultRefusal, SourceItem, SourceName, SourceRefusal, SourceTable, editable_target, source_table,
};

fn column(name: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.to_owned(),
        data_type: "text".into(),
        nullable: !primary_key,
        primary_key,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn source(sql: &str) -> Option<SourceTable> {
    source_table(sql, SqlGrammar::PostgreSql).ok()
}

fn named(name: &str) -> SourceItem {
    SourceItem::Column(SourceName {
        name: name.to_owned(),
        quoted: false,
    })
}

#[test]
fn a_select_from_one_table_with_its_key_is_editable() {
    let found = source("SELECT id, name FROM people WHERE id > 1").expect("one table");
    assert_eq!(
        found.table,
        TableInfo {
            schema: None,
            name: "people".into(),
        }
    );
    assert_eq!(found.items, vec![named("id"), named("name")]);
    let catalog = [column("id", true), column("name", false)];
    let result = catalog.clone();
    let target = editable_target(&found, &result, &catalog).expect("editable");
    assert_eq!(target.positions, vec![Some(0), Some(1)]);
}

#[test]
fn a_star_select_is_editable_when_the_catalog_width_matches() {
    let found = source("SELECT * FROM people").expect("one table");
    let catalog = [column("id", true), column("name", false), column("note", false)];
    let result = catalog.clone();
    let target = editable_target(&found, &result, &catalog).expect("editable");
    assert_eq!(target.positions, vec![Some(0), Some(1), Some(2)]);
}

#[test]
fn a_partial_key_stays_read_only() {
    let found = source("SELECT name FROM people").expect("one table");
    let catalog = [column("id", true), column("name", false)];
    let result = [column("name", false)];
    assert_eq!(editable_target(&found, &result, &catalog), Err(ResultLock::KeyNotShown));
}

#[test]
fn a_join_is_refused() {
    assert_eq!(
        source_table("SELECT * FROM a JOIN b ON a.id = b.id", SqlGrammar::PostgreSql),
        Err(SourceRefusal::MoreThanOneTable)
    );
}

#[test]
fn a_grouped_select_is_refused() {
    assert_eq!(
        source_table("SELECT name, count(*) FROM a GROUP BY name", SqlGrammar::PostgreSql),
        Err(SourceRefusal::Collapsed)
    );
}

#[test]
fn every_refusal_variant_is_distinct() {
    use std::collections::HashSet;
    let all = [
        ResultRefusal::NotOneSelect,
        ResultRefusal::MoreThanOneTable,
        ResultRefusal::Collapsed,
        ResultRefusal::Untraced,
        ResultRefusal::NoPrimaryKey,
        ResultRefusal::KeyNotShown,
        ResultRefusal::ColumnsChanged,
        ResultRefusal::ReadOnlySession,
        ResultRefusal::OpenTransaction,
        ResultRefusal::OtherDatabase,
        ResultRefusal::OtherResult,
        ResultRefusal::Checking,
        ResultRefusal::ColumnsUnread,
        ResultRefusal::MultiResult,
    ];
    let ids: HashSet<_> = all.iter().copied().collect();
    assert_eq!(ids.len(), all.len());
}

#[test]
fn a_table_without_a_primary_key_is_refused() {
    let found = source("SELECT id, name FROM people").expect("one table");
    let catalog = [column("id", false), column("name", false)];
    assert_eq!(
        editable_target(&found, &catalog, &catalog),
        Err(ResultLock::NoPrimaryKey)
    );
}
