pub fn cancel_statement(driver_id: &str, process_id: i64) -> Option<String> {
    match driver_id {
        "postgres" => Some(format!("SELECT pg_cancel_backend({process_id});")),
        "mysql" => Some(format!("KILL QUERY {process_id};")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::cancel_statement;

    #[test]
    fn postgres_and_mysql_name_the_process_and_other_engines_have_no_cancel() {
        assert_eq!(
            cancel_statement("postgres", 4242).as_deref(),
            Some("SELECT pg_cancel_backend(4242);")
        );
        assert_eq!(cancel_statement("mysql", 7).as_deref(), Some("KILL QUERY 7;"));
        for driver_id in ["sqlite", "mongodb", "redis", "clickhouse", "mssql", ""] {
            assert_eq!(cancel_statement(driver_id, 1), None, "{driver_id}");
        }
    }
}
