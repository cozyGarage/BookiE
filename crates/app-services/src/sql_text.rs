use thiserror::Error;

pub const EDITOR_MAX_SQL_BYTES: u64 = 8 * 1024 * 1024;
pub const EDITOR_MAX_SQL_LINE_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqlTextLimits {
    max_document_bytes: usize,
    max_line_bytes: usize,
}

impl SqlTextLimits {
    pub const EDITOR: Self = Self {
        max_document_bytes: EDITOR_MAX_SQL_BYTES as usize,
        max_line_bytes: EDITOR_MAX_SQL_LINE_BYTES,
    };

    pub fn validate_document(self, text: &str) -> Result<(), SqlTextError> {
        if text.len() > self.max_document_bytes {
            return Err(SqlTextError::DocumentTooLarge);
        }
        for (index, line) in text.lines().enumerate() {
            if line.len() > self.max_line_bytes {
                return Err(SqlTextError::LineTooLong { line: index + 1 });
            }
        }
        Ok(())
    }

    pub fn validate_insertion(
        self,
        current_bytes: usize,
        line_prefix_bytes: usize,
        line_suffix_bytes: usize,
        first_line: usize,
        text: &str,
    ) -> Result<(), SqlTextError> {
        if current_bytes.saturating_add(text.len()) > self.max_document_bytes {
            return Err(SqlTextError::InsertionTooLarge);
        }
        let first_line = first_line.max(1);
        if let Some((first, rest)) = text.split_once('\n') {
            if line_prefix_bytes.saturating_add(first.len()) > self.max_line_bytes {
                return Err(SqlTextError::LineTooLong { line: first_line });
            }
            let mut lines = rest.split('\n');
            let last = lines.next_back().unwrap_or_default();
            let last_line = first_line + text.bytes().filter(|byte| *byte == b'\n').count();
            if last.len().saturating_add(line_suffix_bytes) > self.max_line_bytes {
                return Err(SqlTextError::LineTooLong { line: last_line });
            }
            if let Some((index, _)) = lines.enumerate().find(|(_, line)| line.len() > self.max_line_bytes) {
                return Err(SqlTextError::LineTooLong {
                    line: first_line + index + 1,
                });
            }
            return Ok(());
        }
        if line_prefix_bytes
            .saturating_add(text.len())
            .saturating_add(line_suffix_bytes)
            > self.max_line_bytes
        {
            return Err(SqlTextError::LineTooLong { line: first_line });
        }
        Ok(())
    }

    pub fn validate_drop(self, current: &str, cursor: usize, text: &str) -> Result<(), SqlTextError> {
        let replacing = current.trim().is_empty();
        let total = if replacing {
            text.len()
        } else {
            current.len().saturating_add(text.len())
        };
        if total > self.max_document_bytes {
            return Err(SqlTextError::InsertionTooLarge);
        }
        if replacing {
            return self.validate_document(text);
        }
        if !current.is_char_boundary(cursor) {
            return Err(SqlTextError::InvalidCursor);
        }
        let before = &current[..cursor];
        let after = &current[cursor..];
        let prefix = before.rsplit('\n').next().unwrap_or_default();
        let suffix = after.split('\n').next().unwrap_or_default();
        let first_line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
        self.validate_insertion(current.len(), prefix.len(), suffix.len(), first_line, text)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SqlTextError {
    #[error("SQL text exceeds the document limit")]
    DocumentTooLarge,
    #[error("SQL insertion exceeds the document limit")]
    InsertionTooLarge,
    #[error("SQL line {line} exceeds the line limit")]
    LineTooLong { line: usize },
    #[error("SQL insertion position is not a UTF-8 boundary")]
    InvalidCursor,
}

#[cfg(test)]
mod tests {
    use super::{EDITOR_MAX_SQL_LINE_BYTES, SqlTextError, SqlTextLimits};

    fn small_limits() -> SqlTextLimits {
        SqlTextLimits {
            max_document_bytes: 8,
            max_line_bytes: 4,
        }
    }

    #[test]
    fn document_validation_keeps_exact_utf8_and_line_boundaries() {
        let limits = small_limits();
        assert_eq!(limits.validate_document("éé\nx"), Ok(()));
        assert_eq!(
            limits.validate_document("ééé"),
            Err(SqlTextError::LineTooLong { line: 1 })
        );
        assert_eq!(
            limits.validate_document("12345\nx"),
            Err(SqlTextError::LineTooLong { line: 1 })
        );
        assert_eq!(
            limits.validate_document("1234\n5678\n9"),
            Err(SqlTextError::DocumentTooLarge)
        );
    }

    #[test]
    fn insertion_validation_checks_joined_lines_and_total_bytes() {
        let limits = small_limits();
        assert_eq!(
            limits.validate_insertion(8, 2, 2, 3, "x"),
            Err(SqlTextError::InsertionTooLarge)
        );
        assert_eq!(
            limits.validate_insertion(3, 2, 1, 3, "xy"),
            Err(SqlTextError::LineTooLong { line: 3 })
        );
        assert_eq!(limits.validate_insertion(4, 2, 1, 3, "x\ny"), Ok(()));
    }

    #[test]
    fn multiline_insertion_reports_the_resulting_document_line() {
        let limits = small_limits();
        assert_eq!(
            limits.validate_insertion(0, 0, 0, 4, "ok\n12345"),
            Err(SqlTextError::LineTooLong { line: 5 })
        );
    }

    #[test]
    fn editor_limits_are_shared_with_the_gtk_adapter() {
        assert_eq!(SqlTextLimits::EDITOR.max_document_bytes, 8 * 1024 * 1024);
        assert_eq!(SqlTextLimits::EDITOR.max_line_bytes, EDITOR_MAX_SQL_LINE_BYTES);
    }

    #[test]
    fn drop_validation_refuses_invalid_cursor_and_combined_overlong_line() {
        let limits = small_limits();
        assert_eq!(limits.validate_drop("é", 1, "x"), Err(SqlTextError::InvalidCursor));
        assert_eq!(
            limits.validate_drop("abxy", 2, "z"),
            Err(SqlTextError::LineTooLong { line: 1 })
        );
        assert_eq!(
            limits.validate_drop("1234", 4, "12345"),
            Err(SqlTextError::InsertionTooLarge)
        );
        assert_eq!(limits.validate_drop("   ", 1, "éé"), Ok(()));
    }
}
