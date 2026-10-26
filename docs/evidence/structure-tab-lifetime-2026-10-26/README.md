# Structure tab lifetime, 2026-10-26

On Linux base `e5471ec71`, the generated index and foreign-key action rows capture their parent boxes. The hidden retry button also captures the stack containing the complete structure view. These references form cycles.

An isolated regression initializes the actual Structure component, drops its controller, pumps the main context and checks weak references to both generated views and the SQL preview buffer. The original code fails because the index view remains alive. Weakening only the action-row parent captures still fails. Weakening the retry stack capture as well makes all three lifetime assertions pass. The focused check uses the simple input method and fatal GTK criticals to isolate application ownership from input-context retention. It does not access a database.

Full preflight, ordinary widget validation, app Clippy and Forgejo acceptance are pending. This evidence concerns Structure tab ownership; it does not establish an aggregate process-memory ceiling.

Release-Note: Fixed: Closing structure tabs releases their views and SQL previews.

The ordinary widget tier exposed a second retention source: after the three ownership cycles were removed, the SQL preview buffer remained alive with the default input method. The preview is read-only. Selecting its simple input context makes the focused regression pass without a process-wide input-method override. Editable widgets retain their configured input methods. Full widget validation is being repeated for this additional change; earlier focused success alone did not qualify the original fix.
