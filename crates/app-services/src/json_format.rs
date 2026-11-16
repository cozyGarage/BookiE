use std::io::{self, Write};

pub fn pretty_json(text: &str, max_bytes: usize) -> Result<String, serde_json::Error> {
    if text.len() > max_bytes {
        return Err(serde_json::Error::io(io::Error::other(
            "JSON exceeds the formatting byte limit",
        )));
    }
    serde_json::from_str::<&serde_json::value::RawValue>(text)?;
    let mut output = vec![0; max_bytes];
    let mut remaining = output.as_mut_slice();
    write_pretty(text.as_bytes(), &mut remaining).map_err(serde_json::Error::io)?;
    let length = max_bytes - remaining.len();
    output.truncate(length);
    String::from_utf8(output).map_err(|error| serde_json::Error::io(io::Error::other(error)))
}

fn write_pretty(text: &[u8], output: &mut impl Write) -> io::Result<()> {
    let mut quoted = false;
    let mut escaped = false;
    let mut depth = 0;
    let mut previous = 0;
    for (index, &byte) in text.iter().enumerate() {
        if quoted {
            output.write_all(&[byte])?;
            if !escaped && byte == b'"' {
                quoted = false;
            }
            escaped = !escaped && byte == b'\\';
            previous = byte;
            continue;
        }
        match byte {
            b' ' | b'\t' | b'\r' | b'\n' => continue,
            b'"' => {
                quoted = true;
                output.write_all(&[byte])?;
            }
            b'{' | b'[' => {
                output.write_all(&[byte])?;
                depth += 1;
                let next = text[index + 1..].iter().find(|byte| !byte.is_ascii_whitespace());
                if !matches!(next, Some(b'}' | b']')) {
                    indent(output, depth)?;
                }
            }
            b'}' | b']' => {
                depth -= 1;
                if !matches!(previous, b'{' | b'[') {
                    indent(output, depth)?;
                }
                output.write_all(&[byte])?;
            }
            b',' => {
                output.write_all(b",")?;
                indent(output, depth)?;
            }
            b':' => output.write_all(b": ")?,
            _ => output.write_all(&[byte])?,
        }
        previous = byte;
    }
    Ok(())
}

fn indent(output: &mut impl Write, depth: usize) -> io::Result<()> {
    output.write_all(b"\n")?;
    for _ in 0..depth {
        output.write_all(b"  ")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_preserves_tokens_inside_strings_and_scalar_spellings() {
        let source = r#" {"a":1,"a":2,"n":-0.000,"e":1e+004,"s":"é\u0041\\\"{},:[]","items":[{},[],true,false,null]} "#;
        let formatted = pretty_json(source, 4096).unwrap();
        assert_eq!(
            formatted,
            r#"{
  "a": 1,
  "a": 2,
  "n": -0.000,
  "e": 1e+004,
  "s": "é\u0041\\\"{},:[]",
  "items": [
    {},
    [],
    true,
    false,
    null
  ]
}"#
        );
        assert_eq!(pretty_json(&formatted, 4096).unwrap(), formatted);
    }

    #[test]
    fn formatting_accepts_empty_containers_and_root_scalars() {
        for (source, expected) in [
            (" { } ", "{}"),
            ("[\n]", "[]"),
            (" 1e+004 ", "1e+004"),
            (" \"é\" ", "\"é\""),
        ] {
            assert_eq!(pretty_json(source, 64).unwrap(), expected);
        }
    }

    #[test]
    fn invalid_documents_are_refused_before_formatting() {
        for source in ["", "{\"a\":1,}", "[1,]", "[1] [2]", "\"\\x\"", "[01]", "{\"a\":"] {
            assert!(pretty_json(source, 1024).is_err(), "{source}");
        }
    }

    #[test]
    fn budgets_count_input_and_expanded_output_bytes() {
        assert_eq!(pretty_json("\"é\"", 4).unwrap(), "\"é\"");
        assert!(pretty_json("\"é\"", 3).is_err());
        let formatted = pretty_json("[0]", 64).unwrap();
        assert_eq!(pretty_json("[0]", formatted.len()).unwrap(), formatted);
        assert!(pretty_json("[0]", formatted.len() - 1).is_err());
        assert!(pretty_json(" [0] ", 4).is_err());
        assert!(pretty_json("0", 0).is_err());
    }
}
