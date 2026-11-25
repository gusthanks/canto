use crate::app::components::navigation_panel::NavigationPanelDestination;
use crate::app::components::SortOrder;
use crate::app::models::CardModel;
use crate::app::AppEvent;
use crate::app::ProvidesApi;

use super::CardListModel;

/// Trait defining the full contract for a page that displays a card list.
///
/// Extends `CardListModel` (data layer) with page-level concerns: identity,
/// empty state, sort capabilities, and event handling. Analogous to `PageModel`
/// for the details page framework.
pub trait CardListPageModel: CardListModel + ProvidesApi {
    // Page identity (used for sort persistence)

    fn page_id(&self) -> &str;

    /// What a card opens, for its right-click menu (the library's own menu).
    /// `None`: no menu.
    fn card_destination(&self, _card: &CardModel) -> Option<NavigationPanelDestination> {
        None
    }

    // Empty state

    fn empty_title(&self) -> String;
    fn empty_description(&self) -> String;
    fn empty_icon(&self) -> &str {
        "audio-x-generic-symbolic"
    }

    // Sort capabilities

    fn available_sort_orders(&self) -> &[SortOrder] {
        &[
            SortOrder::RecentlyAdded,
            SortOrder::Alphabetic,
            SortOrder::Creator,
        ]
    }

    // Event handling

    /// Returns true if this event means the card list data was updated.
    fn should_refresh(&self, event: &AppEvent) -> bool;
}
