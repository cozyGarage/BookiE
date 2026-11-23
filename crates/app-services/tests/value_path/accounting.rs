use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub literal: String,
    pub cell: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeCases {
    pub data_type: String,
    #[serde(default)]
    pub contract: Option<String>,
    #[serde(default)]
    pub cases: Vec<Case>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub engine: String,
    pub data_type: String,
    pub step: String,
    pub signature: String,
}

pub fn account<'a>(
    expected: &BTreeMap<Key, u64>,
    failures: impl IntoIterator<Item = &'a Key>,
) -> Result<Vec<String>, String> {
    let mut unmatched = expected.clone();
    let mut reported = Vec::new();
    for failure in failures {
        match unmatched.remove(failure) {
            Some(issue) => reported.push(format!(
                "{} {} {}: #{} ({})",
                failure.engine, failure.data_type, failure.step, issue, failure.signature
            )),
            None => return Err(format!("unexpected failure: {failure:?}")),
        }
    }
    if !unmatched.is_empty() {
        return Err(format!("known failure did not occur: {unmatched:?}"));
    }
    Ok(reported)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(signature: &str) -> Key {
        Key {
            engine: "fake".into(),
            data_type: "text".into(),
            step: "browse".into(),
            signature: signature.into(),
        }
    }

    #[test]
    fn passes_clean_results_and_reports_known_failures() {
        assert!(account(&BTreeMap::new(), &[]).unwrap().is_empty());
        let known = BTreeMap::from([(key("error"), 123)]);
        assert!(account(&known, &[key("error")]).unwrap()[0].contains("#123"));
    }

    #[test]
    fn rejects_unexpected_and_newly_passing_cases() {
        let known = BTreeMap::from([(key("error"), 123)]);
        assert!(account(&known, &[key("different")]).is_err());
        assert!(account(&known, &[]).is_err());
    }

    #[test]
    fn displayed_cell_expectation_is_required() {
        assert!(serde_json::from_str::<Case>(r#"{"literal":"1"}"#).is_err());
    }
}
