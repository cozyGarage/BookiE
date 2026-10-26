Closing a SQL editor could retain its text buffer and input context. Component shutdown now detaches both editor views from the old buffer and retires their input contexts. Live editors keep their configured input method.

The retained-view regression fails before buffer detachment. The default-input-method lifetime regression still fails after detachment alone, then passes when the retiring input contexts are released. The previous buffer is not cleared for other owners.

Validation: preflight passed with 2,197 unit/bin and 291 sandbox tests; 176 GTK widget tests passed; app Clippy and four installed-app close/save/discard/restore scenarios passed. The evidence packet records repeated editor cycles and separate file-refusal profiling. Native Wayland acceptance and the Forgejo gate remain pending. Broader freeze and large-SQL issues stay open.

Release-Note: Fixed: Closing SQL editors releases their text buffers and input contexts.
