use std::io::{self, Write};

use serde::Serialize;
use serde_json::Value;

pub(crate) fn enforce_response_budget(mut response: Value, limit: usize) -> Result<Value, String> {
    let size = encoded_size(&response)?;
    if size <= limit {
        return Ok(response);
    }
    let row_bytes = row_content_size(&response)?;
    let base_bytes = size.saturating_sub(row_bytes);
    if base_bytes > limit {
        return Err("tool response metadata exceeds the configured byte limit".into());
    }
    let mut remaining = limit - base_bytes;
    let mut truncated = false;
    fit_rows(&mut response, &mut remaining, &mut truncated)?;
    if truncated && let Some(object) = response.as_object_mut() {
        object.insert("truncated".into(), Value::Bool(true));
    }
    if encoded_size(&response)? > limit {
        return Err("tool response exceeds the configured byte limit".into());
    }
    Ok(response)
}

pub(crate) fn encoded_size<T: Serialize + ?Sized>(value: &T) -> Result<usize, String> {
    let mut counter = ByteCounter::default();
    serde_json::to_writer(&mut counter, value).map_err(|error| error.to_string())?;
    Ok(counter.0)
}

fn row_content_size(value: &Value) -> Result<usize, String> {
    if let Some(object) = value.as_object() {
        return object.iter().try_fold(0usize, |total, (key, child)| {
            let child_size = if is_row_array_key(key) {
                row_array_content_size(child)?
            } else {
                row_content_size(child)?
            };
            Ok(total.saturating_add(child_size))
        });
    }
    value.as_array().map_or(Ok(0), |array| {
        array.iter().try_fold(
            0usize,
            |total, child| Ok(total.saturating_add(row_content_size(child)?)),
        )
    })
}

fn row_array_content_size(value: &Value) -> Result<usize, String> {
    let Some(rows) = value.as_array() else {
        return Ok(0);
    };
    rows.iter().enumerate().try_fold(0usize, |total, (index, row)| {
        let row_size = encoded_size(row)?.saturating_add(usize::from(index > 0));
        Ok(total.saturating_add(row_size))
    })
}

fn fit_rows(value: &mut Value, remaining: &mut usize, truncated: &mut bool) -> Result<(), String> {
    let Some(object) = value.as_object_mut() else {
        if let Some(array) = value.as_array_mut() {
            for child in array {
                fit_rows(child, remaining, truncated)?;
            }
        }
        return Ok(());
    };
    for key in ["rows", "plan_rows"] {
        let Some(rows) = object.get_mut(key).and_then(Value::as_array_mut) else {
            continue;
        };
        let mut kept = 0usize;
        for row in rows.iter() {
            let row_bytes = encoded_size(row)?.saturating_add(usize::from(kept > 0));
            if row_bytes > *remaining {
                break;
            }
            *remaining -= row_bytes;
            kept += 1;
        }
        if kept < rows.len() {
            rows.truncate(kept);
            *truncated = true;
            if let Some(flag) = object.get_mut("truncated") {
                *flag = Value::Bool(true);
            }
        }
    }
    for (key, child) in object.iter_mut() {
        if !is_row_array_key(key) {
            fit_rows(child, remaining, truncated)?;
        }
    }
    Ok(())
}

fn is_row_array_key(key: &str) -> bool {
    matches!(key, "rows" | "plan_rows")
}

#[derive(Default)]
struct ByteCounter(usize);

impl Write for ByteCounter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0 = self.0.saturating_add(buffer.len());
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "response_budget_tests.rs"]
mod tests;
