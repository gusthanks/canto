//! A side panel: the sidebar of an `Adw.OverlaySplitView`, resized by dragging.

use std::cell::Cell;

use gtk::prelude::*;

use super::{NAVIGATION_RAIL_WIDTH, PANEL_COLLAPSE_SLACK};

#[derive(Clone, Copy, Debug)]
pub struct PanelSize {
    pub min_width: i32,
    pub max_width: i32,
    pub max_fraction: f64,
    pub default_width: i32,
    /// Over the content, how much of the window it leaves uncovered (see
    /// [`PanelSize::drawer_width`]); without one, it is as wide there as
    /// beside the content
    pub overlay_peek: Option<i32>,
}

impl PanelSize {
    pub fn shown_width(&self, width: i32, window_width: i32) -> i32 {
        width
            .min((f64::from(window_width) * self.max_fraction) as i32)
            .max(self.min_width)
    }

    pub fn uncapped_width(&self, width: i32, window_width: i32, content_min_width: i32) -> i32 {
        width
            .min(window_width - content_min_width)
            .max(self.min_width)
    }

    /// Its width as a drawer over the content: as wide as set, leaving
    /// `peek` of the window uncovered.
    pub fn drawer_width(&self, width: i32, window_width: i32, peek: i32) -> i32 {
        width
            .clamp(self.min_width, self.max_width)
            .min(window_width - peek)
            .max(self.min_width)
    }

    pub fn resized_width(&self, width: i32) -> Option<i32> {
        (width >= self.min_width - PANEL_COLLAPSE_SLACK)
            .then(|| width.clamp(self.min_width, self.max_width))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
}

pub struct Panel {
    pub size: PanelSize,
    pub split: libadwaita::OverlaySplitView,
    edge: Edge,
    width: Cell<i32>,
    capped: Cell<bool>,
    content_min_width: Cell<i32>,
    contents_min_width: Cell<i32>,
    share: Cell<Option<f64>>,
    rail: Cell<bool>,
    auto_rail: Cell<bool>,
    /// Open over the content of a narrow window, at its own width
    overlay: Cell<bool>,
}

impl Panel {
    pub fn new(size: PanelSize, split: libadwaita::OverlaySplitView, edge: Edge) -> Self {
        Self {
            size,
            split,
            edge,
            width: Cell::new(size.default_width),
            capped: Cell::new(true),
            content_min_width: Cell::new(0),
            contents_min_width: Cell::new(0),
            share: Cell::new(None),
            rail: Cell::new(false),
            auto_rail: Cell::new(false),
            overlay: Cell::new(false),
        }
    }

    pub fn width(&self) -> i32 {
        self.width.get()
    }

    /// Collapsed to covers and icons (beside the content only).
    pub fn set_rail(&self, rail: bool) {
        self.rail.set(rail);
    }

    pub fn is_rail(&self) -> bool {
        (self.rail.get() || self.auto_rail.get()) && !self.split.is_collapsed()
    }

    /// The user chose the collapsed sidebar (not just squeezed into it).
    pub fn rail_chosen(&self) -> bool {
        self.rail.get()
    }

    pub fn set_overlay(&self, overlay: bool) {
        self.overlay.set(overlay);
    }

    /// Collapsed to the rail only because the full sidebar doesn't fit.
    pub fn set_auto_rail(&self, auto: bool) {
        self.auto_rail.set(auto);
    }

    pub fn least_width(&self) -> i32 {
        if self.is_rail() {
            return NAVIGATION_RAIL_WIDTH;
        }
        self.full_least_width()
    }

    /// The least width of the full (not rail) sidebar.
    pub fn full_least_width(&self) -> i32 {
        let rail_showing = self
            .split
            .sidebar()
            .is_some_and(|s| s.has_css_class("navigation-panel--rail"));
        if !rail_showing && (!self.split.is_collapsed() || self.contents_min_width.get() == 0) {
            if let Some(sidebar) = self.split.sidebar() {
                let (min, _, _, _) = sidebar.measure(gtk::Orientation::Horizontal, -1);
                self.contents_min_width.set(min);
            }
        }
        self.size.min_width.max(self.contents_min_width.get())
    }

