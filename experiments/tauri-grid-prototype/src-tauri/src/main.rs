use std::sync::Mutex;

use bookie_core::Value;
use serde::{Deserialize, Serialize};

const DEFAULT_ROWS: u64 = 1_000_000;
const ALLOWED_ROWS: [u64; 3] = [150_000, 500_000, 1_000_000];
const MAX_BATCH: usize = 128;
const MAX_FIND_MATCHES: usize = 10_000;
const MAX_DIFF_ROWS: usize = 200;
const TYPED_KINDS: u64 = 12;

#[derive(Serialize)]
struct Cell {
    kind: &'static str,
    value: Option<String>,
}

#[derive(Serialize)]
struct Row {
    row_number: u64,
    cells: Vec<Cell>,
}

#[derive(Serialize)]
struct Config {
    rows: u64,
    autoscroll_steps: u32,
}

#[derive(Serialize)]
struct FindReply {
    generation: u64,
    matches: Vec<u64>,
    capped: bool,
    scanned: u64,
}

#[derive(Deserialize, Clone, Debug)]
struct Predicate {
    column: String,
    op: String,
    text: String,
}

#[derive(Serialize)]
struct FilterReply {
    generation: u64,
    matched: u64,
}

#[derive(Serialize, Default, Debug, PartialEq)]
struct DiffSummary {
    mode: &'static str,
    equal: u64,
    added: u64,
    removed: u64,
    changed: u64,
}

#[derive(Serialize)]
struct DiffRow {
    change: &'static str,
    cells: Vec<Cell>,
}

#[derive(Serialize)]
struct CompareReply {
    generation: u64,
    summary: DiffSummary,
    rows: Vec<DiffRow>,
    capped: bool,
}

#[derive(Default)]
struct FilterState(Mutex<Option<(u64, Vec<u32>)>>);

fn cell(value: Value) -> Result<Cell, &'static str> {
    let (kind, value) = match value {
        Value::Null => ("null", None),
        Value::Bool(value) => ("boolean", Some(value.to_string())),
        Value::Int(value) => ("integer", Some(value.to_string())),
        Value::Float(value) => ("float", Some(value.to_string())),
        Value::Text(value) => ("text", Some(value)),
        Value::Bytes(value) => ("bytes", Some(format!("\\x{}", hex(&value)))),
        Value::Date(value) => ("date", Some(value.to_string())),
        Value::Time(value) => ("time", Some(value.to_string())),
        Value::DateTime(value) => ("datetime", Some(value.to_string())),
        Value::TimestampTz(value) => ("timestamptz", Some(value.to_rfc3339())),
        Value::Decimal(value) => ("decimal", Some(value.to_string())),
        Value::Uuid(value) => ("uuid", Some(value.to_string())),
        Value::Json(value) => ("json", Some(value.to_string())),
        Value::JsonText(value) => ("json_text", Some(value)),
        Value::Undecodable(type_name) => ("undecodable", Some(format!("<undecodable {type_name}>"))),
    };
    Ok(Cell { kind, value })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn typed_of(index: u64) -> Value {
    use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
    let date = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap_or_default();
    let time = NaiveTime::from_hms_micro_opt(23, 59, 59, 123_456).unwrap_or_default();
    match index % TYPED_KINDS {
        0 => Value::Float(0.1 + 0.2),
        1 => Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]),
        2 => Value::Date(date),
        3 => Value::Time(time),
        4 => Value::DateTime(date.and_time(time)),
        5 => Value::TimestampTz(Utc.from_utc_datetime(&date.and_time(time))),
        6 => Value::Decimal("12345678901234567890.12345678".parse().unwrap_or_default()),
        7 => Value::Uuid(uuid::Uuid::from_u128(0x67e5_5044_10b1_426f_9247_bb68_0e5f_e0c8)),
        8 => Value::Json(Box::new(serde_json::json!({"a": [1, {"b": null}]}))),
        9 => Value::JsonText("{\"raw\":  1,   \"keep\" : 2}".into()),
        10 => Value::Undecodable("numeric(40,20)".into()),
        _ => Value::Null,
    }
}

