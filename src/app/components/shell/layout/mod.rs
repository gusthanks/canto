//! How the navigation panel, the content panel and the utility panel share
//! the window's width (sizes in constants.rs):
//!
//! - A side panel is resized by dragging its inner edge, and closes when
//!   dragged well under its minimum.
//! - Under `UTILITY_DOCK_MIN_WIDTH_SP`, the utility panel floats over the
//!   content as a drawer instead of squeezing it, so the content keeps the
//!   window's width. Narrowing the window under it closes the panel, and
//!   widening it back brings the panel back beside the content.
//! - Floating, it closes with its button, Esc, a click on the dimmed content
//!   or a move to another page, and the keyboard focus it had goes back to
//!   the content widget that had it before (or the content's first), with
//!   the content staying where it was scrolled.
//! - At `MOBILE_WIDTH_SP` or less, both collapse and fill the window when open.
//! - One panel over the content at a time: the floating utility panel, the
//!   sidebar opened over the content, the queue sheet (panel_manager).
//! - Too narrow for the full library beside the content, the library shrinks
//!   to its rail of covers. So it does beside the utility panel when the full
//!   library would leave the content card under `DOCKED_CONTENT_MIN_WIDTH_SP`,
//!   until the panel closes (once it has slid out) or the window grows (the
//!   library icon brings it back beside the panel until then). Neither is
//!   saved: the user's own choice of rail comes back.
//! - The full library opened over the content from its rail closes with its
//!   own collapse button (nothing saved), and once the window is wide enough
//!   for it beside the content.
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
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::app::components::{AppHeaderBar, UtilityPanelWidget};

const NAVIGATION_ACTION: &str = "show-navigation-panel";
const RAIL_ACTION: &str = "sidebar-rail";
const RAIL_CLASS: &str = "navigation-panel--rail";
/// On the utility split while its panel floats over the content as a drawer
/// (app.css); filling a window of `MOBILE_WIDTH_SP` or less, it is flush
const FLOATING_CLASS: &str = "utility-split--floating";
/// The controllers that close the floating utility panel: Esc on the
/// window, a click on the dimmed content (the smoke test finds them by name)
const FLOATING_ESCAPE: &str = "canto-floating-panel-escape";
const FLOATING_CLICK: &str = "canto-floating-panel-click";

fn sp_to_px(sp: f64) -> i32 {
    let settings = gtk::Settings::default();
    LengthUnit::Sp.to_px(sp, settings.as_ref()) as i32
}

/// Where the utility panel opens in a window this wide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UtilityMode {
    /// Beside the content
    Beside,
    /// Over the content at its own width, leaving a strip of it uncovered
    Drawer,
    /// Over the content, filling the window
    FullWindow,
}