    pub fn set_width(&self, width: i32, window_width: i32) {
        self.width.set(width);
        if window_width > 0 {
            self.share
                .set(Some(f64::from(width) / f64::from(window_width)));
        }
    }

    pub fn set_capped(&self, capped: bool) {
        self.capped.set(capped);
    }

    pub fn set_content_min_width(&self, width: i32) {
        self.content_min_width.set(width);
    }

    pub fn shown_width(&self, window_width: i32) -> i32 {
        if self.is_rail() {
            return NAVIGATION_RAIL_WIDTH;
        }
        let mut width = self.width.get();
        if let Some(share) = self.share.get().filter(|_| window_width > 0) {
            width = width.min((share * f64::from(window_width)) as i32);
        }
        self.shown_width_for(width, window_width)
    }

    pub fn shown_width_for(&self, width: i32, window_width: i32) -> i32 {
        if self.capped.get() {
            self.size.shown_width(width, window_width)
        } else {
            self.size
                .uncapped_width(width, window_width, self.content_min_width.get())
        }
    }

    pub fn apply(&self, window_width: i32) {
        let split = &self.split;
        let (min, max) = if split.is_collapsed() && self.overlay.get() {
            let width = self.width.get();
            let width = f64::from(match self.size.overlay_peek {
                Some(peek) => self.size.drawer_width(width, window_width, peek),
                None => self.size.shown_width(width, window_width),
            });
            (width, width)
        } else if split.is_collapsed() {
            (0.0, f64::from(i32::MAX))
        } else {
            let width = f64::from(self.shown_width(window_width));
            (width, width)
        };
        if split.sidebar_width_fraction() != 1.0 {
            split.set_sidebar_width_fraction(1.0);
        }
        if split.min_sidebar_width() != min || split.max_sidebar_width() != max {
            split.set_min_sidebar_width(min);
            split.set_max_sidebar_width(max);
        }
    }

    pub fn dragged_width(&self, start_width: i32, offset_x: f64) -> i32 {
        match self.edge {
            Edge::Start => start_width + offset_x as i32,
            Edge::End => start_width - offset_x as i32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_widths() {
        let size = PanelSize {
            min_width: 280,
            max_width: 600,
            max_fraction: 1.0 / 3.0,
            default_width: 360,
            overlay_peek: Some(96),
        };
        assert_eq!(size.resized_width(400), Some(400));
        assert_eq!(size.resized_width(700), Some(600));
        assert_eq!(size.resized_width(279), Some(280));
        assert_eq!(size.resized_width(280 - PANEL_COLLAPSE_SLACK), Some(280));
        assert_eq!(size.resized_width(280 - PANEL_COLLAPSE_SLACK - 1), None);

        assert_eq!(size.shown_width(400, 1500), 400);
        assert_eq!(size.shown_width(400, 900), 300);
        assert_eq!(size.shown_width(400, 600), 280);
        assert_eq!(size.shown_width(600, 3000), 600);

        assert_eq!(size.uncapped_width(400, 900, 350), 400);
        assert_eq!(size.uncapped_width(400, 700, 350), 350);
        assert_eq!(size.uncapped_width(400, 500, 350), 280);

        // A drawer keeps its width, not a third of the window, and leaves
        // 96 of it uncovered
        assert_eq!(size.drawer_width(360, 583, 96), 360);
        assert_eq!(size.drawer_width(600, 700, 96), 600);
        assert_eq!(size.drawer_width(600, 640, 96), 544);
        assert_eq!(size.drawer_width(360, 400, 96), 304);
        assert_eq!(size.drawer_width(900, 3000, 96), 600);
    }
}
