use bookie_core::Value;
use serde::Serialize;

const TOTAL_ROWS: u64 = 1_000_000;
const MAX_BATCH: usize = 128;

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

fn cell(value: Value) -> Result<Cell, &'static str> {
    let (kind, value) = match value {
        Value::Null => ("null", None),
        Value::Bool(value) => ("boolean", Some(value.to_string())),
        Value::Int(value) => ("integer", Some(value.to_string())),
        Value::Text(value) => ("text", Some(value)),
        _ => return Err("unsupported prototype value"),
    };
    Ok(Cell { kind, value })
}

#[tauri::command]
fn sample_rows(offset: u64, limit: usize) -> Result<Vec<Row>, &'static str> {
    if limit > MAX_BATCH {
        return Err("batch exceeds the prototype limit");
    }
    let end = offset.saturating_add(limit as u64).min(TOTAL_ROWS);
    (offset.min(TOTAL_ROWS)..end)
        .map(|index| {
            let number = 9_007_199_254_740_993_i64 + index as i64;
            let state = if index % 3 == 0 {
                Value::Null
            } else {
                Value::Bool(index % 2 == 0)
            };
            Ok(Row {
                row_number: index,
                cells: vec![
                    cell(Value::Int(number))?,
                    cell(Value::Text(format!("Sample {index}")))?,
                    cell(state)?,
                ],
            })
        })
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![sample_rows])
        .run(tauri::generate_context!())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_bounded_and_large_integers_keep_all_digits() {
        let rows = sample_rows(0, 2).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].cells[0].value.as_deref(), Some("9007199254740993"));
        assert_eq!(rows[0].cells[2].kind, "null");
        assert!(sample_rows(0, MAX_BATCH + 1).is_err());
        assert_eq!(sample_rows(TOTAL_ROWS - 1, 10).unwrap().len(), 1);
        assert!(sample_rows(u64::MAX, 10).unwrap().is_empty());
    }
}