fn utility_mode(window_width: i32, mobile_width: i32, dock_width: i32) -> UtilityMode {
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
enum AutoRail {
    /// The window is too narrow for the full library beside the content
    Narrow,
    /// Beside the utility panel, the full library would leave the content
    /// card under `DOCKED_CONTENT_MIN_WIDTH_SP`
    BesidePanel,
}

/// The widths (px) the library's rail is decided from.
#[derive(Clone, Copy, Debug)]
struct LibraryFit {
    window: i32,
    /// At `MOBILE_WIDTH_SP` or less
    mobile: bool,
    /// The least the content takes
    content_min: i32,
    /// The least the full library takes
    library_least: i32,
    /// While the utility panel sits open beside the content: the full
    /// library's width there and the panel's
    beside_panel: Option<(i32, i32)>,
    /// The least the content card keeps beside the utility panel
    docked_content_min: i32,
}

impl LibraryFit {
    fn full_fits(&self) -> bool {
        self.window - self.library_least >= self.content_min
    }

    fn rail_fits(&self) -> bool {
        self.window - NAVIGATION_RAIL_WIDTH >= self.content_min
    }

    /// Beside the utility panel, the full library would leave the content
    /// card narrower than it keeps there.
    fn squeezed_by_panel(&self) -> bool {
        self.beside_panel.is_some_and(|(library, panel)| {
            self.window - panel - library - CONTENT_CARD_GAPS < self.docked_content_min
        })
    }

    /// Whether, and why, the library shrinks to its rail on its own: not
    /// when the user chose the rail (it is on it already), nor for the
    /// utility panel once the user expanded it by hand beside the panel.
    fn auto_rail(&self, rail_chosen: bool, expanded_by_hand: bool) -> Option<AutoRail> {
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

#[derive(Clone, Copy, Debug)]
enum Which {
    Navigation,
    Utility,
}

pub struct WindowLayout {
    window: libadwaita::ApplicationWindow,
    /// The window's content: its width is the window's as soon as its
    /// allocation starts, before the panels in it are allocated
    shell_box: Option<glib::WeakRef<crate::app::components::ShellBox>>,
    navigation: Panel,
    utility: Panel,
    header: AppHeaderBar,
    closed_to_fit: Cell<bool>,
    hid_navigation: Cell<bool>,
    collapsing_navigation: Rc<Cell<bool>>,
    /// The full sidebar is open over the content of a narrow window
    navigation_overlay: Cell<bool>,
    /// The sidebar is collapsing to open over the content: the refit that
    /// collapsing runs leaves it collapsed
    opening_navigation_overlay: Cell<bool>,
    /// The content's pages: the visible one takes the keyboard focus the
    /// floating utility panel had
    pages: gtk::Stack,
    /// Where the keyboard focus was on the content when the utility panel
    /// opened over it: it goes back there when the panel closes
    focus_before_utility: RefCell<Option<glib::WeakRef<gtk::Widget>>>,
    /// The utility panel, closed beside the content, is still sliding out:
    /// until it has, the library is laid out as beside it
    utility_sliding_out: Cell<bool>,
    /// Why the library is on its rail without the user choosing it
    auto_rail: Cell<Option<AutoRail>>,
    /// The user expanded the library while the utility panel beside the
    /// content had it on its rail: it stays full until the panel stops
    /// squeezing it (closed, or a wider window)
    expanded_by_hand: Cell<bool>,
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
    top_bar: gtk::Widget,
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
        let pages: gtk::Stack = builder.object("content_stack").unwrap();
        // Back and forward live in the window's top bar
        header.set_content(&pages);
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
            top_bar: builder.object("top_bar").unwrap(),
        };

        let navigation = Panel::new(NAVIGATION_PANEL, navigation_split.clone(), Edge::Start);
        let utility = Panel::new(UTILITY_PANEL, utility_split.clone(), Edge::End);
        // Its content, not its child: libadwaita puts a dialog host between
        // the window and the content
        let shell_box = window
            .content()
            .and_then(|child| child.downcast::<crate::app::components::ShellBox>().ok());
        let layout = Rc::new(Self {
            window: window.clone(),
            shell_box: shell_box.as_ref().map(|shell_box| shell_box.downgrade()),
            navigation,
            utility,
            header,
            closed_to_fit: Cell::new(false),
            hid_navigation: Cell::new(false),
            collapsing_navigation: Rc::new(Cell::new(false)),
            navigation_overlay: Cell::new(false),
            opening_navigation_overlay: Cell::new(false),
            pages: pages.clone(),
            focus_before_utility: RefCell::new(None),
            utility_sliding_out: Cell::new(false),
            auto_rail: Cell::new(None),
            expanded_by_hand: Cell::new(false),
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

        // A new width refits before the panels are allocated for it
        if let Some(shell_box) = shell_box {
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
            move |split| {
                if !split.shows_sidebar() && !split.is_collapsed() {
                    layout.follow_utility_sliding_out();
                }
                layout.fit()
            }
        ));
        // Another page while the utility panel floats over the content (a
        // shortcut, a link in the panel): the panel closes, so the new page
        // can take the keyboard focus, which it can't under the panel
        pages.connect_visible_child_notify(clone!(
            #[weak]
            layout,
            move |_| {
                if layout.utility_floating_open() {
                    layout.set_utility_open(false);
                }
            }
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
                navigation_handle
                    .set_visible(!split.is_collapsed() && !layout.navigation.is_rail());
                layout.apply_rail();
            }
        ));

        let settings = gio::Settings::new(canto_config::settings::SETTINGS);
        let rail_saved = settings.boolean(canto_config::settings::NAVIGATION_RAIL);
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
                let _ = settings.set_boolean(canto_config::settings::NAVIGATION_RAIL, rail);
                // Expanded by hand, it stays full beside the utility panel too
                layout.expanded_by_hand.set(!rail);
                layout.navigation.set_rail(rail);
                layout.apply_rail();
                layout.fit();
            }
        ));
        rail_action.connect_activate(clone!(
            #[weak]
            layout,
            move |action, _| {
                // The full sidebar open over the content (from its rail, or
                // its button at `MOBILE_WIDTH_SP` or less): its own collapse
                // button closes it, and the saved choice stays as it was
                if layout.navigation_over_content() {
                    layout.navigation.split.set_show_sidebar(false);
                    return;
                }
                if layout.navigation.is_rail() && !layout.navigation.rail_chosen() {
                    if layout.auto_rail.get() == Some(AutoRail::BesidePanel) {
                        // On its rail for the utility panel: the library icon
                        // brings the full library back beside the panel
                        layout.expanded_by_hand.set(true);
                        layout.fit();
                    } else {
                        // Squeezed to the rail by a narrow window: the library
                        // icon opens the full sidebar over the content for a
                        // moment
                        layout.open_navigation_overlay();
                    }
                    return;
                }
                let rail = action
                    .state()
                    .and_then(|s| s.get::<bool>())
                    .unwrap_or(false);
                action.change_state(&(!rail).to_variant());
            }
        ));
        // Closing the sidebar opened over the content brings the rail back
        layout.navigation.split.connect_show_sidebar_notify(clone!(
            #[weak]
            layout,
            move |split| {
                if !split.shows_sidebar() && layout.navigation_overlay.replace(false) {
                    layout.navigation.set_overlay(false);
                    split.set_collapsed(false);
                    layout.fit();
                }
            }
        ));
        window.add_action(&rail_action);
        if rail_saved {
            layout.navigation.set_rail(true);
            layout.apply_rail();
            layout.fit();
        }

        Self::align_top_center_with_sidebar(builder);
        Self::clip_card_panes(builder);
        layout.wire_resize(Which::Navigation, navigation_handle);
        layout.wire_resize(Which::Utility, utility_handle);
        layout.close_floating_on_escape();
        layout.close_floating_on_click_outside();
        layout
    }

    pub fn set_utility_open(&self, open: bool) {
        self.closed_to_fit.set(false);
        if !open {
            self.hide_utility();
            return;
        }
        // One over the content at a time: floating, it closes the sidebar
        // opened over the content
        if self.utility.split.is_collapsed() && self.navigation_over_content() {
            self.navigation.split.set_show_sidebar(false);
        }
        // Where the keyboard focus goes back to when it closes again
        let split = &self.utility.split;
        if split.is_collapsed() && !split.shows_sidebar() {
            let focus = GtkWindowExt::focus(&self.window).filter(|f| f.is_ancestor(&self.pages));
            self.focus_before_utility
                .replace(focus.map(|f| f.downgrade()));
        }
        self.make_room_for_utility(self.utility.width());
        self.utility.split.set_show_sidebar(true);
    }

    /// Closes the utility panel. Floating, it hands the keyboard focus it
    /// has back to the page (`focus_page`), which stays where it was
    /// scrolled: left to libadwaita, the focus would go back to the widget
    /// that had it before and scroll the page back to it, or go to the
    /// sidebar's first row and scroll the sidebar to its top.
    fn hide_utility(&self) {
        let split = &self.utility.split;
        let focus_inside = split.is_collapsed() && self.focus_in_utility();
        if focus_inside {
            // Out of the panel first, or libadwaita moves it on closing
            GtkWindowExt::set_focus(&self.window, None::<&gtk::Widget>);
        }
        split.set_show_sidebar(false);
        // The content takes the focus again once nothing floats over it
        if focus_inside {
            self.focus_page();
        }
    }

    fn focus_in_utility(&self) -> bool {
        let Some(panel) = self.utility.split.sidebar() else {
            return false;
        };
        GtkWindowExt::focus(&self.window)
            .is_some_and(|focus| focus == panel || focus.is_ancestor(&panel))
    }

    /// Moves the keyboard focus back to the visible page, without scrolling
    /// the page: to the widget that had it when the utility panel opened, if
    /// it is still there (a keyboard user keeps their place), else to the
    /// page's first widget.
    fn focus_page(&self) {
        let before = self
            .focus_before_utility
            .take()
            .and_then(|widget| widget.upgrade());
        let Some(page) = self.pages.visible_child() else {
            return;
        };
        let viewports = viewports_in(&page);
        let scrolling: Vec<bool> = viewports.iter().map(|v| v.is_scroll_to_focus()).collect();
        for viewport in &viewports {
            viewport.set_scroll_to_focus(false);
        }
        let back = before
            .filter(|widget| widget.is_ancestor(&page) && widget.is_mapped())
            .is_some_and(|widget| widget.grab_focus());
        if !back {
            page.child_focus(gtk::DirectionType::TabForward);
        }
        for (viewport, scroll) in viewports.iter().zip(scrolling) {
            viewport.set_scroll_to_focus(scroll);
        }
    }

    /// The utility panel open over the content (as a drawer, or filling
    /// the window).
    fn utility_floating_open(&self) -> bool {
        let split = &self.utility.split;
        split.is_collapsed() && split.shows_sidebar()
    }

    /// The full sidebar open over the content (narrow windows).
    fn navigation_over_content(&self) -> bool {
        self.navigation.split.is_collapsed() && self.navigation_shown()
    }

    /// Esc closes the utility panel floating over the content, as its
    /// button does. On the window and bubbling: what has the focus (a text
    /// field, a popover) takes Esc first. libadwaita's split view closes on
    /// Esc too, but this runs before it (newer controllers run first) and
    /// hands the focus to the page.
    fn close_floating_on_escape(self: &Rc<Self>) {
        let layout = Rc::downgrade(self);
        let close = gtk::CallbackAction::new(move |_, _| match layout.upgrade() {
            Some(layout) if layout.utility_floating_open() => {
                layout.set_utility_open(false);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
        let shortcuts = gtk::ShortcutController::new();
        shortcuts.set_name(Some(FLOATING_ESCAPE));
        shortcuts.set_propagation_phase(gtk::PropagationPhase::Bubble);
        shortcuts.add_shortcut(gtk::Shortcut::new(
            Some(gtk::KeyvalTrigger::new(
                gtk::gdk::Key::Escape,
                gtk::gdk::ModifierType::empty(),
            )),
            Some(close),
        ));
        self.window.add_controller(shortcuts);
    }

    /// A click on the content dimmed under the floating utility panel (or
    /// on the dimmed frame around its card) closes it, as its button does.
    /// libadwaita's split view closes on that click too, but then moves the
    /// focus as `hide_utility` says: this runs first (capture) and keeps
    /// the click.
    fn close_floating_on_click_outside(self: &Rc<Self>) {
        let click = gtk::GestureClick::new();
        click.set_name(Some(FLOATING_CLICK));
        click.set_button(0);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let layout = Rc::downgrade(self);
        click.connect_pressed(move |gesture, _, x, y| {
            let Some(layout) = layout.upgrade() else {
                return;
            };
            let split = &layout.utility.split;
            let on_panel = split.sidebar().is_some_and(|panel| {
                split
                    .compute_point(&panel, &gtk::graphene::Point::new(x as f32, y as f32))
                    .is_some_and(|p| panel.contains(f64::from(p.x()), f64::from(p.y())))
            });
            if !layout.utility_floating_open() || on_panel {
                gesture.set_state(gtk::EventSequenceState::Denied);
                return;
            }
            gesture.set_state(gtk::EventSequenceState::Claimed);
            layout.set_utility_open(false);
        });
        self.utility.split.add_controller(click);
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

    /// The window's content's width from the start of its allocation (the
    /// split's is the last frame's until the split is allocated), and until
    /// it's allocated, the size it's about to show at
    fn window_width(&self) -> i32 {
        let width = self
            .shell_box
            .as_ref()
            .and_then(|shell_box| shell_box.upgrade())
            .map_or(0, |shell_box| shell_box.allocated_width());
        let width = if width > 0 {
            width
        } else {
            self.utility.split.width()
        };
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

    /// Too narrow to sit beside the content without squeezing it, the
    /// utility panel floats over it: as a drawer, or filling the window
    /// when that is narrower still. Closing it then leaves the content the
    /// whole width.
    fn collapse_utility_to_fit(&self) {
        let window_width = self.window_width();
        if window_width <= 0 {
            return;
        }
        let mode = utility_mode(
            window_width,
            sp_to_px(MOBILE_WIDTH_SP),
            sp_to_px(UTILITY_DOCK_MIN_WIDTH_SP),
        );
        // Before collapsing, which applies its width
        self.utility.set_overlay(mode == UtilityMode::Drawer);
        let collapse = mode != UtilityMode::Beside;
        let split = &self.utility.split;
        // A card in a shaded frame as a drawer; filling the window, flush
        if mode == UtilityMode::Drawer {
            split.add_css_class(FLOATING_CLASS);
        } else {
            split.remove_css_class(FLOATING_CLASS);
        }
        if split.is_collapsed() != collapse {
            split.set_collapsed(collapse);
        }
    }

    /// The library, the page and the side panel are rounded cards (see
    /// `.canto-cards` in app.css); what is inside them is cut at the corners.
    /// A window that starts narrow has its side panel floating already: its
    /// pane is then an `.overlay-pane`.
    fn clip_card_panes(builder: &gtk::Builder) {
        for name in ["navigation_split", "utility_split"] {
            let Some(split) = builder.object::<libadwaita::OverlaySplitView>(name) else {
                continue;
            };
            let mut child = split.first_child();
            while let Some(pane) = child {
                if ["sidebar-pane", "content-pane", "overlay-pane"]
                    .iter()
                    .any(|class| pane.has_css_class(class))
                {
                    pane.set_overflow(gtk::Overflow::Hidden);
                }
                child = pane.next_sibling();
            }
        }
    }

    /// In narrow windows home and search sit at the start of the top bar;
    /// with the full sidebar open they move right, to end where it ends.
    fn align_top_center_with_sidebar(builder: &gtk::Builder) {
        let center: gtk::Widget = builder.object("top_center").unwrap();
        let top_bar: gtk::Widget = builder.object("top_bar").unwrap();
        let sidebar: gtk::Widget = builder.object("navigation_panel").unwrap();
        let weak_bar = top_bar.downgrade();
        let weak_sidebar = sidebar.downgrade();
        center.add_tick_callback(move |center, _| {
            let (Some(bar), Some(sidebar)) = (weak_bar.upgrade(), weak_sidebar.upgrade()) else {
                return glib::ControlFlow::Break;
            };
            let current = center.margin_start();
            let mut wanted = 0;
            let beside = sidebar.is_mapped()
                && !sidebar.has_css_class(RAIL_CLASS)
                && center.halign() == gtk::Align::Start;
            if beside {
                if let (Some(side), Some(own)) =
                    (sidebar.compute_bounds(&bar), center.compute_bounds(&bar))
                {
                    let sidebar_end = side.x() + side.width();
                    let start_without_margin = own.x() - current as f32;
                    let gap = 12.0;
                    wanted =
                        (sidebar_end - gap - own.width() - start_without_margin).max(0.0) as i32;
                }
            }
            if (wanted - current).abs() > 1 {
                center.set_margin_start(wanted);
            }
            glib::ControlFlow::Continue
        });
    }

    fn open_navigation_overlay(&self) {
        // One over the content at a time
        if self.utility_floating_open() {
            self.set_utility_open(false);
        }
        self.navigation.set_auto_rail(false);
        self.apply_rail();
        let split = &self.navigation.split;
        // Collapsing refits the layout, which must leave the sidebar
        // collapsed, not put it back on its rail. It also hides the sidebar
        // on its own: only then is the overlay marked open, or that hiding
        // would count as closing it
        self.opening_navigation_overlay.set(true);
        split.set_collapsed(true);
        self.opening_navigation_overlay.set(false);
        self.navigation_overlay.set(true);
        self.navigation.set_overlay(true);
        self.show_navigation(true);
        split.set_show_sidebar(true);
        self.apply();
    }

    /// What decides the library's rail in a window this wide, as things are.
    fn library_fit(&self, window_width: i32) -> LibraryFit {
        LibraryFit {
            window: window_width,
            mobile: window_width <= sp_to_px(MOBILE_WIDTH_SP),
            content_min: self.content_min_width(),
            library_least: self.navigation.full_least_width(),
            // Sliding out, it still takes its room from the content
            beside_panel: (self.utility_beside() || self.utility_sliding_out.get()).then(|| {
                (
                    self.navigation.full_width_beside(window_width),
                    self.utility.shown_width(window_width),
                )
            }),
            docked_content_min: sp_to_px(DOCKED_CONTENT_MIN_WIDTH_SP),
        }
    }

    /// Closed beside the content, the utility panel slides out over it:
    /// until it has, the library keeps the rail it had for the panel
    /// (`library_fit`), so the content only grows. A full library at once
    /// would squeeze the content under the panel still there, dropping
    /// columns and the album header's layout for a moment. Without
    /// animations the panel leaves at once, and so does the rail.
    fn follow_utility_sliding_out(self: &Rc<Self>) {
        let split = &self.utility.split;
        let pane_shows = |split: &libadwaita::OverlaySplitView| {
            split
                .sidebar()
                .and_then(|sidebar| sidebar.parent())
                .is_some_and(|pane| pane.is_child_visible())
        };
        if !split.settings().is_gtk_enable_animations()
            || !pane_shows(split)
            || self.utility_sliding_out.replace(true)
        {
            return;
        }
        let layout = Rc::downgrade(self);
        split.add_tick_callback(move |split, _| {
            let Some(layout) = layout.upgrade() else {
                return glib::ControlFlow::Break;
            };
            // Slid out, opened again, or floating now: laid out as it is
            if pane_shows(split) && !split.shows_sidebar() && !split.is_collapsed() {
                return glib::ControlFlow::Continue;
            }
            layout.utility_sliding_out.set(false);
            layout.fit();
            glib::ControlFlow::Break
        });
    }

    fn collapse_navigation_to_fit(&self) {
        let window_width = self.window_width();
        if window_width <= 0 || self.opening_navigation_overlay.get() {
            return;
        }
        if self.navigation_overlay.get() {
            // Wide enough again for the full sidebar beside the content
            // (maximized from a narrow window): closing it over the content
            // puts it back there
            if self.library_fit(window_width).full_fits() {
                self.navigation.split.set_show_sidebar(false);
            }
            return;
        }
        let fit = self.library_fit(window_width);
        // The utility panel closed or the window grew: a library expanded
        // by hand beside the panel shrinks again the next time it squeezes
        if !fit.squeezed_by_panel() {
            self.expanded_by_hand.set(false);
        }
        // Too narrow for the full sidebar, or for it and a roomy page beside
        // the utility panel: covers only, like big music apps, before hiding
        // it altogether
        let auto = fit.auto_rail(self.navigation.rail_chosen(), self.expanded_by_hand.get());
        self.auto_rail.set(auto);
        self.navigation.set_auto_rail(auto.is_some());
        self.apply_rail();
        let collapse = fit.mobile
            || if self.navigation.rail_chosen() || auto.is_some() {
                !fit.rail_fits()
            } else {
                !fit.full_fits()
            };
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
        } else if !self.navigation.split.is_collapsed()
            && !self.collapsing_navigation.get()
            && self.hid_navigation.replace(false)
        {
            // Floating over the content, the utility panel takes no room
            // from the library: the library hidden for it comes back
            self.show_navigation(true);
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
        // The top bar's menu lines up over the rail's library icon
        if rail {
            w.sidebar.add_css_class(RAIL_CLASS);
            w.top_bar.add_css_class("top-bar--rail");
        } else {
            w.sidebar.remove_css_class(RAIL_CLASS);
            w.top_bar.remove_css_class("top-bar--rail");
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
        // One over the content at a time: the sidebar opened over it (its
        // button or F9 at `MOBILE_WIDTH_SP` or less) closes the floating
        // utility panel
        if self.navigation_over_content() && self.utility_floating_open() {
            self.set_utility_open(false);
        }
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
            self.hide_utility();
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
fn viewports_in(widget: &gtk::Widget) -> Vec<gtk::Viewport> {
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
        // card 448 at 1100, 548 at 1200 (Gus's full screen) and 655 at 1307:
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
