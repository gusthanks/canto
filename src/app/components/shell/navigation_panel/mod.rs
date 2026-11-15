#[allow(clippy::module_inception)]
mod navigation_panel;
pub use navigation_panel::*;

mod navigation_panel_item;
pub use navigation_panel_item::*;

mod context_menu;
mod model;
pub use model::*;

mod navigation_panel_row;

/// The real "Now Playing" sidebar row, for rendering tests.
#[cfg(test)]
pub(crate) fn now_playing_row_for_tests() -> gtk::Widget {
    use gtk::prelude::*;
    let item = NavigationPanelItem::from_destination(NavigationPanelDestination::NowPlaying);
    navigation_panel_row::NavigationPanelRow::new(item).upcast()
}
