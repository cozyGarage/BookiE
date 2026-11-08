use super::column_widths::ColumnWidthStore;
use super::csv_import_mappings::CsvMappingStore;
use super::filter_settings::FilterSettingsStore;
use super::highlight_settings::HighlightSettingsStore;
use super::named_filter_presets::NamedFilterPresetsStore;

#[derive(Clone)]
pub struct PersistenceStores {
    pub column_widths: Option<ColumnWidthStore>,
    pub filter_settings: Option<FilterSettingsStore>,
    pub highlight_settings: Option<HighlightSettingsStore>,
    pub csv_import_mappings: Option<CsvMappingStore>,
    pub named_filter_presets: Option<NamedFilterPresetsStore>,
}

impl PersistenceStores {
    pub fn open() -> Self {
        Self {
            column_widths: ColumnWidthStore::open(),
            filter_settings: FilterSettingsStore::open(),
            highlight_settings: HighlightSettingsStore::open(),
            csv_import_mappings: CsvMappingStore::open(),
            named_filter_presets: NamedFilterPresetsStore::open(),
        }
    }

    pub fn flush(&self) {
        if let Some(store) = &self.column_widths
            && let Err(error) = store.flush()
        {
            tracing::warn!(%error, "column width flush failed");
        }
        if let Some(store) = &self.filter_settings
            && let Err(error) = store.flush()
        {
            tracing::warn!(%error, "filter settings flush failed");
        }
        if let Some(store) = &self.highlight_settings
            && let Err(error) = store.flush()
        {
            tracing::warn!(%error, "highlight settings flush failed");
        }
        if let Some(store) = &self.csv_import_mappings
            && let Err(error) = store.flush()
        {
            tracing::warn!(%error, "CSV import mapping flush failed");
        }
        if let Some(store) = &self.named_filter_presets
            && let Err(error) = store.flush()
        {
            tracing::warn!(%error, "named filter presets flush failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_with_no_backing_stores_is_a_noop() {
        PersistenceStores {
            column_widths: None,
            filter_settings: None,
            highlight_settings: None,
            csv_import_mappings: None,
            named_filter_presets: None,
        }
        .flush();
    }

    #[test]
    fn flush_runs_each_present_store() {
        let dir = tempfile::tempdir().unwrap();
        let stores = PersistenceStores {
            column_widths: Some(ColumnWidthStore::load_in(dir.path())),
            filter_settings: None,
            highlight_settings: None,
            csv_import_mappings: None,
            named_filter_presets: Some(NamedFilterPresetsStore::load_at(
                dir.path().join("named_filter_presets.json"),
            )),
        };
        stores.flush();
    }
}
