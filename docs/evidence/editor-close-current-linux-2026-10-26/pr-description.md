Closing an editor copied its full query into the recently closed tab record before discarding the editor. The close path now transfers the query allocation instead. Reopening the tab preserves its SQL text.

The regression verifies the transferred text and allocation, and the installed-app scenario closes and reopens an editor while checking its query. Local preflight passed with 2,197 unit/bin and 291 sandbox tests; app Clippy, 175 GTK widget tests and the focused close/reopen scenario passed.

Forgejo run #989 gates code commit d3566e3b210b47334cca4113f264530933485223. The run remains pending; native Wayland freeze acceptance is not established by these checks.

Release-Note: Changed: Closing SQL editor tabs uses less temporary memory.
