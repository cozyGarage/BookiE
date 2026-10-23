# Empty workspace restoration after skipped records

Issue: [#1509](https://github.com/cozyGarage/BookiE/issues/1509). Baseline: Linux `0053bf339`.

The UI selects its empty state when the normalized saved tab list is empty. Empty structure-table records survived normalization but were later skipped by restored_workspace_tab. The UI therefore selected a tab workspace without restored pages, and a skipped record could also offset the saved active index.

Two normalization regressions failed before the correction: an all-skipped record list remained nonempty, and a skipped record before the active editor remained in the list. The existing retention/index-mapping helper now removes empty structure records in both legacy and modern formats, using the same selected-index remapping already used for unknown tabs. Editor drafts and valid table records are not filtered.

Headless service tests and full preflight are pending. Installed application reproduction, empty-state button activation and Forgejo acceptance remain pending. This correction does not establish resolution of the broader native freeze report #1333.
