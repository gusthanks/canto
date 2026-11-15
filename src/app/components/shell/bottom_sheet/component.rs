use gtk::prelude::*;
use libadwaita::prelude::*;
use std::cell::Cell;

use super::BottomSheetContent;
use crate::app::components::QueuePageWidget;

/// Inside the scroller, so the scrollbar stays at the sheet's edge
const SHEET_MARGIN_X: i32 = 12;

/// Room for the sheet's drag handle, drawn over its content (as in bottom_sheet.blp):
/// the theme's 36px for it, and 5px more
const HANDLE_ROOM: i32 = 41;

/// About as long as the sheet takes to open
const HOLD_AT_TOP_MICROS: i64 = 500_000;

#[derive(Clone)]
pub struct BottomSheet {
    sheet: libadwaita::BottomSheet,
    content: BottomSheetContent,
}

impl BottomSheet {
    pub fn new(sheet: libadwaita::BottomSheet) -> Self {
        let content = BottomSheetContent::default();
        sheet.connect_open_notify(clone!(
            #[weak]
            content,
            move |sheet| {
                if let Some(page) = content.child().and_downcast::<QueuePageWidget>() {
                    if sheet.is_open() {
                        hold_at_top(&page);
                    }
                }
            }
        ));
        let bottom_sheet = Self { sheet, content };
        bottom_sheet.sync_can_open();
        bottom_sheet
    }

    pub fn set_page(&self, page: Option<&QueuePageWidget>) {
        if let Some(old) = self.content.child().and_downcast::<QueuePageWidget>() {
            old.set_extra_margin_x(0);
            old.set_fit_content_height(false);
        }
        if let Some(page) = page {
            page.set_extra_margin_x(SHEET_MARGIN_X);
            page.set_fit_content_height(true);
        }
        self.content.set_child(page);
        self.sheet
            .set_sheet(page.is_some().then_some(&self.content));
        self.sync_can_open();
    }

    pub fn set_selecting(&self, selecting: bool) {
        self.sheet.set_show_drag_handle(!selecting);
        self.content
            .set_margin_top(if selecting { 0 } else { HANDLE_ROOM });
    }

    pub fn is_open(&self) -> bool {
        self.sheet.is_open()
    }

    pub fn set_open(&self, open: bool) {
        self.sheet.set_open(open);
    }

    pub fn connect_open_notify<F: Fn() + 'static>(&self, f: F) {
        self.sheet.connect_open_notify(move |_| f());
    }

    fn sync_can_open(&self) {
        self.sheet.set_can_open(self.content.child().is_some());
    }
}

fn hold_at_top(page: &QueuePageWidget) {
    page.scroll_to_top();
    let start = Cell::new(None);
    page.add_tick_callback(move |page, clock| {
        page.scroll_to_top();
        let now = clock.frame_time();
        let elapsed = now - start.get().unwrap_or(now);
        start.set(start.get().or(Some(now)));
        if elapsed < HOLD_AT_TOP_MICROS {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}
