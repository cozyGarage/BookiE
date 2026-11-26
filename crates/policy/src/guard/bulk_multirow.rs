use bookie_core::Value;

const MAX_BIND_PARAMETERS: usize = 30_000;
const MAX_STATEMENT_VALUE_BYTES: usize = 4 * 1024 * 1024;

pub(super) fn mysql_multi_row_safe(sql_mode: &str, engine: Option<&str>) -> bool {
    let strict = sql_mode
        .split(',')
        .any(|mode| matches!(mode.trim(), "STRICT_ALL_TABLES" | "STRICT_TRANS_TABLES"));
    strict && engine.is_some_and(|engine| engine.eq_ignore_ascii_case("InnoDB"))
}

fn value_bytes(value: &Value) -> usize {
    match value {
        Value::Text(text) | Value::JsonText(text) => text.len(),
        Value::Bytes(bytes) => bytes.len(),
        _ => 16,
    }
}

fn chunk_rows(rows: &[Vec<Value>], per_statement: usize) -> Vec<&[Vec<Value>]> {
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut bytes = 0;
    for (index, row) in rows.iter().enumerate() {
        let size: usize = row.iter().map(value_bytes).sum();
        if index > start && (index - start == per_statement || bytes + size > MAX_STATEMENT_VALUE_BYTES) {
            chunks.push(&rows[start..index]);
            start = index;
            bytes = 0;
        }
        bytes += size;
    }
    if start < rows.len() {
        chunks.push(&rows[start..]);
    }
    chunks
}

fn skip_quoted(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut at = start + 1;
    while at < bytes.len() {
        if bytes[at] == quote {
            if bytes.get(at + 1) == Some(&quote) {
                at += 2;
                continue;
            }
            return at + 1;
        }
        at += 1;
    }
    bytes.len()
}

fn values_tuple(statement: &str) -> Option<(usize, usize)> {
    let bytes = statement.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'"' | b'`' | b'\'' => at = skip_quoted(bytes, at),
            _ if at > 0
                && bytes[at - 1].is_ascii_whitespace()
                && bytes.len() >= at + 6
                && bytes[at..at + 6].eq_ignore_ascii_case(b"VALUES") =>
            {
                let mut open = at + 6;
                while open < bytes.len() && bytes[open].is_ascii_whitespace() {
                    open += 1;
                }
                if bytes.get(open) != Some(&b'(') {
                    at += 6;
                    continue;
                }
                let mut depth = 0_usize;
                let mut cursor = open;
                while cursor < bytes.len() {
                    match bytes[cursor] {
                        b'"' | b'`' | b'\'' => {
                            cursor = skip_quoted(bytes, cursor);
                            continue;
                        }
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                return Some((open, cursor + 1));
                            }
                        }
                        _ => {}
                    }
                    cursor += 1;
                }
                return None;
            }
            _ => at += 1,
        }
    }
    None
}

fn shifted(tuple: &str, offset: usize, marker: u8) -> String {
    let mut out = String::with_capacity(tuple.len() + 8);
    let bytes = tuple.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == marker && bytes.get(at + 1).is_some_and(u8::is_ascii_digit) {
            let mut end = at + 1;
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            let number: usize = tuple[at + 1..end].parse().unwrap_or(0);
            out.push(char::from(marker));
            out.push_str(&(number + offset).to_string());
            at = end;
        } else {
            let character = tuple[at..].chars().next().unwrap_or(' ');
            out.push(character);
            at += character.len_utf8();
        }
    }
    out
}

fn placeholder_numbers(tuple: &str, marker: u8) -> (usize, Vec<usize>) {
    let bytes = tuple.as_bytes();
    let mut count = 0;
    let mut numbers = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == marker {
            count += 1;
            let mut end = at + 1;
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if end > at + 1 {
                numbers.push(tuple[at + 1..end].parse().unwrap_or(0));
            }
        }
        at += 1;
    }
    (count, numbers)
}

