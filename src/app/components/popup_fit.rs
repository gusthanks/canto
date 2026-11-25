//! Keeps a menu inside the window.
//!
//! A menu opened at a click or under a button goes below it when it fits,
//! above when only that fits, and otherwise slides up until its bottom edge
//! meets the window's. A menu taller than the window scrolls. The compositor
//! is not trusted to do this: some (Hyprland) neither slide nor resize a popup,
//! and a tall menu opened mid-screen ran off the bottom edge (Q41, 24/11).

use gtk::prelude::*;

/// Room kept between a menu and the window's edges, in logical pixels.
const EDGE_MARGIN: i32 = 8;

/// Where a menu goes, in window coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuPlacement {
    pub position: gtk::PositionType,
    /// The rectangle the menu points to: (x, y, width, height)
    pub anchor: (i32, i32, i32, i32),
    /// Set when the menu is taller than the window and has to scroll
    pub max_height: Option<i32>,
    /// Height free on the side the menu opens to: a menu that grows once
    /// open (a submenu page) scrolls past it instead of leaving the window
    pub room: i32,
}

/// Places a `menu` (width, height) by an `anchor` (x, y, width, height) in a
/// `window` (width, height). Pure, for the tests.
pub fn place_menu(
    window: (i32, i32),
    anchor: (i32, i32, i32, i32),
    menu: (i32, i32),
    margin: i32,
) -> MenuPlacement {
    let (win_w, win_h) = window;
    let (ax, ay, aw, ah) = anchor;
    let (menu_w, mut menu_h) = menu;

    let room = (win_h - 2 * margin).max(1);
    let max_height = (menu_h > room).then_some(room);
    menu_h = menu_h.min(room);

    // The menu is centred on the anchor: keep that centre far enough from
    // the left and right edges for the whole menu to show
    let centre = ax + aw / 2;
    let half = menu_w / 2;
    let centre = if menu_w + 2 * margin >= win_w {
        win_w / 2
    } else {
        centre.clamp(margin + half, win_w - margin - half)
    };

    let below = win_h - margin - (ay + ah);
    let above = ay - margin;
    let (position, y, h) = if menu_h <= below {
        (gtk::PositionType::Bottom, ay, ah)
    } else if menu_h <= above {
        (gtk::PositionType::Top, ay, ah)
    } else {
        // Neither side has room: slide up so the bottom edge fits
        (gtk::PositionType::Bottom, win_h - margin - menu_h - 1, 1)
    };

    let h = h.max(1);
    let room = match position {
        gtk::PositionType::Top => y - margin,
        _ => win_h - margin - (y + h),
    };
    MenuPlacement {
        position,
        anchor: (centre, y, 1, h),
        max_height,
        room: room.max(1),
    }
}

/// Points `popover` at `anchor` (in the popover parent's coordinates) so it
/// fits in the window. Call after the popover has a parent and its content,
/// before `popup()`.
pub fn fit_popover(popover: &gtk::Popover, anchor: &gtk::gdk::Rectangle) {
    let Some(parent) = popover.parent() else {
        return;
    };
    let Some(window) = parent.root().map(|root| root.upcast::<gtk::Widget>()) else {
        return;
    };
    let (win_w, win_h) = (window.width(), window.height());
    if win_w <= 0 || win_h <= 0 {
        return;
    }
    let Some(origin) = parent.compute_point(
        &window,
        &gtk::graphene::Point::new(anchor.x() as f32, anchor.y() as f32),
    ) else {
        return;
    };

    // Unbounded first, to read the menu's full height
    let scroller = popover.child().and_downcast::<gtk::ScrolledWindow>();
    if let Some(scroller) = &scroller {
        scroller.set_max_content_height(-1);
    }
    let (_, menu_h, _, _) = popover.measure(gtk::Orientation::Vertical, -1);
    let (_, menu_w, _, _) = popover.measure(gtk::Orientation::Horizontal, -1);

    let placement = place_menu(
        (win_w, win_h),
        (
            origin.x() as i32,
            origin.y() as i32,
            anchor.width(),
            anchor.height(),
        ),
        (menu_w, menu_h),
        EDGE_MARGIN,
    );

    if let Some(scroller) = &scroller {
        // The popover's frame and shadow sit around the scroller
        let frame = (menu_h - scroller.measure(gtk::Orientation::Vertical, -1).1).max(0);
        scroller.set_max_content_height((placement.room - frame).max(1));
    }

    let (x, y, w, h) = placement.anchor;
    let Some(local) = window.compute_point(&parent, &gtk::graphene::Point::new(x as f32, y as f32))
    else {
        return;
    };
    popover.set_position(placement.position);
    popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
        local.x() as i32,
        local.y() as i32,
        w,
        h,
    )));
}

/// Opens `popover` at the point (x, y) of its parent, fitted to the window.
pub fn popup_at(popover: &gtk::Popover, x: f64, y: f64) {
    fit_popover(popover, &gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1));
    popover.popup();
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: (i32, i32) = (1200, 675);

    #[test]
    fn a_menu_with_room_below_opens_below_the_click() {
        let p = place_menu(WINDOW, (400, 100, 1, 1), (300, 400), 8);
        assert_eq!(p.position, gtk::PositionType::Bottom);
        assert_eq!(p.anchor, (400, 100, 1, 1));
        assert_eq!(p.max_height, None);
        assert_eq!(p.room, 675 - 8 - 101);
    }

    #[test]
    fn a_menu_near_the_bottom_opens_above_the_click() {
        let p = place_menu(WINDOW, (400, 600, 1, 1), (300, 400), 8);
        assert_eq!(p.position, gtk::PositionType::Top);
        assert_eq!(p.anchor.1, 600);
        assert_eq!(p.room, 600 - 8);
    }

    #[test]
    fn a_menu_without_room_on_either_side_slides_up_to_fit() {
        // The print of 24/11: a 425 px menu clicked at mid-height
        let p = place_menu(WINDOW, (400, 344, 1, 1), (300, 425), 8);
        assert_eq!(p.position, gtk::PositionType::Bottom);
        let top = p.anchor.1 + p.anchor.3;
        assert!(top >= 8, "top {}", top);
        assert!(top + 425 <= 675 - 8 + 1, "bottom {}", top + 425);
    }

    #[test]
    fn a_menu_taller_than_the_window_scrolls() {
        let p = place_menu(WINDOW, (400, 300, 1, 1), (300, 900), 8);
        assert_eq!(p.max_height, Some(675 - 16));
        assert!(p.anchor.1 + p.anchor.3 >= 8);
        assert!(p.room <= 675 - 16);
    }

    #[test]
    fn a_menu_by_the_right_edge_moves_left_to_show_whole() {
        let p = place_menu(WINDOW, (1190, 100, 1, 1), (300, 200), 8);
        assert_eq!(p.anchor.0, 1200 - 8 - 150);
        let p = place_menu(WINDOW, (5, 100, 1, 1), (300, 200), 8);
        assert_eq!(p.anchor.0, 8 + 150);
    }

    #[test]
    fn a_button_anchor_keeps_its_height_when_the_menu_fits() {
        let p = place_menu(WINDOW, (380, 100, 40, 30), (300, 200), 8);
        assert_eq!(p.position, gtk::PositionType::Bottom);
        assert_eq!(p.anchor, (400, 100, 1, 30));
    }
}
