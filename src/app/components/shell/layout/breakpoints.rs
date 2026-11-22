//! Window-level responsive breakpoints and focus-restore viewport scanning.

use gtk::prelude::*;
use libadwaita::prelude::*;
use libadwaita::{Breakpoint, BreakpointCondition, BreakpointConditionLengthType, LengthUnit};

use super::NARROW_WIDTH_SP;

/// Frames the page doesn't scroll to the focus coming back to it.
pub(super) const SCROLL_PAUSE_FRAMES: u32 = 3;

pub(super) fn add_window_breakpoints(builder: &gtk::Builder) {
    let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();
    let bottom_sheet: gtk::Widget = builder.object("bottom_sheet").unwrap();

    // Full width, so without the side margins of its style class
    let no_classes = bottom_sheet
        .css_classes()
        .iter()
        .filter(|class| class.as_str() != "bottom-sheet--padded")
        .map(|class| class.to_string())
        .collect::<Vec<String>>()
        .to_value();

    let narrow = max_width_breakpoint(NARROW_WIDTH_SP);
    narrow.add_setter(&bottom_sheet, "full-width", Some(&true.to_value()));
    narrow.add_setter(&bottom_sheet, "css-classes", Some(&no_classes));
    // Only one window breakpoint applies at a time, so the top bar's narrow
    // look rides on this one: the search pill turns into a round button
    // beside home, both next to back and forward
    for (id, property, value) in [
        ("search_button", "visible", false.to_value()),
        ("top_search_icon", "visible", true.to_value()),
        ("top_center", "halign", gtk::Align::Start.to_value()),
    ] {
        let widget: glib::Object = builder.object(id).unwrap();
        narrow.add_setter(&widget, property, Some(&value));
    }
    window.add_breakpoint(narrow);
}

fn max_width_breakpoint(width_sp: f64) -> Breakpoint {
    Breakpoint::new(BreakpointCondition::new_length(
        BreakpointConditionLengthType::MaxWidth,
        width_sp,
        LengthUnit::Sp,
    ))
}

/// The viewports in `widget`, each scrolling to the widget that takes the
/// focus. Not inside lists: their rows hold none.
pub(super) fn viewports_in(widget: &gtk::Widget) -> Vec<gtk::Viewport> {
    let mut found = Vec::new();
    let mut stack = vec![widget.clone()];
    while let Some(widget) = stack.pop() {
        if let Some(viewport) = widget.downcast_ref::<gtk::Viewport>() {
            found.push(viewport.clone());
        }
        if widget.is::<gtk::ListView>() || widget.is::<gtk::GridView>() {
            continue;
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            stack.push(c);
        }
    }
    found
}
