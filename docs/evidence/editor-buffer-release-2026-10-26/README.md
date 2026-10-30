# Editor buffer teardown, October 26

Related: [#1299](https://github.com/cozyGarage/BookiE/issues/1299) and [#1333](https://github.com/cozyGarage/BookiE/issues/1333).

## Behavior

Editor shutdown releases its view and text-buffer references after the application has captured query snapshots and recently closed tab state. The configured input method remains unchanged for live editors.

## Regression coverage

The GTK regressions cover primary and split editor views. They verify that large buffers are released during component shutdown and that editor views and buffers do not remain alive through retained callbacks or input contexts. These lifetime checks do not establish a process memory ceiling or native Wayland behavior.
