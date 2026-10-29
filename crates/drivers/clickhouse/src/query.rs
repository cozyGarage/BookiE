use super::{ROW_FORMAT, map_clickhouse_error};
use tablepro_core::DriverError;

pub(super) enum RowCell {
    StringBytes(Vec<u8>),
    Json(serde_json::Value),
}

/// Reads a `ROW_FORMAT` response one line at a time so the caller can
/// stop at `max_rows` without materialising the rest of the result.
pub(super) struct LineReader {
    cursor: clickhouse::query::BytesCursor,
    buf: Vec<u8>,
    consumed: usize,
    eof: bool,
}

impl LineReader {
    pub(super) fn new(cursor: clickhouse::query::BytesCursor) -> Self {
        Self {
            cursor,
            buf: Vec::new(),
            consumed: 0,
            eof: false,
        }
    }

    pub(super) async fn next_line(&mut self) -> Result<Option<Vec<u8>>, DriverError> {
        loop {
            if let Some(idx) = self.buf[self.consumed..].iter().position(|b| *b == b'\n') {
                let end = self.consumed + idx;
                let line = self.buf[self.consumed..end].to_vec();
                self.consumed = end + 1;
                return Ok(Some(line));
            }
            if self.eof {
                let rest = self.buf[self.consumed..].to_vec();
                self.consumed = self.buf.len();
                return Ok((!rest.is_empty()).then_some(rest));
            }
            match self.cursor.next().await.map_err(map_clickhouse_error)? {
                Some(chunk) => {
                    self.buf.drain(..self.consumed);
                    self.consumed = 0;
                    self.buf.extend_from_slice(&chunk);
                }
                None => self.eof = true,
            }
        }
    }
}

pub(super) fn parse_line<T: serde::de::DeserializeOwned>(line: &[u8]) -> Result<T, DriverError> {
    serde_json::from_slice(line).map_err(|e| DriverError::Internal(format!("clickhouse response parse: {e}")))
}

pub(super) fn parse_row(line: &[u8]) -> Result<Vec<RowCell>, DriverError> {
    let mut cursor = skip_whitespace(line, 0);
    if line.get(cursor) != Some(&b'[') {
        return Err(parse_error("expected a JSON array"));
    }
    cursor += 1;
    let mut cells = Vec::new();
    loop {
        cursor = skip_whitespace(line, cursor);
        if line.get(cursor) == Some(&b']') {
            cursor += 1;
            break;
        }
        let Some(first) = line.get(cursor).copied() else {
            return Err(parse_error("unexpected end of row"));
        };
        if first == b'"' {
            let (bytes, next) = parse_string_bytes(line, cursor)?;
            cells.push(RowCell::StringBytes(bytes));
            cursor = next;
        } else {
            let end = json_value_end(line, cursor)?;
            let value = serde_json::from_slice(&line[cursor..end])
                .map_err(|error| DriverError::Internal(format!("clickhouse response parse: {error}")))?;
            cells.push(RowCell::Json(value));
            cursor = end;
        }
        cursor = skip_whitespace(line, cursor);
        match line.get(cursor) {
            Some(b',') => {
                cursor += 1;
                if line.get(skip_whitespace(line, cursor)) == Some(&b']') {
                    return Err(parse_error("trailing comma in JSON array"));
                }
            }
            Some(b']') => {
                cursor += 1;
                break;
            }
            _ => return Err(parse_error("expected a comma or closing bracket")),
        }
    }
    if skip_whitespace(line, cursor) != line.len() {
        return Err(parse_error("trailing data after row"));
    }
    Ok(cells)
}

/// ClickHouse 24.x can append an exception to a partially streamed
/// `JSONCompactEachRowWithNamesAndTypes` response as a one-cell JSON row. Do
/// not let that terminal error masquerade as a successful data row.
pub(super) fn exception_row_message(raw: &[RowCell]) -> Option<&str> {
    let [RowCell::StringBytes(message)] = raw else {
        return None;
    };
    let message = std::str::from_utf8(message).ok()?;
    let remainder = message.strip_prefix("Code: ")?;
    let (code, detail) = remainder.split_once(". ")?;
    if code.parse::<u32>().is_err()
        || !detail.contains("DB::")
        || !detail.contains("Exception:")
        || !detail.contains(" (version ")
        || !detail.ends_with("))")
    {
        return None;
    }
    Some(message)
}

fn parse_string_bytes(line: &[u8], start: usize) -> Result<(Vec<u8>, usize), DriverError> {
    let mut cursor = start + 1;
    let mut bytes = Vec::new();
    loop {
        let Some(byte) = line.get(cursor).copied() else {
            return Err(parse_error("unterminated JSON string"));
        };
        cursor += 1;
        match byte {
            b'"' => return Ok((bytes, cursor)),
            b'\\' => parse_escape(line, &mut cursor, &mut bytes)?,
            0..=0x1f => return Err(parse_error("unescaped control byte in JSON string")),
            _ => bytes.push(byte),
        }
    }
}

