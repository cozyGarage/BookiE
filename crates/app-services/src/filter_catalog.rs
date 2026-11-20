use bookie_core::{FilterOp, FilterRule, FilterValue, FilterValueKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueShape {
    None,
    Single,
    Pair,
    List,
}

#[derive(Debug, Clone, Copy)]
pub struct OperatorEntry {
    pub op: FilterOp,
    pub label: &'static str,
    pub shape: ValueShape,
}

const fn entry(op: FilterOp, label: &'static str, shape: ValueShape) -> OperatorEntry {
    OperatorEntry { op, label, shape }
}

pub const OPERATORS: [OperatorEntry; 18] = [
    entry(FilterOp::Eq, "equals", ValueShape::Single),
    entry(FilterOp::NotEq, "does not equal", ValueShape::Single),
    entry(FilterOp::Lt, "is less than", ValueShape::Single),
    entry(FilterOp::LtEq, "is at most", ValueShape::Single),
    entry(FilterOp::Gt, "is greater than", ValueShape::Single),
    entry(FilterOp::GtEq, "is at least", ValueShape::Single),
    entry(FilterOp::Contains, "contains", ValueShape::Single),
    entry(FilterOp::IContains, "contains (ignore case)", ValueShape::Single),
    entry(FilterOp::StartsWith, "starts with", ValueShape::Single),
    entry(FilterOp::EndsWith, "ends with", ValueShape::Single),
    entry(FilterOp::Like, "matches LIKE pattern", ValueShape::Single),
    entry(FilterOp::NotLike, "does not match LIKE pattern", ValueShape::Single),
    entry(FilterOp::Ilike, "matches ILIKE pattern", ValueShape::Single),
    entry(FilterOp::IsNull, "is NULL", ValueShape::None),
    entry(FilterOp::IsNotNull, "is not NULL", ValueShape::None),
    entry(FilterOp::In, "is one of", ValueShape::List),
    entry(FilterOp::NotIn, "is none of", ValueShape::List),
    entry(FilterOp::Between, "is between", ValueShape::Pair),
];

const OPS_TEXT: [OperatorEntry; 15] = [
    OperatorEntry {
        op: FilterOp::Eq,
        label: "equals",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::NotEq,
        label: "doesn't equal",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::Contains,
        label: "contains",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::IContains,
        label: "contains (case-insensitive)",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::StartsWith,
        label: "starts with",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::EndsWith,
        label: "ends with",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::Like,
        label: "LIKE",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::NotLike,
        label: "NOT LIKE",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::Ilike,
        label: "ILIKE (case-insensitive)",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::IsNull,
        label: "is empty",
        shape: ValueShape::None,
    },
    OperatorEntry {
        op: FilterOp::IsNotNull,
        label: "is not empty",
        shape: ValueShape::None,
    },
    OperatorEntry {
        op: FilterOp::In,
        label: "is one of",
        shape: ValueShape::List,
    },
    OperatorEntry {
        op: FilterOp::NotIn,
        label: "is none of",
        shape: ValueShape::List,
    },
    OperatorEntry {
        op: FilterOp::Lt,
        label: "less than (lex)",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::Gt,
        label: "greater than (lex)",
        shape: ValueShape::Single,
    },
];

const OPS_NUMERIC: [OperatorEntry; 11] = [
    OperatorEntry {
        op: FilterOp::Eq,
        label: "=",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::NotEq,
        label: "≠",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::Lt,
        label: "<",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::LtEq,
        label: "≤",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::Gt,
        label: ">",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::GtEq,
        label: "≥",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::Between,
        label: "between",
        shape: ValueShape::Pair,
    },
    OperatorEntry {
        op: FilterOp::IsNull,
        label: "is empty",
        shape: ValueShape::None,
    },
    OperatorEntry {
        op: FilterOp::IsNotNull,
        label: "is not empty",
        shape: ValueShape::None,
    },
    OperatorEntry {
        op: FilterOp::In,
        label: "is one of",
        shape: ValueShape::List,
    },
    OperatorEntry {
        op: FilterOp::NotIn,
        label: "is none of",
        shape: ValueShape::List,
    },
];

const OPS_BOOL: [OperatorEntry; 3] = [
    OperatorEntry {
        op: FilterOp::Eq,
        label: "=",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::IsNull,
        label: "is empty",
        shape: ValueShape::None,
    },
    OperatorEntry {
        op: FilterOp::IsNotNull,
        label: "is not empty",
        shape: ValueShape::None,
    },
];

const OPS_UUID: [OperatorEntry; 4] = [
    OperatorEntry {
        op: FilterOp::Eq,
        label: "=",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::NotEq,
        label: "≠",
        shape: ValueShape::Single,
    },
    OperatorEntry {
        op: FilterOp::IsNull,
        label: "is empty",
        shape: ValueShape::None,
    },
    OperatorEntry {
        op: FilterOp::IsNotNull,
        label: "is not empty",
        shape: ValueShape::None,
    },
];

pub fn operators_for_kind(kind: FilterValueKind) -> &'static [OperatorEntry] {
    match kind {
        FilterValueKind::Bool => &OPS_BOOL,
        FilterValueKind::Uuid | FilterValueKind::Json => &OPS_UUID,
        FilterValueKind::Int
        | FilterValueKind::Float
        | FilterValueKind::Decimal
        | FilterValueKind::Date
        | FilterValueKind::Time
        | FilterValueKind::DateTime
        | FilterValueKind::TimestampTz => &OPS_NUMERIC,
        FilterValueKind::Text | FilterValueKind::Bytes => &OPS_TEXT,
    }
}

pub fn operator_allowed(kind: FilterValueKind, op: FilterOp) -> bool {
    operators_for_kind(kind).iter().any(|entry| entry.op == op)
}

pub fn operator(op: FilterOp) -> OperatorEntry {
    OPERATORS
        .iter()
        .copied()
        .find(|entry| entry.op == op)
        .unwrap_or(OPERATORS[0])
}

pub fn rule_from_text(column: &str, op: FilterOp, text: &str) -> Result<FilterRule, String> {
    if column.trim().is_empty() {
        return Err(crate::tr!("Choose a column first"));
    }
    let parts = || {
        text.split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
    };
    let value = match operator(op).shape {
        ValueShape::None => None,
        ValueShape::Single => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(crate::tr!("Type a value to compare with"));
            }
            Some(FilterValue::Single(trimmed.to_owned()))
        }
        ValueShape::Pair => {
            let bounds: Vec<String> = text.split(',').map(|part| part.trim().to_owned()).collect();
            match bounds.as_slice() {
                [low, high] if !low.is_empty() && !high.is_empty() => {
                    Some(FilterValue::Pair(low.clone(), high.clone()))
                }
                _ => return Err(crate::tr!("Between needs two values separated by a comma")),
            }
        }
        ValueShape::List => {
            let items: Vec<String> = parts().collect();
            if items.is_empty() {
                return Err(crate::tr!("Type at least one value, separated by commas"));
            }
            Some(FilterValue::List(items))
        }
    };
    Ok(FilterRule {
        column: column.trim().to_owned(),
        op,
        value,
    })
}

#[cfg(test)]
#[path = "filter_catalog_tests.rs"]
mod tests;
