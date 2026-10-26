# Structure form lifetime, 2026-10-26

On Linux base `ac190e9b2`, the shared Cancel callback and the Add Index and Add Foreign Key submit callbacks each capture their parent dialog strongly. The dialog owns those buttons, forming reference cycles after dismissal.

Two isolated GTK regressions launch the actual Structure component in new-table mode, activate each form, dismiss it and check that its weak dialog reference expires. Both fail on the original code and pass after all three callbacks use weak dialog captures. The focused checks use the simple input method and fatal GTK criticals. The source component stays alive throughout each assertion. No schema operation or database access occurs.

The fix leaves form validation and submitted metadata unchanged. App Clippy passed with warnings denied. Full preflight passed (2,197 unit tests, 291 sandbox tests and one Redis cancellation test), and all 177 isolated GTK widget tests passed in the ordinary suite environment. Forgejo acceptance is pending. [Regression evidence](validation.json) records before/after outcomes and source hashes.

Release-Note: Fixed: Dismissing index and foreign key setup releases the closed forms.

The fixture now also submits a valid index and foreign key, verifies the chosen metadata appears in the SQL preview, and checks that the submitted dialog is released. All four lifecycle assertions fail on the original code; both added submission paths passed their initial focused fixed-code runs. The expanded 179-test widget tier, full preflight and app Clippy all passed. The earlier 177-test result applies to head `f9a571a42`.

All four fixed lifecycle cases also passed three independent isolated runs each (12 of 12). These focused repetitions use the simple input method; the 179-test tier above covers the ordinary suite environment.
