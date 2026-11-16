//! How the navigation panel, the content panel and the utility panel share
//! the window's width (sizes in constants.rs):
//!
//! - A side panel is resized by dragging its inner edge, and closes when
//!   dragged well under its minimum.
//! - At `MOBILE_WIDTH_SP` or less, both collapse and fill the window when open.
//! - Without room for both beside the content panel, the one opened last
//!   stays: narrowing the window closes the utility panel first, and widening
//!   it brings it back.

mod constants;
pub use constants::*;

mod panel;
pub use panel::*;

mod clip_bin;
pub use clip_bin::expose_widgets;

use gio::prelude::*;
use gtk::prelude::*;
use libadwaita::prelude::*;
use libadwaita::{Breakpoint, BreakpointCondition, BreakpointConditionLengthType, LengthUnit};
use std::cell::Cell;
use std::rc::Rc;

use crate::app::components::{AppHeaderBar, UtilityPanelWidget};

const NAVIGATION_ACTION: &str = "show-navigation-panel";
const RAIL_ACTION: &str = "sidebar-rail";
const RAIL_CLASS: &str = "navigation-panel--rail";

fn sp_to_px(sp: f64) -> i32 {
    let settings = gtk::Settings::default();
    LengthUnit::Sp.to_px(sp, settings.as_ref()) as i32
}

#[derive(Clone, Copy, Debug)]
enum Which {
    Navigation,
    Utility,
}

pub struct WindowLayout {
    window: libadwaita::ApplicationWindow,
    navigation: Panel,
    utility: Panel,
    header: AppHeaderBar,
    closed_to_fit: Cell<bool>,
    hid_navigation: Cell<bool>,
    collapsing_navigation: Rc<Cell<bool>>,
    fitted_width: Cell<i32>,
    fitted_header_width: Cell<i32>,
    rail: RailWidgets,
}

/// What moves when the sidebar collapses to covers and icons.
struct RailWidgets {
    sidebar: gtk::Widget,
    handle: gtk::Widget,
    header: libadwaita::HeaderBar,
    rail_header: gtk::Box,
}

