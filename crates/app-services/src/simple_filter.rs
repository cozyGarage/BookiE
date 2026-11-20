use bookie_core::{FilterOp, FilterRule, FilterSet, FilterValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimpleOp {
    Contains,
    Eq,
    NotEq,
}

impl SimpleOp {
    fn filter_op(self) -> FilterOp {
        match self {
            Self::Contains => FilterOp::Contains,
            Self::Eq => FilterOp::Eq,
            Self::NotEq => FilterOp::NotEq,
        }
    }

    fn from_filter_op(op: FilterOp) -> Option<Self> {
        match op {
            FilterOp::Contains => Some(Self::Contains),
            FilterOp::Eq => Some(Self::Eq),
            FilterOp::NotEq => Some(Self::NotEq),
            _ => None,
        }
    }
}

pub fn simple_filter_set(column: &str, op: SimpleOp, text: &str) -> FilterSet {
    FilterSet {
        rules: vec![FilterRule {
            column: column.to_string(),
            op: op.filter_op(),
            value: Some(FilterValue::Single(text.to_string())),
        }],
        ..FilterSet::default()
    }
}

pub fn simple_filter_parts(set: &FilterSet) -> Option<(String, SimpleOp, String)> {
    if set.extra_sql.is_some() {
        return None;
    }
    let [rule] = set.rules.as_slice() else {
        return None;
    };
    let op = SimpleOp::from_filter_op(rule.op)?;
    let Some(FilterValue::Single(text)) = &rule.value else {
        return None;
    };
    Some((rule.column.clone(), op, text.clone()))
}

#[cfg(test)]
#[path = "simple_filter_tests.rs"]
mod tests;
