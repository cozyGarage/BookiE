use super::*;

fn table(schema: Option<&str>, name: &str) -> TableInfo {
    TableInfo {
        schema: schema.map(str::to_string),
        name: name.into(),
    }
}

#[test]
fn the_chooser_lists_every_other_table_sorted_and_leaves_out_the_one_being_changed() {
    let tables = [
        table(Some("public"), "people"),
        table(Some("archive"), "people"),
        table(Some("public"), "orders"),
    ];
    let others = other_tables(&tables, Some("public"), "people");
    let labels: Vec<String> = others.iter().map(table_label).collect();
    assert_eq!(labels, ["archive.people", "public.orders"]);
}