fn checked_total(total: Option<u64>) -> Result<u64, &'static str> {
    let total = total.unwrap_or(DEFAULT_ROWS);
    if ALLOWED_ROWS.contains(&total) {
        Ok(total)
    } else {
        Err("unsupported synthetic row count")
    }
}

fn value_of(index: u64) -> i64 {
    9_007_199_254_740_993_i64 + index as i64
}

fn label_of(index: u64) -> String {
    format!("Sample {index}")
}

fn state_of(index: u64) -> Value {
    if index % 3 == 0 {
        Value::Null
    } else {
        Value::Bool(index % 2 == 0)
    }
}

fn row_at(index: u64) -> Result<Row, &'static str> {
    Ok(Row {
        row_number: index,
        cells: cells_of(index, &label_of(index))?,
    })
}

fn cells_of(index: u64, label: &str) -> Result<Vec<Cell>, &'static str> {
    Ok(vec![
        cell(Value::Int(value_of(index)))?,
        cell(Value::Text(label.to_string()))?,
        cell(state_of(index))?,
        cell(typed_of(index))?,
    ])
}

fn column_text(index: u64, column: &str) -> Option<String> {
    match column {
        "value" => Some(value_of(index).to_string()),
        "label" => Some(label_of(index)),
        "state" => match state_of(index) {
            Value::Bool(flag) => Some(flag.to_string()),
            _ => None,
        },
        "typed" => cell(typed_of(index)).ok().and_then(|cell| cell.value),
        _ => None,
    }
}

fn matches_predicate(index: u64, predicate: &Predicate) -> bool {
    let needle = predicate.text.to_lowercase();
    let text = column_text(index, &predicate.column).map(|text| text.to_lowercase());
    match predicate.op.as_str() {
        "contains" => text.is_some_and(|text| text.contains(&needle)),
        "equals" => text.is_some_and(|text| text == needle),
        "not_equals" => text.is_none_or(|text| text != needle),
        _ => false,
    }
}

fn valid_predicate(predicate: &Predicate) -> Result<(), &'static str> {
    let known_column = matches!(predicate.column.as_str(), "value" | "label" | "state" | "typed");
    let known_op = matches!(predicate.op.as_str(), "contains" | "equals" | "not_equals");
    if !known_column || !known_op || predicate.text.is_empty() || predicate.text.len() > 256 {
        return Err("invalid filter");
    }
    Ok(())
}

#[tauri::command]
fn startup_config() -> Config {
    let rows = std::env::var("BOOKIE_PROTOTYPE_ROWS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|rows| ALLOWED_ROWS.contains(rows))
        .unwrap_or(DEFAULT_ROWS);
    let autoscroll_steps = std::env::var("BOOKIE_PROTOTYPE_AUTOSCROLL")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    Config { rows, autoscroll_steps }
}

#[tauri::command]
fn scroll_done(steps: u32, elapsed_ms: u64) {
    println!("PROTOTYPE_SCROLL_DONE steps={steps} elapsed_ms={elapsed_ms}");
}

#[tauri::command]
fn sample_rows(offset: u64, limit: usize, total: Option<u64>) -> Result<Vec<Row>, &'static str> {
    if limit > MAX_BATCH {
        return Err("batch exceeds the prototype limit");
    }
    let total = checked_total(total)?;
    let end = offset.saturating_add(limit as u64).min(total);
    (offset.min(total)..end).map(row_at).collect()
}

#[tauri::command]
fn find_rows(total: Option<u64>, query: String, cap: usize, generation: u64) -> Result<FindReply, &'static str> {
    if cap == 0 || cap > MAX_FIND_MATCHES {
        return Err("match cap exceeds the prototype limit");
    }
    if query.is_empty() || query.len() > 256 {
        return Err("invalid search text");
    }
    let total = checked_total(total)?;
    let needle = query.to_lowercase();
    let mut matches = Vec::new();
    for index in 0..total {
        let hit = ["value", "label", "state", "typed"]
            .iter()
            .filter_map(|column| column_text(index, column))
            .any(|text| text.to_lowercase().contains(&needle));
        if !hit {
            continue;
        }
        if matches.len() == cap {
            return Ok(FindReply {
                generation,
                matches,
                capped: true,
                scanned: index,
            });
        }
        matches.push(index);
    }
    Ok(FindReply {
        generation,
        matches,
        capped: false,
        scanned: total,
    })
}

