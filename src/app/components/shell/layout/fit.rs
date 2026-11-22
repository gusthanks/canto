//! Responsive panel fitting and unit conversion for `WindowLayout`.

use libadwaita::LengthUnit;

use super::CONTENT_CARD_GAPS;
use super::NAVIGATION_RAIL_WIDTH;

pub(super) fn sp_to_px(sp: f64) -> i32 {
    let settings = gtk::Settings::default();
    LengthUnit::Sp.to_px(sp, settings.as_ref()) as i32
}

/// Where the utility panel opens in a window this wide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UtilityMode {
    /// Beside the content
    Beside,
    /// Over the content at its own width, leaving a strip of it uncovered
    Drawer,
    /// Over the content, filling the window
    FullWindow,
}

pub(super) fn utility_mode(window_width: i32, mobile_width: i32, dock_width: i32) -> UtilityMode {
    if window_width <= mobile_width {
        UtilityMode::FullWindow
    } else if window_width < dock_width {
        UtilityMode::Drawer
    } else {
        UtilityMode::Beside
    }
}

/// Why the library shrinks to its rail of covers on its own (never saved).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AutoRail {
    /// The window is too narrow for the full library beside the content
    Narrow,
    /// Beside the utility panel, the full library would leave the content
    /// card under `DOCKED_CONTENT_MIN_WIDTH_SP`
    BesidePanel,
}

/// The widths (px) the library's rail is decided from.
#[derive(Clone, Copy, Debug)]
pub(super) struct LibraryFit {
    pub window: i32,
    /// At `MOBILE_WIDTH_SP` or less
    pub mobile: bool,
    /// The least the content takes
    pub content_min: i32,
    /// The least the full library takes
    pub library_least: i32,
    /// While the utility panel sits open beside the content: the full
    /// library's width there and the panel's
    pub beside_panel: Option<(i32, i32)>,
    /// The least the content card keeps beside the utility panel
    pub docked_content_min: i32,
}

impl LibraryFit {
    pub(super) fn full_fits(&self) -> bool {
        self.window - self.library_least >= self.content_min
    }

    pub(super) fn rail_fits(&self) -> bool {
        self.window - NAVIGATION_RAIL_WIDTH >= self.content_min
    }

    /// Beside the utility panel, the full library would leave the content
    /// card narrower than it keeps there.
    pub(super) fn squeezed_by_panel(&self) -> bool {
        self.beside_panel.is_some_and(|(library, panel)| {
            self.window - panel - library - CONTENT_CARD_GAPS < self.docked_content_min
        })
    }

    /// Whether, and why, the library shrinks to its rail on its own: not
    /// when the user chose the rail (it is on it already), nor for the
    /// utility panel once the user expanded it by hand beside the panel.
    pub(super) fn auto_rail(&self, rail_chosen: bool, expanded_by_hand: bool) -> Option<AutoRail> {
        if self.mobile || rail_chosen || !self.rail_fits() {
            None
        } else if !self.full_fits() {
            Some(AutoRail::Narrow)
        } else if self.squeezed_by_panel() && !expanded_by_hand {
            Some(AutoRail::BesidePanel)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utility_panel_floats_under_the_dock_width() {
        let mode = |width| utility_mode(width, 550, 1100);
        for width in [1920, 1200, 1100] {
            assert_eq!(mode(width), UtilityMode::Beside, "at {}", width);
        }
        for width in [1099, 960, 760, 583, 572, 551] {
            assert_eq!(mode(width), UtilityMode::Drawer, "at {}", width);
        }
        for width in [550, 360] {
            assert_eq!(mode(width), UtilityMode::FullWindow, "at {}", width);
        }
    }

    /// A window `window` px wide with the full library 280 wide (its least
    /// 270) and, when `panel` is given, the utility panel that wide beside
    /// the content.
    fn library_fit(window: i32, panel: Option<i32>) -> LibraryFit {
        LibraryFit {
            window,
            mobile: window <= 550,
            content_min: 401,
            library_least: 270,
            beside_panel: panel.map(|panel| (280, panel)),
            docked_content_min: 656,
        }
    }

    #[test]
    fn library_shrinks_to_its_rail_beside_the_side_panel() {
        let auto = |window, panel| library_fit(window, panel).auto_rail(false, false);
        // Beside the side panel (360) the full library would leave the page
        // card 448 at 1100, 548 at 1200 (a 1200 sp full screen) and 655 at 1307:
        // its rail of covers leaves 656, 756 and 863
        for window in [1100, 1200, 1307] {
            assert_eq!(
                auto(window, Some(360)),
                Some(AutoRail::BesidePanel),
                "at {}",
                window
            );
        }
        // 656 at 1308, 714 at 1366, 1268 at 1920: the full library stays
        for window in [1308, 1366, 1920] {
            assert_eq!(auto(window, Some(360)), None, "at {}", window);
        }
        // A wider side panel squeezes the page in a wider window too
        assert_eq!(auto(1366, Some(600)), Some(AutoRail::BesidePanel));
        // Closed, or floating over the page as a drawer, the panel takes no
        // room from the page
        for window in [1920, 1200, 1100, 1099, 960, 673] {
            assert_eq!(auto(window, None), None, "at {}", window);
            assert!(!library_fit(window, None).squeezed_by_panel());
        }
        assert!(library_fit(1200, Some(360)).squeezed_by_panel());
        assert!(!library_fit(1366, Some(360)).squeezed_by_panel());
    }

    #[test]
    fn the_users_own_rail_choice_wins() {
        let fit = library_fit(1200, Some(360));
        // Left alone, the side panel shrinks the library to its rail here
        assert_eq!(fit.auto_rail(false, false), Some(AutoRail::BesidePanel));
        // A rail the user chose is on already, and saved: nothing to add
        assert_eq!(fit.auto_rail(true, false), None);
        // Expanded by hand beside the panel, the library stays full
        assert_eq!(fit.auto_rail(false, true), None);
        // Narrow windows are as before: the rail from 551 to 670 here (the
        // library icon then opens the full library over the page), hidden
        // at 550 or less, whatever the hand did beside the panel
        for window in [551, 600, 670] {
            assert_eq!(
                library_fit(window, None).auto_rail(false, true),
                Some(AutoRail::Narrow),
                "at {}",
                window
            );
        }
        assert_eq!(library_fit(671, None).auto_rail(false, false), None);
        assert_eq!(library_fit(550, None).auto_rail(false, false), None);
        assert_eq!(library_fit(600, None).auto_rail(true, false), None);
    }
}
