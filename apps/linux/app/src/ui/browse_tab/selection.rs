pub(in crate::ui::browse_tab) mod stats;

use relm4::gtk;
use relm4::gtk::glib;

use bookie_core::Value;

use super::*;

impl BrowseTab {
    pub(super) fn find_row_position_by_key(&self, key: &crate::services::change_tracker::RowKey) -> Option<u32> {
        self.row_store()?.position_of_key(key, &self.pk_indices())
    }

    pub(super) fn row_store(&self) -> Option<crate::ui::row_store::RowStore> {
        self.current_selection.as_ref()?.model()?.downcast().ok()
    }

    pub(super) fn pk_indices(&self) -> Vec<usize> {
        self.current_columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.primary_key)
            .map(|(i, _)| i)
            .collect()
    }

    pub(super) fn flash_error_row(&self, source: &crate::services::change_tracker::StatementSource) {
        use crate::services::change_tracker::{RowKey, StatementSource};
        let key = match source {
            StatementSource::Insert { draft_id } => RowKey::Draft(*draft_id),
            StatementSource::Update { row_key } | StatementSource::Delete { row_key } => row_key.clone(),
        };
        let Some(position) = self.find_row_position_by_key(&key) else {
            return;
        };
        if let Some(selection) = self.current_selection.as_ref() {
            selection.select_item(position, true);
        }
        if let Some(cv) = self.current_column_view.as_ref() {
            cv.scroll_to(
                position,
                None,
                gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT,
                None,
            );
        }
        let tab_id = self.tab_id;
        let generation = self
            .change_trackers
            .with_tab(tab_id, |t| t.set_error_row(key.clone()))
            .unwrap_or(0);
        self.refresh_row_at(position);
        let selection_for_clear = self.current_selection.clone();
        let change_trackers = self.change_trackers.clone();
        // Match the CSS flash duration and prevent an older timeout clearing a newer flash.
        glib::timeout_add_local_once(std::time::Duration::from_millis(1800), move || {
            let cleared = change_trackers
                .with_tab(tab_id, |t| {
                    let was_match = t.is_error_row_gen(generation);
                    t.clear_error_row_if_gen(generation);
                    was_match
                })
                .unwrap_or(false);
            if !cleared {
                return;
            }
            if let Some(selection) = selection_for_clear
                && let Some(model) = selection.model()
                && let Some(row_obj) = model
                    .item(position)
                    .and_then(|o| o.downcast::<crate::ui::row_object::RowObject>().ok())
                && let Some(store) = model.downcast::<crate::ui::row_store::RowStore>().ok()
                && let Some(store_pos) = store.find(&row_obj)
            {
                store.items_changed(store_pos, 1, 1);
            }
        });
    }

    pub(super) fn row_key_at(
        &self,
        row_position: u32,
    ) -> Option<(crate::services::change_tracker::RowKey, Vec<Value>)> {
        let row_obj = self.row_object_at(row_position)?;
        if row_obj.draft_id().is_some() {
            return None;
        }
        build_persisted_row_key(row_obj.cells_clone(), &self.pk_indices())
    }

    /// Returns true when the loaded columns include at least one PK.
    /// Used to gate Insert / Delete and reveal the no-PK banner.
    pub(super) fn has_primary_key(&self) -> bool {
        self.current_columns.iter().any(|c| c.primary_key)
    }
}

pub(super) fn build_persisted_row_key(
    cells: Vec<Value>,
    pk_indices: &[usize],
) -> Option<(crate::services::change_tracker::RowKey, Vec<Value>)> {
    let pk_values = pk_values_of(&cells, pk_indices)?;
    let key = crate::services::change_tracker::RowKey::from_pk_values(&pk_values)?;
    Some((key, cells))
}

pub(super) fn pk_values_of(cells: &[Value], pk_indices: &[usize]) -> Option<Vec<Value>> {
    pk_indices.iter().map(|&i| cells.get(i).cloned()).collect()
}

pub(super) fn row_key_is_unreadable(cells: &[Value], pk_indices: &[usize]) -> bool {
    pk_values_of(cells, pk_indices)
        .as_deref()
        .is_some_and(crate::services::change_tracker::pk_values_are_unreadable)
}

fn selection_details(n: u32, stats_tooltip: Option<&str>) -> String {
    let details_text = stats_tooltip
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| bookie_app_services::tr!("No numeric values in the selected rows."));
    let scope = bookie_app_services::tr!("Selected loaded rows: {n}").replace("{n}", &n.to_string());
    format!("{scope}\n{details_text}")
}

