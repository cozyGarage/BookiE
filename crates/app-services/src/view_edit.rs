use bookie_core::{ColumnInfo, Connection, OperationControl, ViewEditability};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewEditRestriction {
    NotUpdatable,
    KeyNotExposed,
    InsertUnavailable,
}

impl ViewEditRestriction {
    pub fn banner_text(self) -> Option<String> {
        match self {
            Self::NotUpdatable => Some(crate::tr!(
                "This view is not automatically updatable, so its rows are read-only. Use the SQL editor to change the underlying table."
            )),
            Self::KeyNotExposed => Some(crate::tr!(
                "This view does not expose every primary-key column of its table, so its rows are read-only. Add the key columns to the view to edit it."
            )),
            Self::InsertUnavailable => None,
        }
    }

    pub fn insert_reason(self) -> String {
        match self {
            Self::NotUpdatable => crate::tr!("This view is not automatically updatable. Inline editing is disabled."),
            Self::KeyNotExposed => {
                crate::tr!("This view does not expose the table's primary key. Inline editing is disabled.")
            }
            Self::InsertUnavailable => crate::tr!("This view does not accept inserts."),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViewEditPlan {
    pub columns: Vec<ColumnInfo>,
    pub restriction: Option<ViewEditRestriction>,
}

pub fn apply_view_editability(mut columns: Vec<ColumnInfo>, editability: &ViewEditability) -> ViewEditPlan {
    let restriction = match editability {
        ViewEditability::NotApplicable => None,
        ViewEditability::NotUpdatable => Some(ViewEditRestriction::NotUpdatable),
        ViewEditability::KeyNotExposed => Some(ViewEditRestriction::KeyNotExposed),
        ViewEditability::Editable {
            key_columns,
            read_only_columns,
            insertable,
        } => {
            for column in &mut columns {
                column.primary_key = key_columns.contains(&column.name);
                column.is_generated |= read_only_columns.contains(&column.name);
            }
            (!insertable).then_some(ViewEditRestriction::InsertUnavailable)
        }
    };
    ViewEditPlan { columns, restriction }
}

pub async fn resolve_view_columns(
    connection: &dyn Connection,
    schema: Option<&str>,
    table: &str,
    columns: Vec<ColumnInfo>,
    check: bool,
    control: &OperationControl,
) -> ViewEditPlan {
    if !check {
        return ViewEditPlan {
            columns,
            restriction: None,
        };
    }
    match connection
        .fetch_view_editability_controlled(schema, table, control)
        .await
    {
        Ok(editability) => apply_view_editability(columns, &editability),
        Err(error) => {
            tracing::warn!(%error, "view editability could not be read; the relation stays read-only");
            ViewEditPlan {
                columns,
                restriction: None,
            }
        }
    }
}

#[cfg(test)]
#[path = "view_edit_tests.rs"]
mod tests;
