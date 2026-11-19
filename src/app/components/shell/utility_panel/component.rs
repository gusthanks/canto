use gtk::prelude::*;
use std::rc::Rc;

use super::{PanelPages, UtilityPanelWidget};
use crate::app::components::WindowLayout;

/// The panel beside the content, at the window's end. Whatever it shows is
/// set with [`UtilityPanel::set_pages`]; without content it leaves the
/// split view, so it can't be opened.
#[derive(Clone)]
pub struct UtilityPanel {
    widget: UtilityPanelWidget,
    split: libadwaita::OverlaySplitView,
    layout: Rc<WindowLayout>,
}

impl UtilityPanel {
    pub fn new(
        widget: UtilityPanelWidget,
        split: libadwaita::OverlaySplitView,
        layout: Rc<WindowLayout>,
    ) -> Self {
        widget.wire_tabs();
        let panel = Self {
            widget,
            split,
            layout,
        };
        panel.set_pages(&PanelPages::default());
        panel
    }

    pub fn set_pages(&self, pages: &PanelPages) {
        self.widget.set_pages(pages);
        let has_content = !pages.is_empty();
        self.split.set_sidebar(has_content.then_some(&self.widget));
        self.widget
            .resize_handle()
            .set_visible(has_content && !self.split.is_collapsed());
    }

    #[allow(dead_code)]
    pub fn set_content(&self, content: Option<&gtk::Widget>, _title: &str) {
        self.set_pages(&PanelPages {
            queue: content.cloned(),
            ..Default::default()
        });
    }

    pub fn set_selection(&self, selected: Option<usize>, all_selected: bool) {
        self.widget.set_selection(selected, all_selected);
    }

    pub fn connect_selection_cancel<F: Fn() + 'static>(&self, f: F) {
        self.widget.connect_selection_cancel(f);
    }

    pub fn connect_select_all<F: Fn() + 'static>(&self, f: F) {
        self.widget.connect_select_all(f);
    }

    pub fn set_now_playing(&self, playing: bool) {
        self.widget.set_now_playing(playing);
    }

    pub fn is_open(&self) -> bool {
        self.split.shows_sidebar()
    }

    pub fn set_open(&self, open: bool) {
        self.layout.set_utility_open(open);
    }

    pub fn connect_open_notify<F: Fn() + 'static>(&self, f: F) {
        self.split.connect_show_sidebar_notify(move |_| f());
    }

    pub fn active_tab(&self) -> &'static str {
        self.widget.active_tab()
    }

    pub fn show_tab(&self, tab: &str) {
        self.widget.show_tab(tab);
    }
}
