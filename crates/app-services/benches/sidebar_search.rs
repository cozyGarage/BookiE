use std::collections::HashSet;
use std::time::Instant;

use tablepro_app_services::sidebar_tree::{CollapseState, build_nodes, visible_nodes_with_hidden_schemas};
use tablepro_core::TableInfo;

const SAMPLES: usize = 101;

fn tables(count: usize) -> Vec<TableInfo> {
    (0..count)
        .map(|index| TableInfo {
            schema: Some(format!("profile_schema_{:04}", index / 100)),
            name: format!("profile_table_{index:05}"),
        })
        .collect()
}

fn queries() -> [&'static str; 3] {
    [
        "profile_schema_0001.profile_table_00100",
        "profile_schema_0001.profile_table_",
        "profile_table_",
    ]
}

fn percentile(sorted: &[u128], percentile: usize) -> u128 {
    sorted[(sorted.len() - 1) * percentile / 100]
}

fn profile_query(nodes: &[tablepro_app_services::sidebar_tree::TreeNode], query: &str) -> String {
    let mut samples = Vec::with_capacity(SAMPLES);
    let hidden = HashSet::new();
    let collapsed = CollapseState::default();
    let mut visible_count = 0;
    for _ in 0..SAMPLES {
        let start = Instant::now();
        visible_count = visible_nodes_with_hidden_schemas(nodes, &collapsed, query, &hidden).len();
        samples.push(start.elapsed().as_nanos());
    }
    samples.sort_unstable();
    serde_json::json!({
        "scenario": "sidebar_search",
        "query": query,
        "visible_nodes": visible_count,
        "samples": samples.len(),
        "median_ns": percentile(&samples, 50),
        "p95_ns": percentile(&samples, 95),
    })
    .to_string()
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let count = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "5000".into())
        .parse::<usize>()?;
    if !(1..=100_000).contains(&count) {
        return Err("table count must be between 1 and 100000".into());
    }
    let source_commit = std::env::var("BOOKIE_PROFILE_SOURCE_COMMIT").unwrap_or_default();
    let output_path = std::path::PathBuf::from(
        std::env::var_os("TABLEPRO_SIDEBAR_PROFILE_OUTPUT")
            .ok_or_else(|| std::io::Error::other("TABLEPRO_SIDEBAR_PROFILE_OUTPUT is required"))?,
    );
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let input = tables(count);
    let start = Instant::now();
    let nodes = build_nodes(&input, &[]);
    let build_ns = start.elapsed().as_nanos();
    let mut records = vec![
        serde_json::json!({
            "scenario": "sidebar_search_build",
            "source_commit": source_commit,
            "tables": count,
            "nodes": nodes.len(),
            "build_ns": build_ns,
        })
        .to_string(),
    ];
    for query in queries() {
        records.push(profile_query(&nodes, query));
    }
    records.push(String::new());
    std::fs::write(output_path, records.join("\n"))?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run()
}
