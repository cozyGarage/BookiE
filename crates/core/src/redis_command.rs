pub fn split_redis_cli(input: &str) -> Result<Vec<Vec<u8>>, &'static str> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    let mut quote = None;
    let mut token_started = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quote == Some('"') => quote = None,
            '"' if quote.is_none() => {
                quote = Some('"');
                token_started = true;
            }
            '\'' if quote == Some('\'') => quote = None,
            '\'' if quote.is_none() => {
                quote = Some('\'');
                token_started = true;
            }
            '\\' if quote == Some('"') => append_double_quoted_escape(&mut chars, &mut cur)?,
            '\\' if quote == Some('\'') => append_single_quoted_escape(&mut chars, &mut cur),
            c if c.is_whitespace() && quote.is_none() => {
                if token_started {
                    out.push(std::mem::take(&mut cur));
                    token_started = false;
                }
            }
            _ => {
                let mut encoded = [0; 4];
                cur.extend_from_slice(c.encode_utf8(&mut encoded).as_bytes());
                token_started = true;
            }
        }
    }
    if quote.is_some() {
        return Err(invalid_redis_command_quoting());
    }
    if token_started {
        out.push(cur);
    }
    Ok(out)
}

fn append_double_quoted_escape(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    token: &mut Vec<u8>,
) -> Result<(), &'static str> {
    let Some(escape) = chars.next() else {
        return Err(invalid_redis_command_quoting());
    };
    match escape {
        '"' | '\\' => token.push(escape as u8),
        'n' => token.push(b'\n'),
        'r' => token.push(b'\r'),
        't' => token.push(b'\t'),
        'b' => token.push(b'\x08'),
        'a' => token.push(b'\x07'),
        'x' => {
            let Some(high) = chars.next().and_then(|digit| digit.to_digit(16)) else {
                return Err(invalid_redis_command_quoting());
            };
            let Some(low) = chars.next().and_then(|digit| digit.to_digit(16)) else {
                return Err(invalid_redis_command_quoting());
            };
            token.push(((high << 4) | low) as u8);
        }
        _ => return Err(invalid_redis_command_quoting()),
    }
    Ok(())
}

fn append_single_quoted_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, token: &mut Vec<u8>) {
    if let Some(next @ ('\'' | '\\')) = chars.peek().copied() {
        chars.next();
        token.push(next as u8);
        return;
    }
    token.push(b'\\');
}

fn invalid_redis_command_quoting() -> &'static str {
    "invalid Redis command quoting or escape"
}