pub(super) fn multi_row_statements(
    statement: &str,
    driver_id: &str,
    rows: &[Vec<Value>],
) -> Option<Vec<(String, Vec<Value>)>> {
    let marker = match driver_id {
        "postgres" => b'$',
        "mysql" | "sqlite" => b'?',
        _ => return None,
    };
    let (open, close) = values_tuple(statement)?;
    let tuple = &statement[open..close];
    let width = rows.first()?.len();
    if width == 0 || rows.iter().any(|row| row.len() != width) {
        return None;
    }
    let (count, mut numbers) = placeholder_numbers(tuple, marker);
    let numbered = !numbers.is_empty();
    numbers.sort_unstable();
    let in_order = numbers.iter().copied().eq(1..=width);
    if count != width || (numbered && !in_order) || (!numbered && marker == b'$') {
        return None;
    }
    let per_statement = (MAX_BIND_PARAMETERS / width).max(1);
    Some(
        chunk_rows(rows, per_statement)
            .into_iter()
            .map(|chunk| {
                let mut sql = String::with_capacity(statement.len() + chunk.len() * tuple.len());
                sql.push_str(&statement[..open]);
                for index in 0..chunk.len() {
                    if index > 0 {
                        sql.push_str(", ");
                    }
                    if numbered {
                        sql.push_str(&shifted(tuple, index * width, marker));
                    } else {
                        sql.push_str(tuple);
                    }
                }
                sql.push_str(&statement[close..]);
                (sql, chunk.iter().flatten().cloned().collect())
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(count: usize, width: usize) -> Vec<Vec<Value>> {
        (0..count)
            .map(|row| {
                (0..width)
                    .map(|column| Value::Int((row * width + column) as i64))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn mysql_uses_multi_row_only_for_a_strict_transactional_table() {
        assert!(mysql_multi_row_safe("STRICT_TRANS_TABLES,NO_ZERO_DATE", Some("InnoDB")));
        assert!(mysql_multi_row_safe(
            "ONLY_FULL_GROUP_BY,STRICT_ALL_TABLES",
            Some("innodb")
        ));
        assert!(
            !mysql_multi_row_safe("", Some("InnoDB")),
            "non-strict coerces NULL and bad values"
        );
        assert!(
            !mysql_multi_row_safe("STRICT_TRANS_TABLES", Some("MyISAM")),
            "no rollback before the retry"
        );
        assert!(
            !mysql_multi_row_safe("STRICT_ALL_TABLES", None),
            "an unknown engine stays per-row"
        );
        assert!(!mysql_multi_row_safe("NOT_STRICT_TRANS_TABLES_X", Some("InnoDB")));
    }

    #[test]
    fn large_values_split_a_batch_before_the_statement_grows_too_big() {
        let wide: Vec<Vec<Value>> = (0..10)
            .map(|row| vec![Value::Int(row), Value::Text("x".repeat(1024 * 1024))])
            .collect();
        let statements = multi_row_statements("INSERT INTO t (id, body) VALUES (?, ?)", "mysql", &wide).unwrap();
        assert!(statements.len() >= 3, "{} statements", statements.len());
        assert!(statements.iter().all(|(_, params)| params.len() / 2 <= 4));
        assert_eq!(statements.iter().map(|(_, params)| params.len()).sum::<usize>(), 20);
    }

    #[test]
    fn postgres_repeats_the_tuple_and_renumbers_the_placeholders() {
        let statements = multi_row_statements(
            "INSERT INTO \"people\" (\"id\", \"name\") VALUES ($1, $2)",
            "postgres",
            &rows(3, 2),
        )
        .expect("a multi-row statement");
        assert_eq!(statements.len(), 1);
        assert_eq!(
            statements[0].0,
            "INSERT INTO \"people\" (\"id\", \"name\") VALUES ($1, $2), ($3, $4), ($5, $6)"
        );
        assert_eq!(statements[0].1.len(), 6);
        assert_eq!(statements[0].1[5], Value::Int(5));
    }

    #[test]
    fn the_overriding_clause_casts_and_the_conflict_suffix_are_kept() {
        let statements = multi_row_statements(
            "INSERT INTO t (id, seen) OVERRIDING SYSTEM VALUE VALUES ($1, $2::timestamp) ON CONFLICT (id) DO UPDATE SET seen = EXCLUDED.seen",
            "postgres",
            &rows(2, 2),
        )
        .unwrap();
        assert_eq!(
            statements[0].0,
            "INSERT INTO t (id, seen) OVERRIDING SYSTEM VALUE VALUES ($1, $2::timestamp), ($3, $4::timestamp) ON CONFLICT (id) DO UPDATE SET seen = EXCLUDED.seen"
        );
    }

    #[test]
    fn a_quoted_name_that_contains_values_is_not_mistaken_for_the_clause() {
        let statements = multi_row_statements(
            "INSERT INTO \"my VALUES (t\" (\"id\") VALUES ($1)",
            "postgres",
            &rows(2, 1),
        )
        .unwrap();
        assert_eq!(
            statements[0].0,
            "INSERT INTO \"my VALUES (t\" (\"id\") VALUES ($1), ($2)"
        );
    }

    #[test]
    fn mysql_and_sqlite_repeat_question_marks_unchanged() {
        for driver in ["mysql", "sqlite"] {
            let statements =
                multi_row_statements("INSERT INTO `t` (`a`, `b`) VALUES (?, ?)", driver, &rows(3, 2)).unwrap();
            assert_eq!(
                statements[0].0,
                "INSERT INTO `t` (`a`, `b`) VALUES (?, ?), (?, ?), (?, ?)"
            );
        }
    }

    #[test]
    fn numbered_question_marks_are_shifted_so_each_row_binds_its_own_values() {
        let statements = multi_row_statements("INSERT INTO t (a, b) VALUES (?1, ?2)", "sqlite", &rows(2, 2)).unwrap();
        assert_eq!(statements[0].0, "INSERT INTO t (a, b) VALUES (?1, ?2), (?3, ?4)");
    }

    #[test]
    fn numbering_that_is_not_one_to_width_is_refused() {
        assert!(multi_row_statements("INSERT INTO t (a, b) VALUES (?2, ?3)", "sqlite", &rows(2, 2)).is_none());
        assert!(multi_row_statements("INSERT INTO t (a, b) VALUES ($1, $3)", "postgres", &rows(2, 2)).is_none());
    }

    #[test]
    fn rows_are_split_so_no_statement_exceeds_the_parameter_cap() {
        let width = 4;
        let count = MAX_BIND_PARAMETERS / width + 10;
        let statements = multi_row_statements(
            "INSERT INTO t (a, b, c, d) VALUES ($1, $2, $3, $4)",
            "postgres",
            &rows(count, width),
        )
        .unwrap();
        assert_eq!(statements.len(), 2);
        assert!(statements.iter().all(|(_, params)| params.len() <= MAX_BIND_PARAMETERS));
        assert_eq!(
            statements.iter().map(|(_, params)| params.len()).sum::<usize>(),
            count * width
        );
        assert!(statements[1].0.contains("VALUES ($1, $2, $3, $4), "));
    }

    #[test]
    fn anything_unexpected_falls_back_to_one_statement_per_row() {
        let two = rows(2, 2);
        assert!(multi_row_statements("INSERT INTO t (a, b) VALUES ($1, $2)", "mssql", &two).is_none());
        assert!(multi_row_statements("INSERT INTO t (a, b) SELECT $1, $2", "postgres", &two).is_none());
        assert!(multi_row_statements("INSERT INTO t (a, b) VALUES ($1)", "postgres", &two).is_none());
        assert!(multi_row_statements("INSERT INTO t (a, b) VALUES ($1, $2)", "postgres", &[]).is_none());
        let ragged = vec![vec![Value::Int(1), Value::Int(2)], vec![Value::Int(3)]];
        assert!(multi_row_statements("INSERT INTO t (a, b) VALUES ($1, $2)", "postgres", &ragged).is_none());
    }
}
