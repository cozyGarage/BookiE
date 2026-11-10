pub mod copy_format;
pub mod core_bridge;
pub mod cursor;
pub mod detail;
pub mod edit;
pub mod keymap;
pub mod live;
pub mod model;
pub mod palette;
pub mod preview;
pub mod selection;
pub mod shell;
pub mod source;
pub mod theme;
pub mod type_colors;

#[cfg(test)]
mod core_bridge_tests;
#[cfg(test)]
mod edit_tests;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod selection_tests;