impl WindowLayout {
    pub fn new(builder: &gtk::Builder) -> Rc<Self> {
        add_window_breakpoints(builder);

        let navigation_split: libadwaita::OverlaySplitView =
            builder.object("navigation_split").unwrap();
        let utility_split: libadwaita::OverlaySplitView = builder.object("utility_split").unwrap();
        let navigation_handle: gtk::Widget = builder.object("navigation_resize_handle").unwrap();
        let utility_widget: UtilityPanelWidget = builder.object("utility_panel").unwrap();
        let utility_handle: gtk::Widget = utility_widget.resize_handle().clone().upcast();
        let header: AppHeaderBar = builder.object("app_header").unwrap();
        // Back and forward live in the window's top bar
        let history_slot: gtk::Box = builder.object("top_history").unwrap();
        header.set_history_slot(&history_slot);
        header.set_top_search(
            &builder.object("top_search_stack").unwrap(),
            &builder.object("top_search_slot").unwrap(),
        );
        let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();

        let rail = RailWidgets {
            sidebar: builder.object("navigation_panel").unwrap(),
            handle: navigation_handle.clone(),
            header: builder.object("navigation_panel_header").unwrap(),
            rail_header: builder.object("rail_header").unwrap(),
        };

        let navigation = Panel::new(NAVIGATION_PANEL, navigation_split.clone(), Edge::Start);
        let utility = Panel::new(UTILITY_PANEL, utility_split.clone(), Edge::End);
        let layout = Rc::new(Self {
            window: window.clone(),
            navigation,
            utility,
            header,
            closed_to_fit: Cell::new(false),
            hid_navigation: Cell::new(false),
            collapsing_navigation: Rc::new(Cell::new(false)),
            fitted_width: Cell::new(0),
            fitted_header_width: Cell::new(0),
            rail,
        });
        layout.fit();
        window.connect_realize(clone!(
            #[weak]
            layout,
            move |_| layout.fit()
        ));

        let refit_if_changed = Rc::new(clone!(
            #[weak]
            layout,
            move || {
                let width = layout.window_width();
                let header_width = layout.header.wide_width();
                if layout.fitted_width.replace(width) != width
                    || layout.fitted_header_width.replace(header_width) != header_width
                {
                    layout.fit();
                }
            }
        ));

        if let Some(shell_box) = window
            .child()
            .and_then(|child| child.downcast::<crate::app::components::ShellBox>().ok())
        {
            shell_box.connect_layout_requested(clone!(
                #[strong]
                refit_if_changed,
                move |_| refit_if_changed()
            ));
        }

        window.connect_default_width_notify(clone!(
            #[strong]
            refit_if_changed,
            move |_| refit_if_changed()
        ));
        window.connect_maximized_notify(clone!(
            #[strong]
            refit_if_changed,
            move |_| refit_if_changed()
        ));
        utility_split.connect_map(move |_| refit_if_changed());
        utility_split.connect_show_sidebar_notify(clone!(
            #[weak]
            layout,
            move |_| layout.fit()
        ));
        utility_split.connect_collapsed_notify(clone!(
            #[weak]
            layout,
            #[weak]
            utility_handle,
            move |split| {
                layout.utility_collapsed_changed();
                utility_handle.set_visible(!split.is_collapsed() && split.sidebar().is_some());
            }
        ));
        navigation_split.connect_show_sidebar_notify(clone!(
            #[weak]
            layout,
            move |_| layout.navigation_changed()
        ));
        navigation_split.connect_collapsed_notify(clone!(
            #[weak]
            layout,
            #[weak]
            navigation_handle,
            move |split| {
                layout.fit();
                navigation_handle.set_visible(!split.is_collapsed() && !layout.navigation.is_rail());
                layout.apply_rail();
            }
        ));

        let settings = gio::Settings::new(riff_config::settings::SETTINGS);
        let rail_saved = settings.boolean(riff_config::settings::NAVIGATION_RAIL);
        let rail_action =
            gio::SimpleAction::new_stateful(RAIL_ACTION, None, &rail_saved.to_variant());
        rail_action.connect_change_state(clone!(
            #[weak]
            layout,
            move |action, state| {
                let Some(rail) = state.and_then(|s| s.get::<bool>()) else {
                    return;
                };
                action.set_state(&rail.to_variant());
                let _ = settings.set_boolean(riff_config::settings::NAVIGATION_RAIL, rail);
                layout.navigation.set_rail(rail);
                layout.apply_rail();
                layout.fit();
            }
        ));
        rail_action.connect_activate(|action, _| {
            let rail = action.state().and_then(|s| s.get::<bool>()).unwrap_or(false);
            action.change_state(&(!rail).to_variant());
        });
        window.add_action(&rail_action);
        if rail_saved {
            layout.navigation.set_rail(true);
            layout.apply_rail();
            layout.fit();
        }

        layout.wire_resize(Which::Navigation, navigation_handle);
        layout.wire_resize(Which::Utility, utility_handle);
        layout
    }

    pub fn set_utility_open(&self, open: bool) {
        self.closed_to_fit.set(false);
        if open {
            self.make_room_for_utility(self.utility.width());
        }
        self.utility.split.set_show_sidebar(open);
    }

    // Without room for both, the utility panel wins
    fn make_room_for_utility(&self, width: i32) {
        if !self.utility.split.is_collapsed()
            && self.navigation_shown()
            && !self.room_for_both(self.navigation.width(), width)
        {
            self.hid_navigation.set(true);
            self.show_navigation(false);
        }
    }

    fn panel(&self, which: Which) -> &Panel {
        match which {
            Which::Navigation => &self.navigation,
            Which::Utility => &self.utility,
        }
    }

    /// Until it's allocated, the size it's about to show at
    fn window_width(&self) -> i32 {
        let width = self.utility.split.width();
        if width > 0 || self.window.is_maximized() {
            width
        } else {
            self.window.default_width().max(0)
        }
    }

    fn navigation_shown(&self) -> bool {
        self.navigation.split.shows_sidebar()
    }

    fn utility_beside(&self) -> bool {
        let split = &self.utility.split;
        split.shows_sidebar() && !split.is_collapsed()
    }

