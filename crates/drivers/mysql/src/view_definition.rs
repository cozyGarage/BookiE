pub(super) struct ViewColumn {
    pub alias: String,
    pub origin: Option<(String, String)>,
}

pub(super) struct ParsedView {
    pub base_schema: Option<String>,
    pub base_table: String,
    pub columns: Vec<ViewColumn>,
}

fn mask(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = bytes.to_vec();
    let mut quote: Option<u8> = None;
    let mut index = 0;
    while index < bytes.len() {
        match quote {
            Some(open) => {
                out[index] = b'x';
                if bytes[index] == b'\\' && open != b'`' && index + 1 < bytes.len() {
                    out[index + 1] = b'x';
                    index += 1;
                } else if bytes[index] == open {
                    quote = None;
                }
            }
            None if matches!(bytes[index], b'\'' | b'"' | b'`') => {
                quote = Some(bytes[index]);
                out[index] = b'x';
            }
            None => {}
        }
        index += 1;
    }
    out
}

fn top_level_positions(masked: &[u8], wanted: impl Fn(&[u8]) -> bool) -> Vec<usize> {
    let mut depth = 0i32;
    let mut found = Vec::new();
    for index in 0..masked.len() {
        match masked[index] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ if depth == 0 && wanted(&masked[index..]) => found.push(index),
            _ => {}
        }
    }
    found
}

fn starts_with_word(rest: &[u8], word: &str) -> bool {
    rest.len() >= word.len() && rest[..word.len()].eq_ignore_ascii_case(word.as_bytes())
}

fn identifiers(expression: &str) -> Option<Vec<String>> {
    let bytes = expression.trim().as_bytes();
    let mut parts = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'`' {
            return None;
        }
        let mut name = Vec::new();
        index += 1;
        loop {
            match bytes.get(index)? {
                b'`' if bytes.get(index + 1) == Some(&b'`') => {
                    name.push(b'`');
                    index += 2;
                }
                b'`' => {
                    index += 1;
                    break;
                }
                other => {
                    name.push(*other);
                    index += 1;
                }
            }
        }
        parts.push(String::from_utf8(name).ok()?);
        match bytes.get(index) {
            None => break,
            Some(b'.') => index += 1,
            Some(_) => return None,
        }
    }
    (!parts.is_empty()).then_some(parts)
}

fn column(item: &str) -> Option<ViewColumn> {
    let masked = mask(item);
    let split = top_level_positions(&masked, |rest| starts_with_word(rest, " as "))
        .last()
        .copied()?;
    let alias = identifiers(&item[split + 4..])?;
    let [alias] = <[String; 1]>::try_from(alias).ok()?;
    let origin = match identifiers(&item[..split]).as_deref() {
        Some([table, column]) | Some([_, table, column]) => Some((table.clone(), column.clone())),
        _ => None,
    };
    Some(ViewColumn { alias, origin })
}

fn single_table(clause: &str) -> Option<(Option<String>, String)> {
    let masked = mask(clause);
    let closers = [" where ", " order ", " limit "];
    let end = closers
        .iter()
        .filter_map(|word| {
            top_level_positions(&masked, |rest| starts_with_word(rest, word))
                .first()
                .copied()
        })
        .min()
        .unwrap_or(clause.len());
    let table = &clause[..end];
    match identifiers(table)?.as_slice() {
        [name] => Some((None, name.clone())),
        [schema, name] => Some((Some(schema.clone()), name.clone())),
        _ => None,
    }
}

pub(super) fn parse(definition: &str) -> Option<ParsedView> {
    let definition = definition.trim();
    let definition = match definition.strip_prefix("/*") {
        Some(rest) => rest.split_once("*/")?.1.trim_start(),
        None => definition,
    };
    if !starts_with_word(definition.as_bytes(), "select ") {
        return None;
    }
    let list_start = "select ".len();
    let masked = mask(definition);
    let from = *top_level_positions(&masked, |rest| starts_with_word(rest, " from ")).first()?;
    let list = &definition[list_start..from];
    if starts_with_word(list.trim_start().as_bytes(), "distinct ") {
        return None;
    }
    let list_masked = mask(list);
    let mut bounds = vec![0];
    bounds.extend(
        top_level_positions(&list_masked, |rest| rest.first() == Some(&b','))
            .iter()
            .map(|at| at + 1),
    );
    bounds.push(list.len() + 1);
    let columns = bounds
        .windows(2)
        .map(|pair| column(&list[pair[0]..(pair[1] - 1).min(list.len())]))
        .collect::<Option<Vec<_>>>()?;
    let (base_schema, base_table) = single_table(&definition[from + " from ".len()..])?;
    Some(ParsedView {
        base_schema,
        base_table,
        columns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MYSQL: &str = "select `test`.`editable_base`.`id` AS `item_id`,`test`.`editable_base`.`part` AS `part`,upper(`test`.`editable_base`.`name`) AS `upper_name` from `test`.`editable_base`";
    const MARIADB: &str = "select `editable_base`.`id` AS `item_id`,`editable_base`.`part` AS `part`,ucase(`editable_base`.`name`) AS `upper_name` from `editable_base`";

    fn shape(parsed: &ParsedView) -> Vec<(String, Option<(String, String)>)> {
        parsed
            .columns
            .iter()
            .map(|column| (column.alias.clone(), column.origin.clone()))
            .collect()
    }

    #[test]
    fn mysql_and_mariadb_definitions_map_view_columns_to_base_columns() {
        for (definition, schema) in [(MYSQL, Some("test")), (MARIADB, None)] {
            let parsed = parse(definition).unwrap();
            assert_eq!(parsed.base_schema.as_deref(), schema);
            assert_eq!(parsed.base_table, "editable_base");
            assert_eq!(
                shape(&parsed),
                [
                    (
                        "item_id".to_owned(),
                        Some(("editable_base".to_owned(), "id".to_owned()))
                    ),
                    ("part".to_owned(), Some(("editable_base".to_owned(), "part".to_owned()))),
                    ("upper_name".to_owned(), None),
                ]
            );
        }
    }

    #[test]
    fn computed_columns_commas_and_quotes_inside_expressions_stay_computed() {
        let parsed = parse(
            "select `d`.`t`.`id` AS `id`,concat(`d`.`t`.`a`,', from ',`d`.`t`.`b`) AS `x`,(`d`.`t`.`id` + 0) AS `y` from `d`.`t` where (`d`.`t`.`id` > 1)",
        )
        .unwrap();
        assert_eq!(parsed.base_table, "t");
        assert_eq!(parsed.columns.len(), 3);
        assert!(parsed.columns[0].origin.is_some());
        assert!(parsed.columns[1].origin.is_none() && parsed.columns[2].origin.is_none());
    }

    #[test]
    fn views_that_are_not_a_plain_projection_of_one_table_are_refused() {
        for definition in [
            "select `d`.`a`.`id` AS `id` from `d`.`a` join `d`.`b` on `d`.`a`.`id` = `d`.`b`.`id`",
            "select `d`.`a`.`id` AS `id` from `d`.`a`,`d`.`b`",
            "select distinct `d`.`a`.`id` AS `id` from `d`.`a`",
            "select `d`.`a`.`id` AS `id` from `d`.`a` `x`",
            "select `d`.`a`.`id` from `d`.`a`",
            "select count(0) AS `n` from `d`.`a` group by `d`.`a`.`id`",
            "insert into x",
        ] {
            assert!(parse(definition).is_none(), "{definition}");
        }
    }
}
