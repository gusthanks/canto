//! Custom popover builder for grouped track context menus with leading icons
//! and an inline sliding PlaylistPicker page.

use std::cell::RefCell;
use std::rc::Rc;

use gettextrs::gettext;
use gtk::pango;
use gtk::prelude::*;

use super::MenuHooks;
use crate::app::components::labels;

/// Attribute name on `gio::MenuItem` storing the 16 px symbolic icon name.
pub const MENU_ICON_ATTR: &str = "canto-icon";

struct HiddenCheck {
    button: gtk::Button,
    short_action: String,
}

struct BuiltSection {
    separator: Option<gtk::Separator>,
    box_: gtk::Box,
    buttons: Vec<gtk::Button>,
}

fn sync_menu_visibility(
    sections: &[BuiltSection],
    hidden_checks: &[HiddenCheck],
    actions: &Option<gio::ActionGroup>,
) {
    for check in hidden_checks {
        let enabled = actions
            .as_ref()
            .is_some_and(|group| group.is_action_enabled(&check.short_action));
        check.button.set_visible(enabled);
    }

    let mut seen_visible = false;
    for section in sections {
        let any_visible = section.buttons.iter().any(|b| b.get_visible());
        section.box_.set_visible(any_visible);
        if let Some(ref sep) = section.separator {
            sep.set_visible(seen_visible && any_visible);
        }
        if any_visible {
            seen_visible = true;
        }
    }
}

