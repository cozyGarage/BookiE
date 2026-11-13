use bookie_core::InboundForeignKeyInfo;

pub fn referencing_tables(inbound: &[InboundForeignKeyInfo], schema: Option<&str>, table: &str) -> Vec<String> {
    let mut names: Vec<String> = inbound
        .iter()
        .filter(|key| !(key.source_table == table && (key.source_schema.as_deref() == schema || schema.is_none())))
        .map(|key| match &key.source_schema {
            Some(source_schema) if Some(source_schema.as_str()) != schema => {
                format!("{source_schema}.{}", key.source_table)
            }
            _ => key.source_table.clone(),
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(schema: Option<&str>, table: &str) -> InboundForeignKeyInfo {
        InboundForeignKeyInfo {
            name: format!("{table}_fk"),
            source_schema: schema.map(str::to_string),
            source_table: table.to_string(),
            source_columns: vec!["customer_id".into()],
            target_columns: vec!["id".into()],
        }
    }

    #[test]
    fn each_referencing_table_is_named_once_and_a_self_reference_is_left_out() {
        let inbound = [
            key(Some("public"), "orders"),
            key(Some("public"), "orders"),
            key(Some("billing"), "invoices"),
            key(Some("public"), "customers"),
        ];
        assert_eq!(
            referencing_tables(&inbound, Some("public"), "customers"),
            ["billing.invoices", "orders"]
        );
        assert!(referencing_tables(&[key(None, "customers")], None, "customers").is_empty());
    }
}
