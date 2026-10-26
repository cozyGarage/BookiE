use super::{ClosedTabDescriptor, WorkspaceTab};
use relm4::ComponentController;

pub(super) fn describe_closed_tab(slot: &mut WorkspaceTab, preview: bool) -> Option<ClosedTabDescriptor> {
    match slot {
        WorkspaceTab::Editor(s) => Some(ClosedTabDescriptor::Editor {
            query: take_closed_editor_query(&mut s.query),
        }),
        WorkspaceTab::Table(_) if preview => None,
        WorkspaceTab::Table(s) => {
            let model = s.browse.model();
            let sort = model.current_sort();
            Some(ClosedTabDescriptor::Table {
                schema: s.schema.clone(),
                table: s.table.clone(),
                offset: model.current_offset(),
                page_size: model.page_size(),
                sort,
            })
        }
        WorkspaceTab::Structure(s) => match s.mode {
            crate::ui::structure_tab::StructureMode::New => None,
            crate::ui::structure_tab::StructureMode::Edit => Some(ClosedTabDescriptor::Structure {
                schema: s.schema.clone(),
                table: s.table.clone(),
            }),
        },
    }
}

fn take_closed_editor_query(query: &mut String) -> String {
    std::mem::take(query)
}

#[cfg(test)]
mod tests {
    use super::take_closed_editor_query;

    #[test]
    fn taking_a_closed_editor_query_preserves_its_text_and_allocation() {
        let mut query = "SELECT 1;".to_string();
        let allocation = query.as_ptr();

        let closed_query = take_closed_editor_query(&mut query);

        assert_eq!(closed_query, "SELECT 1;");
        assert_eq!(closed_query.as_ptr(), allocation);
        assert!(query.is_empty());
    }
}