/// Builds a `gtk::Popover` from a sectioned `gio::MenuModel`, rendering each
/// item's `canto-icon` attribute, sliding cleanly to the PlaylistPicker submenu,
/// and hiding disabled `hidden-when="action-disabled"` items alongside unused separators.
pub fn build_song_menu_popover(
    owner: &gtk::Widget,
    model: &gio::MenuModel,
    hooks: Option<MenuHooks>,
    actions: Option<gio::ActionGroup>,
) -> gtk::Popover {
    let popover = gtk::Popover::builder()
        .has_arrow(false)
        .css_classes(["menu", "song-menu-popover"])
        .build();

    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::SlideLeftRight)
        .hhomogeneous(false)
        .vhomogeneous(false)
        .build();

    let main_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .build();

    let mut sections = Vec::new();
    let mut hidden_checks = Vec::new();
    let mut has_playlist_submenu = false;

    for i in 0..model.n_items() {
        let Some(section_model) = model.item_link(i, gio::MENU_LINK_SECTION) else {
            continue;
        };
        let sep = if !sections.is_empty() {
            let s = gtk::Separator::builder()
                .orientation(gtk::Orientation::Horizontal)
                .css_classes(["song-menu__separator"])
                .build();
            main_box.append(&s);
            Some(s)
        } else {
            None
        };

        let sec_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .build();
        let mut sec_buttons = Vec::new();

        for j in 0..section_model.n_items() {
            let label = section_model
                .item_attribute_value(j, gio::MENU_ATTRIBUTE_LABEL, None)
                .and_then(|v| v.get::<String>())
                .unwrap_or_default();
            let action_name = section_model
                .item_attribute_value(j, gio::MENU_ATTRIBUTE_ACTION, None)
                .and_then(|v| v.get::<String>());
            let icon_name = section_model
                .item_attribute_value(j, MENU_ICON_ATTR, None)
                .and_then(|v| v.get::<String>());
            let hidden_when = section_model
                .item_attribute_value(j, "hidden-when", None)
                .and_then(|v| v.get::<String>());
            let has_submenu = section_model.item_link(j, gio::MENU_LINK_SUBMENU).is_some();

            let row_content = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(10)
                .build();

            if let Some(icon) = icon_name {
                let img = gtk::Image::builder()
                    .icon_name(&icon)
                    .pixel_size(16)
                    .css_classes(["song-menu__icon"])
                    .build();
                row_content.append(&img);
            }

            let text_label = gtk::Label::builder()
                .label(&label)
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(pango::EllipsizeMode::End)
                .max_width_chars(28)
                .build();
            row_content.append(&text_label);

            if has_submenu {
                let chevron = gtk::Image::builder()
                    .icon_name("pan-end-symbolic")
                    .pixel_size(16)
                    .css_classes(["song-menu__chevron"])
                    .build();
                row_content.append(&chevron);
            }

            let button = gtk::Button::builder()
                .child(&row_content)
                .halign(gtk::Align::Fill)
                .css_classes(["flat", "song-menu__item"])
                .build();

            if has_submenu {
                has_playlist_submenu = true;
                let stack_weak = stack.downgrade();
                button.connect_clicked(move |_| {
                    if let Some(stack) = stack_weak.upgrade() {
                        stack.set_visible_child_name("playlists");
                    }
                });
            } else if let Some(ref action) = action_name {
                let owner_weak = owner.downgrade();
                let pop_weak = popover.downgrade();
                let action = action.clone();
                button.connect_clicked(move |_| {
                    if let Some(owner) = owner_weak.upgrade() {
                        let _ = owner.activate_action(&action, None);
                    }
                    if let Some(pop) = pop_weak.upgrade() {
                        pop.popdown();
                    }
                });
            }

            if hidden_when.as_deref() == Some("action-disabled") {
                if let Some(short) = action_name
                    .as_deref()
                    .and_then(|a| a.strip_prefix("song."))
                    .map(str::to_string)
                {
                    hidden_checks.push(HiddenCheck {
                        button: button.clone(),
                        short_action: short,
                    });
                }
            }

            sec_box.append(&button);
            sec_buttons.push(button);
        }

        main_box.append(&sec_box);
        sections.push(BuiltSection {
            separator: sep,
            box_: sec_box,
            buttons: sec_buttons,
        });
    }

    stack.add_named(&main_box, Some("main"));

    let picker_slot = if has_playlist_submenu {
        let playlists_page = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        let back_btn = gtk::Button::builder()
            .icon_name("pan-start-symbolic")
            .tooltip_text(gettext("Back"))
            .css_classes(["flat", "circular"])
            .build();
        let stack_weak = stack.downgrade();
        back_btn.connect_clicked(move |_| {
            if let Some(stack) = stack_weak.upgrade() {
                stack.set_visible_child_name("main");
            }
        });
        let title = gtk::Label::builder()
            .label(&*labels::ADD_TO_PLAYLIST)
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["heading"])
            .build();
        header.append(&back_btn);
        header.append(&title);
        playlists_page.append(&header);

        let slot = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        playlists_page.append(&slot);
        stack.add_named(&playlists_page, Some("playlists"));
        Some(slot)
    } else {
        None
    };

    let sections = Rc::new(sections);
    let hidden_checks = Rc::new(hidden_checks);
    sync_menu_visibility(&sections, &hidden_checks, &actions);

    let shown_picker: Rc<RefCell<Option<gtk::Widget>>> = Rc::new(RefCell::new(None));
    let stack_weak = stack.downgrade();
    let slot_weak = picker_slot.as_ref().map(glib::object::ObjectExt::downgrade);
    let sections_ref = Rc::clone(&sections);
    let checks_ref = Rc::clone(&hidden_checks);
    popover.connect_show(move |_| {
        if let Some(stack) = stack_weak.upgrade() {
            stack.set_transition_type(gtk::StackTransitionType::None);
            stack.set_visible_child_name("main");
            stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
        }
        if let Some(ref hooks) = hooks {
            (hooks.opening)();
            if let Some(slot) = slot_weak.as_ref().and_then(glib::WeakRef::upgrade) {
                if let Some(prev) = shown_picker.borrow_mut().take() {
                    slot.remove(&prev);
                }
                if let Some(child) = (hooks.picker)() {
                    slot.append(&child);
                    *shown_picker.borrow_mut() = Some(child);
                }
            }
        }
        sync_menu_visibility(&sections_ref, &checks_ref, &actions);
    });

    // Scrolls when the window is shorter than the menu; `fit_popover` sets
    // its ceiling (`popup_fit.rs`)
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .propagate_natural_width(true)
        .child(&stack)
        .build();
    popover.set_child(Some(&scroller));
    popover
}