fn parse_escape(line: &[u8], cursor: &mut usize, bytes: &mut Vec<u8>) -> Result<(), DriverError> {
    let Some(escape) = line.get(*cursor).copied() else {
        return Err(parse_error("incomplete JSON escape"));
    };
    *cursor += 1;
    match escape {
        b'"' | b'\\' | b'/' => bytes.push(escape),
        b'b' => bytes.push(b'\x08'),
        b'f' => bytes.push(b'\x0c'),
        b'n' => bytes.push(b'\n'),
        b'r' => bytes.push(b'\r'),
        b't' => bytes.push(b'\t'),
        b'u' => append_unicode_escape(line, cursor, bytes)?,
        _ => return Err(parse_error("invalid JSON escape")),
    }
    Ok(())
}

fn append_unicode_escape(line: &[u8], cursor: &mut usize, bytes: &mut Vec<u8>) -> Result<(), DriverError> {
    let first = read_hex_quad(line, cursor)?;
    let code_point = if (0xd800..=0xdbff).contains(&first) {
        if line.get(*cursor..cursor.saturating_add(2)) != Some(b"\\u") {
            return Err(parse_error("incomplete JSON surrogate pair"));
        }
        *cursor += 2;
        let second = read_hex_quad(line, cursor)?;
        if !(0xdc00..=0xdfff).contains(&second) {
            return Err(parse_error("invalid JSON surrogate pair"));
        }
        0x10000 + ((u32::from(first) - 0xd800) << 10) + (u32::from(second) - 0xdc00)
    } else if (0xdc00..=0xdfff).contains(&first) {
        return Err(parse_error("unpaired JSON low surrogate"));
    } else {
        u32::from(first)
    };
    let Some(character) = char::from_u32(code_point) else {
        return Err(parse_error("invalid JSON Unicode escape"));
    };
    let mut encoded = [0; 4];
    bytes.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
    Ok(())
}

fn read_hex_quad(line: &[u8], cursor: &mut usize) -> Result<u16, DriverError> {
    let end = cursor.saturating_add(4);
    let Some(digits) = line.get(*cursor..end) else {
        return Err(parse_error("incomplete JSON Unicode escape"));
    };
    let text = std::str::from_utf8(digits).map_err(|_| parse_error("invalid JSON Unicode escape"))?;
    let value = u16::from_str_radix(text, 16).map_err(|_| parse_error("invalid JSON Unicode escape"))?;
    *cursor = end;
    Ok(value)
}

fn json_value_end(line: &[u8], start: usize) -> Result<usize, DriverError> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, byte) in line.iter().copied().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else {
                match byte {
                    b'\\' => escaped = true,
                    b'"' => in_string = false,
                    _ => {}
                }
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => depth += 1,
            b']' | b'}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    return Ok(offset + 1);
                }
            }
            b']' | b',' if depth == 0 => return Ok(offset),
            b' ' | b'\t' | b'\r' | b'\n' if depth == 0 => return Ok(offset),
            _ => {}
        }
    }
    if in_string || depth != 0 {
        return Err(parse_error("unterminated JSON value"));
    }
    Ok(line.len())
}

fn skip_whitespace(line: &[u8], mut cursor: usize) -> usize {
    while line
        .get(cursor)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
    {
        cursor += 1;
    }
    cursor
}

fn parse_error(message: &str) -> DriverError {
    DriverError::Internal(format!("clickhouse response parse: {message}"))
}

/// `query_id` is a caller-generated UUID, so `KILL QUERY` can name the
/// statement later. ClickHouse's HTTP interface accepts it as a request
/// parameter; the server rejects a duplicate, which is why each call
/// mints a fresh one.
pub(super) fn tag_query(query: clickhouse::query::Query, query_id: Option<&str>) -> clickhouse::query::Query {
    match query_id {
        Some(query_id) => query.with_setting("query_id", query_id),
        None => query,
    }
}

pub(super) fn new_query_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// ClickHouse has no per-request cancel channel, so the statement is
/// stopped by `KILL QUERY` naming the id the request was tagged with.
/// The id is a UUID rendered as hex and dashes, so it cannot carry SQL.
pub(super) async fn request_cancellation(client: &clickhouse::Client, query_id: &str) -> Result<(), DriverError> {
    let sql = format!("KILL QUERY WHERE query_id = '{query_id}'");
    let mut cursor = client
        .query(&sql)
        .fetch_bytes(ROW_FORMAT)
        .map_err(map_clickhouse_error)?;
    while cursor.next().await.map_err(map_clickhouse_error)?.is_some() {}
    Ok(())
}

/// ClickHouse reports an aborted statement as `QUERY_WAS_CANCELLED`
/// (code 394). The driver never populates `sqlstate`, so the code has
/// to be read out of the server's message.
pub(super) fn confirms_cancellation(error: &DriverError) -> bool {
    let DriverError::Query { message, .. } = error else {
        return false;
    };
    let lowered = message.to_lowercase();
    lowered.contains("code: 394") || lowered.contains("query was cancelled")
}
