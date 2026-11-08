use gtk::prelude::*;
use std::rc::Rc;

use super::UtilityPanelWidget;
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
        panel.set_pages(None, None);
        panel
    }

    pub fn set_pages(&self, queue: Option<&gtk::Widget>, lyrics: Option<&gtk::Widget>) {
        self.widget.set_pages(queue, lyrics);
        let has_content = queue.is_some() || lyrics.is_some();
        self.split
            .set_sidebar(has_content.then_some(&self.widget));
        self.widget
            .resize_handle()
            .set_visible(has_content && !self.split.is_collapsed());
    }

    #[allow(dead_code)]
    pub fn set_content(&self, content: Option<&gtk::Widget>, _title: &str) {
        self.set_pages(content, None);
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

    pub fn is_open(&self) -> bool {
        self.split.shows_sidebar()
    }

    pub fn set_open(&self, open: bool) {
        self.layout.set_utility_open(open);
    }

    pub fn connect_open_notify<F: Fn() + 'static>(&self, f: F) {
        self.split.connect_show_sidebar_notify(move |_| f());
    }
}
