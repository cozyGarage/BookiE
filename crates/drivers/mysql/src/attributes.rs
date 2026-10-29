pub(super) fn with_on_update(default: String, column_type: &str, extra: &str) -> String {
    let lower_type = column_type.to_ascii_lowercase();
    let temporal = lower_type.starts_with("timestamp") || lower_type.starts_with("datetime");
    let Some((_, action)) = extra.split_once("on update ").filter(|_| temporal) else {
        return default;
    };
    format!("{default} ON UPDATE {}", action.trim().to_ascii_uppercase())
}

pub(super) fn with_invisible(column_type: String, extra: &str) -> String {
    if extra.contains("invisible") {
        format!("{column_type} INVISIBLE")
    } else {
        column_type
    }
}

pub(super) fn with_generation(column_type: String, expression: Option<&str>, extra: &str) -> String {
    let Some(expression) = expression.filter(|expression| !expression.is_empty()) else {
        return column_type;
    };
    let Some(plain) = unquoted_simple_expression(expression) else {
        return column_type;
    };
    let storage = if extra.contains("stored generated") {
        "STORED"
    } else {
        "VIRTUAL"
    };
    format!("{column_type} GENERATED ALWAYS AS ({plain}) {storage}")
}

fn unquoted_simple_expression(expression: &str) -> Option<String> {
    let mut plain = String::with_capacity(expression.len());
    let mut rest = expression;
    while let Some(start) = rest.find('`') {
        plain.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let end = tail.find('`')?;
        let name = &tail[..end];
        let simple = name.starts_with(|first: char| first.is_ascii_alphabetic() || first == '_')
            && name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_');
        if !simple {
            return None;
        }
        plain.push_str(name);
        rest = &tail[end + 1..];
    }
    plain.push_str(rest);
    let forbidden = [';', '"', '\\', '`'];
    if plain.contains(forbidden) || plain.contains("--") || plain.contains("/*") || plain.contains("*/") {
        return None;
    }
    Some(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generation_clause_is_restated_only_for_plain_identifiers() {
        assert_eq!(
            with_generation("int".into(), Some("(`a` + `b`)"), "stored generated"),
            "int GENERATED ALWAYS AS ((a + b)) STORED"
        );
        assert_eq!(
            with_generation("int".into(), Some("(`a` * 2)"), "virtual generated"),
            "int GENERATED ALWAYS AS ((a * 2)) VIRTUAL"
        );
        for unsafe_expression in [
            "(`odd name` + 1)",
            "(`a` + 1); DROP TABLE t",
            "(concat(`a`,_utf8mb4\\'x\\'))",
        ] {
            assert_eq!(
                with_generation("int".into(), Some(unsafe_expression), "virtual generated"),
                "int"
            );
        }
        assert_eq!(with_generation("int".into(), Some(""), ""), "int");
        assert_eq!(with_generation("int".into(), None, ""), "int");
    }
}
