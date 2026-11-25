#[allow(clippy::module_inception)]
mod navigation_panel;
pub use navigation_panel::*;

mod navigation_panel_item;
pub use navigation_panel_item::*;

mod context_menu;
pub use context_menu::build_context_menu;
mod model;
pub use model::*;

mod navigation_panel_row;
#[cfg(test)]
pub(crate) use model::update_store_for_tests;
#[cfg(test)]
pub(crate) use navigation_panel::{grid_sample_for_tests, grid_titles_for_tests, sample_for_tests};
#[cfg(test)]
pub(crate) use navigation_panel_row::NavigationPanelRow;
