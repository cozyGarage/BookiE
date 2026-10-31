use relm4::ComponentSender;
use tablepro_core::Value;

use super::{BrowseTab, BrowseTabOutput};

impl BrowseTab {
    pub(super) fn request_cell_value(
        &mut self,
        col_index: usize,
        column_name: String,
        row_key: Vec<Value>,
        edit_row_position: Option<u32>,
        sender: ComponentSender<Self>,
    ) {
        let request = self.cell_value_requests.begin();
        *self.pending_cell_edit.borrow_mut() = edit_row_position.map(|position| (request, position, row_key.clone()));
        let _ = sender.output(BrowseTabOutput::FetchCellValue {
            request,
            col_index,
            column_name,
            row_key,
        });
    }

    pub(super) fn show_cell_value(
        &self,
        request: super::BrowseCellValueRequest,
        col_index: usize,
        column_name: String,
        value: Value,
    ) {
        if self.cell_value_requests.accepts(request)
            && let Some(view) = self.current_column_view.as_ref()
            && self
                .current_columns
                .get(col_index)
                .is_some_and(|column| column.name == column_name)
        {
            let pending_edit = self
                .pending_cell_edit
                .borrow_mut()
                .take()
                .filter(|(pending, _, _)| *pending == request);
            match pending_edit {
                Some((_, row_position, row_key)) => crate::ui::grid::present_loaded_cell_editor(
                    view,
                    crate::ui::grid::LoadedCellEdit {
                        column_name: &column_name,
                        col_index,
                        row_position,
                        row_key,
                        column: &self.current_columns[col_index],
                        value,
                        sender: self.grid_sender.clone(),
                    },
                ),
                None => crate::ui::grid::present_value_viewer(view, &column_name, value, None),
            }
        }
    }
}
