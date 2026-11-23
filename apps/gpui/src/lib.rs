#![recursion_limit = "256"]
pub mod binary_mode;
pub mod core_bridge;
pub mod csv_delimiter;
pub mod detail;
pub mod edit;
pub mod export_limit;
pub mod font_settings;
pub mod keymap;
pub mod live;
pub mod messages;
pub mod model;
pub mod net_check;
pub mod palette;
pub mod preview;
pub mod pubsub_view;
pub mod selection;
pub mod shell;
pub mod source;
pub mod status_view;
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

pub mod window_geometry;
