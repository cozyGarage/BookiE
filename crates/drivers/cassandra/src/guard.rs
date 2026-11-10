pub(crate) fn read_only_statement(cql: &str) -> Option<String> {
    let body = strip_leading_comments(cql);
    let body = body.trim_end().trim_end_matches(';').trim_end();
    if body.contains(';') {
        return None;
    }
    let first = body.split(|c: char| !c.is_ascii_alphanumeric()).next()?;
    first.eq_ignore_ascii_case("select").then(|| body.to_string())
}

fn strip_leading_comments(cql: &str) -> &str {
    let mut rest = cql.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("--").or_else(|| rest.strip_prefix("//")) {
            rest = after.split_once('\n').map_or("", |(_, tail)| tail).trim_start();
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, tail)| tail).trim_start();
        } else {
            return rest;
        }
    }
}

pub(crate) fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub(crate) fn qualified(keyspace: Option<&str>, table: &str) -> String {
    match keyspace {
        Some(keyspace) => format!("{}.{}", quote_identifier(keyspace), quote_identifier(table)),
        None => quote_identifier(table),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_select_passes_and_loses_its_trailing_semicolon() {
        assert_eq!(
            read_only_statement("select * from ks.t where id = 1;").as_deref(),
            Some("select * from ks.t where id = 1")
        );
        assert_eq!(
            read_only_statement("-- note\n// more\n/* x */ SELECT 1 FROM system.local").as_deref(),
            Some("SELECT 1 FROM system.local")
        );
    }

    #[test]
    fn writes_ddl_batches_and_stacked_statements_are_refused() {
        for cql in [
            "insert into t (id) values (1)",
            "update t set a = 1 where id = 1",
            "delete from t where id = 1",
            "drop table t",
            "truncate t",
            "create keyspace k with replication = {}",
            "begin batch insert into t (id) values (1); apply batch",
            "use other",
            "select * from t; drop table t",
            "/* select */ delete from t",
            "",
            "  ",
        ] {
            assert_eq!(read_only_statement(cql), None, "{cql}");
        }
    }

    #[test]
    fn identifiers_are_quoted_and_embedded_quotes_are_doubled() {
        assert_eq!(quote_identifier("a\"b"), "\"a\"\"b\"");
        assert_eq!(qualified(Some("ks"), "t"), "\"ks\".\"t\"");
        assert_eq!(qualified(None, "t"), "\"t\"");
    }
}