#[tauri::command]
fn filter_rows(
    state: tauri::State<'_, FilterState>,
    total: Option<u64>,
    predicate: Predicate,
    generation: u64,
) -> Result<FilterReply, &'static str> {
    valid_predicate(&predicate)?;
    let total = checked_total(total)?;
    let kept: Vec<u32> = (0..total)
        .filter(|index| matches_predicate(*index, &predicate))
        .map(|index| index as u32)
        .collect();
    let matched = kept.len() as u64;
    *state.0.lock().map_err(|_| "filter state unavailable")? = Some((generation, kept));
    Ok(FilterReply { generation, matched })
}

#[tauri::command]
fn filtered_rows(
    state: tauri::State<'_, FilterState>,
    generation: u64,
    offset: u64,
    limit: usize,
) -> Result<Vec<Row>, &'static str> {
    if limit > MAX_BATCH {
        return Err("batch exceeds the prototype limit");
    }
    let guard = state.0.lock().map_err(|_| "filter state unavailable")?;
    let Some((stored, kept)) = guard.as_ref() else {
        return Err("no filtered result");
    };
    if *stored != generation {
        return Err("stale filtered result");
    }
    let start = usize::try_from(offset).unwrap_or(usize::MAX).min(kept.len());
    let end = start.saturating_add(limit).min(kept.len());
    kept[start..end].iter().map(|index| row_at(u64::from(*index))).collect()
}

fn right_state(index: u64) -> &'static str {
    match index % 100 {
        1 => "edited",
        2 => "dropped",
        _ => "same",
    }
}

fn build_diff(total: u64, keyed: bool, generation: u64) -> Result<CompareReply, &'static str> {
    let mode = if keyed { "keyed" } else { "whole_row" };
    let mut summary = DiffSummary {
        mode,
        ..DiffSummary::default()
    };
    let mut rows: Vec<DiffRow> = Vec::new();
    let mut capped = false;
    for index in 0..total {
        let mut produced: Vec<(&'static str, Vec<Cell>)> = Vec::new();
        let original = label_of(index);
        let edited = format!("{original} (edited)");
        match right_state(index) {
            "same" => summary.equal += 1,
            "dropped" => {
                summary.removed += 1;
                produced.push(("Removed", cells_of(index, &original)?));
            }
            _ if keyed => {
                summary.changed += 1;
                produced.push(("Before", cells_of(index, &original)?));
                produced.push(("After", cells_of(index, &edited)?));
            }
            _ => {
                summary.removed += 1;
                summary.added += 1;
                produced.push(("Removed", cells_of(index, &original)?));
                produced.push(("Added", cells_of(index, &edited)?));
            }
        }
        for (change, cells) in produced {
            if rows.len() < MAX_DIFF_ROWS {
                rows.push(DiffRow { change, cells });
            } else {
                capped = true;
            }
        }
    }
    Ok(CompareReply {
        generation,
        summary,
        rows,
        capped,
    })
}

#[tauri::command]
fn compare_rows(total: Option<u64>, keyed: bool, generation: u64) -> Result<CompareReply, &'static str> {
    build_diff(checked_total(total)?, keyed, generation)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tauri::Builder::default()
        .manage(FilterState::default())
        .setup(|app| {
            use tauri::Manager;
            let size = |name: &str| std::env::var(name).ok().and_then(|value| value.parse::<f64>().ok());
            if let (Some(width), Some(height), Some(window)) = (
                size("BOOKIE_PROTOTYPE_WIDTH"),
                size("BOOKIE_PROTOTYPE_HEIGHT"),
                app.get_webview_window("main"),
            ) {
                window.set_size(tauri::LogicalSize::new(width, height))?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            startup_config,
            scroll_done,
            sample_rows,
            find_rows,
            filter_rows,
            filtered_rows,
            compare_rows
        ])
        .run(tauri::generate_context!())?;
    Ok(())
}

#[cfg(test)]
mod tests;
