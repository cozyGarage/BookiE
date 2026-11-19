#[derive(Debug, Clone, PartialEq)]
pub struct PlanNode {
    pub depth: usize,
    pub title: String,
    pub cost: Option<f64>,
    pub rows: Option<f64>,
    pub details: Vec<String>,
}

fn number_after(text: &str, key: &str) -> Option<f64> {
    let start = text.find(key)? + key.len();
    let rest = &text[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn postgres_node(depth: usize, text: &str) -> PlanNode {
    let (title, stats) = match text.find("(cost=") {
        Some(at) => (text[..at].trim(), &text[at..]),
        None => (text.trim(), ""),
    };
    let cost = stats
        .find("..")
        .and_then(|at| number_after(&stats[at..], ".."))
        .filter(|_| stats.starts_with("(cost="));
    PlanNode {
        depth,
        title: title.to_string(),
        cost,
        rows: number_after(stats, "rows="),
        details: Vec::new(),
    }
}

fn parse_postgres(text: &str) -> Vec<PlanNode> {
    let mut nodes: Vec<PlanNode> = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let indent = line.len() - line.trim_start().len();
        let body = line.trim_start();
        if let Some(rest) = body.strip_prefix("->") {
            nodes.push(postgres_node((indent + 4) / 6, rest.trim_start()));
        } else if nodes.is_empty() {
            nodes.push(postgres_node(0, body));
        } else if let Some(last) = nodes.last_mut() {
            last.details.push(body.to_string());
        }
    }
    nodes
}

fn parse_sqlite(text: &str) -> Vec<PlanNode> {
    let mut depths: Vec<(u64, usize)> = Vec::new();
    let mut nodes = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(4, " | ").collect();
        let [id, parent, _, detail] = parts.as_slice() else {
            if nodes.is_empty() {
                continue;
            }
            return Vec::new();
        };
        let (Ok(id), Ok(parent)) = (id.trim().parse::<u64>(), parent.trim().parse::<u64>()) else {
            if nodes.is_empty() {
                continue;
            }
            return Vec::new();
        };
        let depth = depths
            .iter()
            .find(|(known, _)| *known == parent)
            .map_or(0, |(_, depth)| depth + 1);
        depths.push((id, depth));
        nodes.push(PlanNode {
            depth,
            title: detail.trim().to_string(),
            cost: None,
            rows: None,
            details: Vec::new(),
        });
    }
    nodes
}

pub fn parse_plan(text: &str) -> Vec<PlanNode> {
    let sqlite = parse_sqlite(text);
    if !sqlite.is_empty() {
        return sqlite;
    }
    let postgres = parse_postgres(text);
    if postgres.iter().any(|node| node.cost.is_some()) {
        postgres
    } else {
        Vec::new()
    }
}

pub fn plan_text(columns: &[String], rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::from("(empty plan)");
    }
    let tabular = columns.len() > 1 || rows.iter().any(|row| row.len() > 1);
    let mut lines = Vec::with_capacity(rows.len() + usize::from(tabular));
    if tabular && !columns.is_empty() {
        lines.push(columns.join(" | "));
    }
    lines.extend(rows.iter().map(|row| row.join(" | ")));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const POSTGRES: &str = "Hash Join  (cost=10.00..120.50 rows=100 width=8)\n  Hash Cond: (a.id = b.id)\n  ->  Seq Scan on a  (cost=0.00..60.00 rows=1000 width=4)\n        Filter: (x > 1)\n  ->  Hash  (cost=5.00..5.00 rows=10 width=4)\n        ->  Index Scan using b_pkey on b  (cost=0.15..5.00 rows=10 width=4)\nPlanning Time: 0.1 ms";

    #[test]
    fn a_postgres_text_plan_becomes_a_tree_with_costs_rows_and_details() {
        let nodes = parse_plan(POSTGRES);
        let shape: Vec<(usize, &str)> = nodes.iter().map(|node| (node.depth, node.title.as_str())).collect();
        assert_eq!(
            shape,
            [
                (0, "Hash Join"),
                (1, "Seq Scan on a"),
                (1, "Hash"),
                (2, "Index Scan using b_pkey on b")
            ]
        );
        assert_eq!(nodes[0].cost, Some(120.5));
        assert_eq!(nodes[1].rows, Some(1000.0));
        assert_eq!(nodes[0].details, ["Hash Cond: (a.id = b.id)"]);
        assert_eq!(nodes[1].details, ["Filter: (x > 1)"]);
        assert!(nodes[3].details.contains(&"Planning Time: 0.1 ms".to_string()));
    }

    #[test]
    fn a_sqlite_query_plan_nests_by_parent_id() {
        let nodes = parse_plan(
            "2 | 0 | 0 | SCAN orders\n5 | 0 | 0 | SEARCH customers USING INTEGER PRIMARY KEY (rowid=?)\n7 | 5 | 0 | USE TEMP B-TREE FOR ORDER BY",
        );
        let depths: Vec<usize> = nodes.iter().map(|node| node.depth).collect();
        assert_eq!(depths, [0, 0, 1]);
        assert_eq!(nodes[2].title, "USE TEMP B-TREE FOR ORDER BY");
    }

    #[test]
    fn a_sqlite_plan_with_a_column_header_line_still_nests() {
        let nodes = parse_plan(
            "id | parent | notused | detail\n2 | 0 | 0 | SCAN orders\n5 | 0 | 0 | SEARCH customers USING INTEGER PRIMARY KEY (rowid=?)\n7 | 5 | 0 | USE TEMP B-TREE FOR ORDER BY",
        );
        let depths: Vec<usize> = nodes.iter().map(|node| node.depth).collect();
        assert_eq!(depths, [0, 0, 1]);
        assert_eq!(nodes[2].title, "USE TEMP B-TREE FOR ORDER BY");
    }

    #[test]
    fn text_that_is_not_a_known_plan_stays_text_only() {
        assert!(parse_plan("Seq Scan on a").is_empty());
        assert!(parse_plan("(empty plan)").is_empty());
        assert!(parse_plan("1 | MySQL | table").is_empty());
        assert!(parse_plan("id | select_type | table | type\n1 | SIMPLE | c | const").is_empty());
        assert!(parse_plan("").is_empty());
    }

    #[test]
    fn plan_text_joins_cells_and_adds_a_header_only_for_several_columns() {
        let one = plan_text(
            &["QUERY PLAN".into()],
            &[vec!["Seq Scan on a".into()], vec!["  Filter: x".into()]],
        );
        assert_eq!(one, "Seq Scan on a\n  Filter: x");
        let many = plan_text(
            &["id".into(), "parent".into(), "notused".into(), "detail".into()],
            &[vec!["2".into(), "0".into(), "0".into(), "SCAN orders".into()]],
        );
        assert_eq!(many, "id | parent | notused | detail\n2 | 0 | 0 | SCAN orders");
        assert_eq!(plan_text(&[], &[]), "(empty plan)");
    }
}
