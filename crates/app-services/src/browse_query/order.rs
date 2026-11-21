use bookie_core::ColumnInfo;

pub(super) fn resolved_order_by(
    driver_id: &str,
    columns: &[ColumnInfo],
    sort: Option<(usize, bool)>,
    row_id_order: bool,
) -> Option<String> {
    let postgres = driver_id == "postgres";
    let mut terms = Vec::new();
    let selected = sort.and_then(|(index, ascending)| {
        columns.get(index).map(|column| {
            let direction = if ascending { "ASC" } else { "DESC" };
            let null_order = if postgres { " NULLS LAST" } else { "" };
            terms.push(format!(
                "{} {direction}{null_order}",
                bookie_core::sql_dialect::quote_ident(driver_id, &column.name)
            ));
            column.name.as_str()
        })
    });
    for column in columns.iter().filter(|column| column.primary_key) {
        if selected == Some(column.name.as_str()) {
            continue;
        }
        terms.push(format!(
            "{} ASC",
            bookie_core::sql_dialect::quote_ident(driver_id, &column.name)
        ));
    }
    if row_id_order && postgres && !columns.iter().any(|column| column.primary_key) {
        terms.push("ctid ASC".to_owned());
    }
    (!terms.is_empty()).then(|| terms.join(", "))
}
