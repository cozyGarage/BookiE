mod dialog;
mod editor;
mod selection;
mod summary;
mod ticket;

pub(in crate::ui) use editor::{CompareDone, CompareRequest};
pub(in crate::ui::editor) use selection::can_compare;

#[cfg(test)]
mod dialog_tests;
#[cfg(test)]
mod tests;
