use relm4::ComponentSender;
use relm4::gtk;

use bookie_app_services::column_selection::{SelectionCopyFormat, copy_selection, pick_columns};

use super::*;

pub(super) fn column_selection_details(names: &[String], loaded_rows: u32, stats: &str) -> String {
    let scope = bookie_app_services::tr!("Selected columns: {columns}").replace("{columns}", &names.join(", "));
    let rows = bookie_app_services::tr!("Loaded rows only: {n}").replace("{n}", &loaded_rows.to_string());
    let stats = if stats.is_empty() {
        bookie_app_services::tr!("No numeric values in the selected columns.")
    } else {
        stats.to_owned()
    };
    format!("{scope}\n{rows}\n{stats}")
}

pub(super) fn selection_buttons(sender: ComponentSender<BrowseTab>) -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    let formats = [
        (bookie_app_services::tr!("Copy TSV"), SelectionCopyFormat::Tsv),
        (bookie_app_services::tr!("Copy CSV"), SelectionCopyFormat::Csv),
        (bookie_app_services::tr!("Copy JSON"), SelectionCopyFormat::Json),
        (bookie_app_services::tr!("Copy IN list"), SelectionCopyFormat::InList),
    ];
    for (label, format) in formats {
        let button = gtk::Button::with_label(&label);
        let sender = sender.clone();
        button.connect_clicked(move |_| sender.input(BrowseTabInput::CopySelectionAs(format)));
        row.append(&button);
    }
    row
}

impl BrowseTab {
    pub(super) fn drop_column_selection(&self) -> bool {
        if self.column_selection.borrow().is_empty() {
            return false;
        }
        self.column_selection.borrow_mut().clear();
        if let Some(view) = self.current_column_view.as_ref() {
            crate::ui::grid::refresh_column_selection(view, &self.column_selection);
        }
        true
    }

    pub(super) fn refresh_selection_chrome(&self) {
        let selected = self.column_selection.borrow().columns();
        let Some(selection) = self.current_selection.as_ref() else {
            return;
        };
        if selected.is_empty() {
            let tooltip = grid_render::selection_tooltip(selection, &self.current_columns);
            update_selection_chrome(
                &self.selection_label,
                &self.selection_count_label,
                &self.selection_details_label,
                selection.selection().size() as u32,
                tooltip.as_deref(),
                self.allows_keyed_row_mutations(),
            );
            return;
        }
        let loaded = self.row_count_loaded();
        let rows: Vec<_> = (0..loaded)
            .filter_map(|position| self.row_object_at(position))
            .collect();
        let stats = selection::stats::column_stats_in(&self.current_columns, &rows, &selected);
        let names: Vec<String> = pick_columns(&self.current_columns, &selected)
            .into_iter()
            .map(|column| column.name)
            .collect();
        let count = selected.len();
        let label = bookie_app_services::trn!("{n} column selected", "{n} columns selected", count)
            .replace("{n}", &count.to_string());
        self.selection_count_label.set_label(&label);
        self.selection_label
            .update_property(&[gtk::accessible::Property::Label(&label)]);
        self.selection_details_label.set_label(&column_selection_details(
            &names,
            loaded,
            &selection::stats::tooltip(&stats),
        ));
        self.selection_label.set_visible(true);
    }

    fn in_list_column(&self) -> usize {
        self.current_column_view
            .as_ref()
            .and_then(|view| crate::ui::grid::last_clicked_cell(view))
            .map_or(0, |(_, column, _)| column)
    }

    fn selection_rows(&self, format: SelectionCopyFormat) -> Result<(Vec<ColumnInfo>, Vec<Vec<Value>>), String> {
        let incomplete = || {
            bookie_app_services::tr!(
                "Some row values were not fetched. Show hidden columns and reload before copying or exporting."
            )
        };
        let selected = self.column_selection.borrow().columns();
        let (columns, positions): (Vec<usize>, Vec<u32>) = if selected.is_empty() {
            let positions = self
                .current_selection
                .as_ref()
                .map(selected_positions)
                .unwrap_or_default();
            if positions.is_empty() {
                return Err(bookie_app_services::tr!("Select rows or a column first"));
            }
            let columns = if format == SelectionCopyFormat::InList {
                vec![self.in_list_column()]
            } else {
                (0..self.current_columns.len()).collect()
            };
            (columns, positions)
        } else {
            (selected, (0..self.row_count_loaded()).collect())
        };
        let rows =
            crate::ui::grid::project_rows(&columns, positions.into_iter(), |position| self.row_object_at(position))
                .ok_or_else(incomplete)?;
        Ok((pick_columns(&self.current_columns, &columns), rows))
    }

    pub(super) fn handle_copy_selection_as(&mut self, format: SelectionCopyFormat, sender: ComponentSender<Self>) {
        let (columns, rows) = match self.selection_rows(format) {
            Ok(selection) => selection,
            Err(message) => {
                let _ = sender.output(BrowseTabOutput::ShowToast(message));
                return;
            }
        };
        match copy_selection(format, &self.driver_id, &columns, &rows) {
            Ok(copied) => {
                let _ = sender.output(BrowseTabOutput::CopyToClipboard(copied.text));
                if copied.note.is_some() {
                    let _ = sender.output(BrowseTabOutput::ShowToast(bookie_app_services::tr!(
                        "Some values were left out of the list."
                    )));
                }
            }
            Err(_) => {
                let _ = sender.output(BrowseTabOutput::ShowToast(bookie_app_services::tr!(
                    "These values cannot be copied in this format."
                )));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::column_selection_details;

    #[test]
    fn details_name_the_columns_and_the_loaded_row_scope() {
        let text = column_selection_details(&["a".into(), "b".into()], 40, "");
        assert!(text.starts_with("Selected columns: a, b\nLoaded rows only: 40\n"));
        assert!(text.contains("No numeric values"));
    }

    #[test]
    fn details_keep_the_statistics_text() {
        let text = column_selection_details(&["n".into()], 3, "n: count 3");
        assert!(text.ends_with("n: count 3"));
    }
}
