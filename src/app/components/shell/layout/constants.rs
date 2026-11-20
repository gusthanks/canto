use super::PanelSize;

/// Under this window width, the bottom sheet's bottom bar shows.
pub const NARROW_WIDTH_SP: f64 = 764.0;
/// Under this window width, the side panels fill the window when open.
pub const MOBILE_WIDTH_SP: f64 = 550.0;

/// Under this window width, the utility panel floats over the content as a
/// drawer instead of squeezing it. 1100 = the panel's default 360 (its third
/// of the window caps it only under 1080) + the library's rail 72 + the
/// gaps between the cards 12 + a content card of 656. Inside its 24 px
/// paddings that keeps an album's header beside its cover (`max-width:
/// 510sp`, header.blp) and the album column of track rows
/// (`HIDE_ALBUM_AT_PX` 600, track_row.rs).
pub const UTILITY_DOCK_MIN_WIDTH_SP: f64 = 1100.0;

/// What the utility panel's drawer leaves uncovered: the rail and a strip
/// of the content.
pub const UTILITY_DRAWER_PEEK: i32 = 96;

/// At this width or less, the header collapses its buttons.
pub const HEADER_COMPACT_WIDTH_PX: f64 = 400.0;

/// The least the content panel keeps beside the side panels.
pub const CONTENT_PANEL_MIN_WIDTH_SP: f64 = 350.0;

/// How far under its minimum width a drag closes a panel.
pub const PANEL_COLLAPSE_SLACK: i32 = 90;

pub const BOTTOM_SHEET_MAX_WIDTH: i32 = 1280;

pub const NAVIGATION_PANEL: PanelSize = PanelSize {
    min_width: 120,
    max_width: 300,
    max_fraction: 1.0 / 3.0,
    default_width: 280,
    overlay_peek: None,
};

/// Width of the collapsed sidebar: covers and icons only.
pub const NAVIGATION_RAIL_WIDTH: i32 = 72;

pub const UTILITY_PANEL: PanelSize = PanelSize {
    min_width: 280,
    max_width: 600,
    max_fraction: 1.0 / 3.0,
    default_width: 360,
    overlay_peek: Some(UTILITY_DRAWER_PEEK),
};
