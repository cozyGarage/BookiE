# Structure form lifetime, 2026-10-26

On Linux base `ac190e9b2`, the shared Cancel callback and the Add Index and Add Foreign Key submit callbacks each capture their parent dialog strongly. The dialog owns those buttons, forming reference cycles after dismissal.

Two isolated GTK regressions launch the actual Structure component in new-table mode, activate each form, dismiss it and check that its weak dialog reference expires. Both fail on the original code and pass after all three callbacks use weak dialog captures. The focused checks use the simple input method and fatal GTK criticals. The source component stays alive throughout each assertion. No schema operation or database access occurs.

The fix leaves form validation and submitted metadata unchanged. App Clippy passed with warnings denied. Full preflight, the ordinary widget suite and Forgejo acceptance are pending. [Regression evidence](validation.json) records before/after outcomes and source hashes.

Release-Note: Fixed: Dismissing index and foreign key setup releases the closed forms.
