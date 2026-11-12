pub const ROUTINE_DEFINITION_LIMIT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineRef {
    pub schema: String,
    pub name: String,
    pub identity_arguments: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineDefinition {
    pub schema: String,
    pub name: String,
    pub kind: String,
    pub identity_arguments: String,
    pub definition: String,
    pub total_bytes: Option<u64>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutineLookup {
    Found(RoutineDefinition),
    NotFound,
    Ambiguous { signatures: Vec<String> },
}

pub fn cap_definition(text: &str, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text.to_string(), false);
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_definition_is_kept_whole() {
        assert_eq!(cap_definition("select 1", 64), ("select 1".to_string(), false));
    }

    #[test]
    fn a_long_definition_is_cut_on_a_character_boundary_and_flagged() {
        let (text, truncated) = cap_definition("aé€😀tail", 5);
        assert!(truncated);
        assert_eq!(text, "aé");
        assert!(text.len() <= 5);
    }

    #[test]
    fn a_zero_limit_yields_an_empty_flagged_definition() {
        assert_eq!(cap_definition("x", 0), (String::new(), true));
    }
}
