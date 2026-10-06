# SonarCloud triage

## GTK CSS node selectors

SonarCloud reports eight “Unknown type selector” findings on
`data/resources/style.css:25-26` for `columnview`, `listview`, `row` and
`cell`.

These are GTK CSS node names in the focus-ring selector. GTK documents
`columnview`, `listview` and `row` as nodes in the `GtkColumnView` /
`GtkListView` tree. GTK's `GtkColumnViewCellWidget` source sets its CSS node
name to `cell`. The stylesheet targets those nodes to draw the grid-cell focus
ring; converting them to classes would stop matching the widget tree.

Disposition: confirmed framework-specific false positives from the web CSS
analyzer. Keep the GTK selectors unchanged. The SonarCloud findings still need
their status updated in the project dashboard; no source suppression was added.

- [GTK ColumnView CSS nodes](https://docs.gtk.org/gtk4/class.ColumnView.html)
- [GTK ListView CSS nodes](https://docs.gtk.org/gtk4/class.ListView.html)
- [GTK ColumnView cell CSS name in GTK source](https://github.com/GNOME/gtk/blob/main/gtk/gtkcolumnviewcellwidget.c#L282-L299)