pub(super) fn update_selection_chrome(
    button: &gtk::MenuButton,
    count_label: &gtk::Label,
    details: &gtk::Label,
    n: u32,
    stats_tooltip: Option<&str>,
    deletable: bool,
) {
    if n <= 1 {
        button.set_visible(false);
        button.set_tooltip_text(None);
        count_label.set_label("");
        details.set_label("");
        return;
    }
    let count = n.to_string();
    let template = if deletable {
        bookie_app_services::trn!(
            "{n} selected · press Delete to remove",
            "{n} selected · press Delete to remove",
            n
        )
    } else {
        bookie_app_services::trn!("{n} selected", "{n} selected", n)
    };
    let selected_label = template.replace("{n}", &count);
    count_label.set_label(&selected_label);
    button.update_property(&[gtk::accessible::Property::Label(&selected_label)]);
    let summary = selection_details(n, stats_tooltip);
    details.set_label(&summary);
    button.set_tooltip_text(stats_tooltip.filter(|text| !text.is_empty()));
    button.set_visible(true);
}

pub(super) fn selected_positions(selection: &gtk::MultiSelection) -> Vec<u32> {
    let bitset = selection.selection();
    let mut out = Vec::with_capacity(bitset.size() as usize);
    for i in 0..bitset.size() {
        out.push(bitset.nth(i as u32));
    }
    out.sort_unstable();
    out
}

#[cfg(test)]
mod tests {
    use super::{build_persisted_row_key, row_key_is_unreadable};
    use crate::services::change_tracker::{KeyValue, RowKey};
    use bookie_core::Value;

    #[test]
    fn copied_selection_summary_names_its_loaded_row_scope() {
        let summary = super::selection_details(3, Some("age: count 3, min 1, max 9, sum 15, average 5"));
        assert_eq!(
            summary,
            "Selected loaded rows: 3\nage: count 3, min 1, max 9, sum 15, average 5"
        );
    }

    #[test]
    fn selection_summary_explains_when_no_numeric_stats_are_available() {
        assert!(super::selection_details(2, None).contains("No numeric values"));
    }

    #[test]
    pub(super) fn build_pk_single_column() {
        let cells = vec![Value::Int(42), Value::Text("alice".into())];
        let (key, returned) = build_persisted_row_key(cells.clone(), &[0]).expect("valid PK");
        assert!(matches!(key, RowKey::Persisted(_)));
        assert_eq!(returned, cells);
    }

    #[test]
    pub(super) fn build_pk_multi_column_preserves_index_order() {
        let cells = vec![
            Value::Text("eu-west".into()),
            Value::Text("ignored".into()),
            Value::Int(7),
        ];
        let (key, _) = build_persisted_row_key(cells, &[2, 0]).expect("composite PK");
        assert_eq!(
            key,
            RowKey::Persisted(vec![KeyValue::Int(7), KeyValue::Text("eu-west".into())])
        );
    }

    #[test]
    pub(super) fn build_pk_empty_indices_returns_none() {
        let cells = vec![Value::Int(1)];
        assert!(build_persisted_row_key(cells, &[]).is_none());
    }

    #[test]
    pub(super) fn build_pk_index_out_of_range_returns_none() {
        let cells = vec![Value::Int(1), Value::Text("x".into())];
        assert!(build_persisted_row_key(cells, &[5]).is_none());
    }

    #[test]
    pub(super) fn build_pk_partial_out_of_range_returns_none() {
        let cells = vec![Value::Int(1)];
        assert!(build_persisted_row_key(cells, &[0, 1]).is_none());
    }

    #[test]
    pub(super) fn build_pk_with_null_components_is_allowed() {
        let cells = vec![Value::Null, Value::Text("x".into())];
        let (key, _) = build_persisted_row_key(cells, &[0]).expect("Null PK is valid");
        assert!(matches!(key, RowKey::Persisted(_)));
    }

    #[test]
    fn a_row_whose_key_could_not_be_decoded_yields_no_key() {
        let cells = vec![Value::Undecodable("NUMERIC".into()), Value::Text("alice".into())];
        assert!(build_persisted_row_key(cells.clone(), &[0]).is_none());
        assert!(row_key_is_unreadable(&cells, &[0]));
        assert!(!row_key_is_unreadable(&cells, &[1]));
    }

    #[test]
    fn two_rows_with_the_same_undecodable_key_type_do_not_collide() {
        let first = vec![Value::Undecodable("NUMERIC".into()), Value::Text("alice".into())];
        let second = vec![Value::Undecodable("NUMERIC".into()), Value::Text("bob".into())];
        assert!(build_persisted_row_key(first, &[0]).is_none());
        assert!(build_persisted_row_key(second, &[0]).is_none());
    }
}