    // Through its action, so the toggle button follows
    fn show_navigation(&self, show: bool) {
        let action = self
            .utility
            .split
            .root()
            .and_downcast::<gtk::ApplicationWindow>()
            .and_then(|window| window.lookup_action(NAVIGATION_ACTION))
            .and_downcast::<gio::SimpleAction>();
        if let Some(action) = action {
            action.change_state(&show.to_variant());
        }
    }

    fn room_for_both(&self, navigation_width: i32, utility_width: i32) -> bool {
        let window_width = self.window_width();
        window_width
            - self.utility.size.shown_width(utility_width, window_width)
            - self
                .navigation
                .size
                .shown_width(navigation_width, window_width)
                .max(self.navigation.least_width())
            >= self.content_min_width()
    }

    fn room_for_navigation(&self) -> bool {
        self.room_for_both(self.navigation.width(), self.utility.width())
    }

    fn room_for_utility(&self) -> bool {
        let window_width = self.window_width();
        window_width
            - self
                .utility
                .size
                .shown_width(self.utility.width(), window_width)
            >= self.content_min_width()
    }

    fn content_min_width(&self) -> i32 {
        sp_to_px(CONTENT_PANEL_MIN_WIDTH_SP)
            .max(self.header.wide_width())
            .max(HEADER_COMPACT_WIDTH_PX as i32 + 1)
    }

    fn collapse_utility_to_fit(&self) {
        let window_width = self.window_width();
        if window_width <= 0 {
            return;
        }
        let collapse = window_width <= sp_to_px(MOBILE_WIDTH_SP);
        if self.utility.split.is_collapsed() != collapse {
            self.utility.split.set_collapsed(collapse);
        }
    }

    fn collapse_navigation_to_fit(&self) {
        let window_width = self.window_width();
        if window_width <= 0 {
            return;
        }
        let collapse = window_width <= sp_to_px(MOBILE_WIDTH_SP)
            || window_width - self.navigation.least_width() < self.content_min_width();
        let split = &self.navigation.split;
        let collapsing = &self.collapsing_navigation;
        if !collapse {
            if collapsing.replace(false) {
                split.set_show_sidebar(true);
            }
            if split.is_collapsed() {
                split.set_collapsed(false);
            }
            return;
        }
        if split.is_collapsed() || collapsing.get() {
            return;
        }
        if !split.shows_sidebar() {
            split.set_collapsed(true);
            return;
        }
        collapsing.set(true);
        split.set_show_sidebar(false);
        let collapsing = Rc::clone(collapsing);
        split.add_tick_callback(move |split, _| {
            if !collapsing.get() {
                return glib::ControlFlow::Break;
            }
            let slid_out = split
                .sidebar()
                .and_then(|sidebar| sidebar.parent())
                .is_none_or(|parent| !parent.is_child_visible());
            if !slid_out {
                return glib::ControlFlow::Continue;
            }
            collapsing.set(false);
            split.set_collapsed(true);
            glib::ControlFlow::Break
        });
    }

    fn fit(&self) {
        self.collapse_utility_to_fit();
        let split = &self.utility.split;
        if !split.is_collapsed() {
            let open = split.shows_sidebar();
            let room = self.room_for_navigation();
            let navigation_shown = self.navigation_shown();
            if open && !room && navigation_shown {
                self.closed_to_fit.set(true);
                split.set_show_sidebar(false);
            } else if !open
                && self.closed_to_fit.get()
                && (room || (!navigation_shown && self.room_for_utility()))
            {
                self.closed_to_fit.set(false);
                split.set_show_sidebar(true);
            } else if self.hid_navigation.get() && (room || !open) {
                self.hid_navigation.set(false);
                self.show_navigation(true);
            }
        }
        self.collapse_navigation_to_fit();
        self.apply();
    }

    /// Swaps the sidebar's header for the rail's, and marks the sidebar so its
    /// rows show covers and icons only.
    fn apply_rail(&self) {
        let rail = self.navigation.is_rail();
        let w = &self.rail;
        if rail == w.sidebar.has_css_class(RAIL_CLASS) {
            return;
        }
        if rail {
            w.sidebar.add_css_class(RAIL_CLASS);
        } else {
            w.sidebar.remove_css_class(RAIL_CLASS);
        }
        w.header.set_visible(!rail);
        w.rail_header.set_visible(rail);
        w.handle
            .set_visible(!rail && !self.navigation.split.is_collapsed());
    }

