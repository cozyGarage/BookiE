use std::io::{self, Write};

use bookie_core::Value;

use super::super::display::value_to_search_text;

pub(super) struct CaseInsensitivePattern {
    characters: Vec<char>,
    prefix: Vec<usize>,
}

impl CaseInsensitivePattern {
    pub(super) fn new(needle: &str) -> Self {
        let characters: Vec<char> = needle.chars().collect();
        let prefix = prefix_lengths(&characters);
        Self { characters, prefix }
    }

    pub(super) fn matches(&self, text: &str) -> bool {
        let mut matched = 0;
        self.matches_from(&mut matched, text)
    }

    fn matches_from(&self, matched: &mut usize, text: &str) -> bool {
        for character in text.chars().flat_map(char::to_lowercase) {
            while *matched > 0 && character != self.characters[*matched] {
                *matched = self.prefix[*matched - 1];
            }
            if character == self.characters[*matched] {
                *matched += 1;
            }
            if *matched == self.characters.len() {
                return true;
            }
        }
        false
    }
}

pub(super) struct JsonSearchWriter<'a> {
    pattern: &'a CaseInsensitivePattern,
    matched: usize,
    found: bool,
    pending: [u8; 4],
    pending_len: usize,
}

impl<'a> JsonSearchWriter<'a> {
    pub(super) fn new(pattern: &'a CaseInsensitivePattern) -> Self {
        Self {
            pattern,
            matched: 0,
            found: false,
            pending: [0; 4],
            pending_len: 0,
        }
    }

    pub(super) fn matches(&self) -> bool {
        self.found && self.pending_len == 0
    }

    fn scan(&mut self, text: &str) {
        self.found = self.pattern.matches_from(&mut self.matched, text);
    }
}

impl Write for JsonSearchWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(bytes.len());
        }
        if self.found {
            return Err(io::ErrorKind::Other.into());
        }
        let mut rest = bytes;
        if self.pending_len > 0 {
            let width = utf8_width(self.pending[0]).ok_or(io::ErrorKind::InvalidData)?;
            let needed = width - self.pending_len;
            let copied = needed.min(rest.len());
            self.pending[self.pending_len..self.pending_len + copied].copy_from_slice(&rest[..copied]);
            self.pending_len += copied;
            rest = &rest[copied..];
            if self.pending_len < width {
                return Ok(bytes.len());
            }
            let completed = self.pending;
            let text = std::str::from_utf8(&completed[..width]).map_err(io::Error::other)?;
            self.scan(text);
            self.pending_len = 0;
            if self.found {
                return Err(io::ErrorKind::Other.into());
            }
        }
        match std::str::from_utf8(rest) {
            Ok(text) => self.scan(text),
            Err(error) => {
                let end = error.valid_up_to();
                let text = std::str::from_utf8(&rest[..end]).map_err(io::Error::other)?;
                self.scan(text);
                if self.found {
                    return Err(io::ErrorKind::Other.into());
                }
                let incomplete = &rest[end..];
                if error.error_len().is_some() || incomplete.len() > 3 {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                self.pending[..incomplete.len()].copy_from_slice(incomplete);
                self.pending_len = incomplete.len();
            }
        }
        if self.found {
            return Err(io::ErrorKind::Other.into());
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn utf8_width(first: u8) -> Option<usize> {
    match first {
        0x00..=0x7f => Some(1),
        0xc2..=0xdf => Some(2),
        0xe0..=0xef => Some(3),
        0xf0..=0xf4 => Some(4),
        _ => None,
    }
}

pub(super) fn matches_value(value: &Value, pattern: &CaseInsensitivePattern) -> bool {
    let Value::Json(json) = value else {
        return pattern.matches(&value_to_search_text(value));
    };
    let mut writer = JsonSearchWriter::new(pattern);
    let _ = serde_json::to_writer(&mut writer, json);
    writer.matches()
}

fn prefix_lengths(pattern: &[char]) -> Vec<usize> {
    let mut prefix = vec![0; pattern.len()];
    let mut matched = 0;
    for index in 1..pattern.len() {
        while matched > 0 && pattern[index] != pattern[matched] {
            matched = prefix[matched - 1];
        }
        if pattern[index] == pattern[matched] {
            matched += 1;
        }
        prefix[index] = matched;
    }
    prefix
}
