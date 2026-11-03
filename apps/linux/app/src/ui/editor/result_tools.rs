mod actions;
mod cell_filter;
mod cursor;
mod derived;
mod editor;
mod jump;
mod ticket;
mod widget;

pub(in crate::ui::editor) use cell_filter::stream_identity;
#[cfg(test)]
pub(in crate::ui::editor) use derived::derived_outcome;
pub(in crate::ui) use editor::{FilterDone, FilterRequest};
pub(in crate::ui::editor) use jump::continuous_container_name;
pub(in crate::ui::editor) use widget::{ToolsTarget, append};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod widget_tests;