    fn apply(&self) {
        let window_width = self.window_width();
        self.navigation.set_capped(self.utility_beside());
        self.navigation
            .set_content_min_width(self.content_min_width());
        let navigation = &self.navigation.split;
        self.header.place_history_buttons(true);
        self.header.set_compact_allowed(
            (navigation.is_collapsed() || !navigation.shows_sidebar())
                && !self.collapsing_navigation.get(),
        );
        self.utility.apply(window_width);
        self.navigation.apply(window_width);
    }

    fn navigation_changed(&self) {
        let split = &self.utility.split;
        if self.navigation_shown()
            && !self.navigation.split.is_collapsed()
            && !split.is_collapsed()
            && split.shows_sidebar()
            && !self.room_for_navigation()
        {
            self.hid_navigation.set(false);
            self.closed_to_fit.set(false);
            split.set_show_sidebar(false);
        }
        self.apply();
    }

    fn utility_collapsed_changed(&self) {
        let split = &self.utility.split;
        if split.is_collapsed() && split.shows_sidebar() {
            self.closed_to_fit.set(true);
            split.set_show_sidebar(false);
        }
        self.fit();
    }

    fn close(&self, which: Which) {
        match which {
            Which::Navigation => {
                self.hid_navigation.set(false);
                self.show_navigation(false);
            }
            Which::Utility => self.set_utility_open(false),
        }
    }

    fn wire_resize(self: &Rc<Self>, which: Which, handle: gtk::Widget) {
        handle.set_cursor_from_name(Some("col-resize"));
        let split = self.panel(which).split.clone();
        let drag = gtk::GestureDrag::new();
        let start_width = Rc::new(Cell::new(0));
        let layout = Rc::downgrade(self);
        drag.connect_drag_begin(clone!(
            #[strong]
            start_width,
            #[strong]
            layout,
            #[weak]
            handle,
            move |drag, x, y| {
                let Some(layout) = layout.upgrade() else {
                    return;
                };
                let panel = layout.panel(which);
                let split = &panel.split;
                let on_handle = handle.is_mapped()
                    && !panel.split.is_collapsed()
                    && handle.compute_bounds(split).is_some_and(|b| {
                        b.contains_point(&gtk::graphene::Point::new(x as f32, y as f32))
                    });
                if !on_handle {
                    drag.set_state(gtk::EventSequenceState::Denied);
                    return;
                }
                drag.set_state(gtk::EventSequenceState::Claimed);
                start_width.set(panel.shown_width(layout.window_width()));
            }
        ));
        drag.connect_drag_update(clone!(
            #[strong]
            start_width,
            #[strong]
            layout,
            move |drag, offset_x, _| {
                let Some(layout) = layout.upgrade() else {
                    return;
                };
                let panel = layout.panel(which);
                let width = panel.dragged_width(start_width.get(), offset_x);
                match panel.size.resized_width(width) {
                    Some(width) => {
                        let width = panel.shown_width_for(width, layout.window_width());
                        let fits = match which {
                            Which::Utility => {
                                layout.make_room_for_utility(width);
                                true
                            }
                            Which::Navigation => {
                                !layout.utility.split.shows_sidebar()
                                    || layout.room_for_both(width, layout.utility.width())
                            }
                        };
                        if fits {
                            panel.set_width(width, layout.window_width());
                        }
                        layout.fit();
                    }
                    None => {
                        layout.close(which);
                        drag.set_state(gtk::EventSequenceState::Denied);
                    }
                }
            }
        ));
        split.add_controller(drag);
    }
}

fn add_window_breakpoints(builder: &gtk::Builder) {
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
    window.add_breakpoint(narrow);
}

fn max_width_breakpoint(width_sp: f64) -> Breakpoint {
    Breakpoint::new(BreakpointCondition::new_length(
        BreakpointConditionLengthType::MaxWidth,
        width_sp,
        LengthUnit::Sp,
    ))
}
