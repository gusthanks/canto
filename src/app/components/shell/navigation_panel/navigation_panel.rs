use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::{
    context_menu::build_context_menu,
    navigation_panel_row::{NavigationPanelRow, COVER_PX},
    NavigationPanelDestination, NavigationPanelItem, NavigationPanelModel, LIBRARY_SECTION,
    PINNED_SECTION, SAVED_PLAYLISTS_SECTION,
};
use crate::app::components::PlaylistDialog;
use crate::app::state::{HomePage, Route, Transition};
use crate::app::{AppEvent, BrowserEvent, Component, EventListener};

pub struct NavigationPanel {
    listbox: gtk::ListBox,
    list_store: gio::ListStore,
    model: Rc<NavigationPanelModel>,
    _context_menu: gtk::PopoverMenu,
    num_fixed_entries: u32,
}

impl NavigationPanel {
    pub fn new(listbox: gtk::ListBox, model: Rc<NavigationPanelModel>) -> Self {
        let list_store = gio::ListStore::new::<NavigationPanelItem>();

        list_store.append(&NavigationPanelItem::from_destination(
            NavigationPanelDestination::SavedArtists,
        ));
        list_store.append(&NavigationPanelItem::from_destination(
            NavigationPanelDestination::Library,
        ));
        list_store.append(&NavigationPanelItem::from_destination(
            NavigationPanelDestination::SavedPlaylists,
        ));
        list_store.append(&NavigationPanelItem::from_destination(
            NavigationPanelDestination::SavedTracks,
        ));
        list_store.append(&NavigationPanelItem::playlists_section());

        let rail = Rc::new(Cell::new(false));
        let weak_model = Rc::downgrade(&model);
        listbox.bind_model(Some(&list_store), clone!(
            #[strong]
            rail,
            move |obj| {
                let item = obj.downcast_ref::<NavigationPanelItem>().unwrap();
                let row = if item.navigatable() {
                    Self::make_navigatable(item, weak_model.upgrade().as_deref())
                } else {
                    match item.id().as_str() {
                        SAVED_PLAYLISTS_SECTION => {
                            Self::make_section_label(item, weak_model.upgrade().as_ref())
                        }
                        PINNED_SECTION | LIBRARY_SECTION => Self::make_section_label(item, None),
                        _ => unimplemented!(),
                    }
                };
                Self::set_row_rail(&row, rail.get());
                row
            }
        ));

        listbox.connect_row_activated(clone!(
            #[weak]
            model,
            move |listbox, row| {
                if let Some(row) = row.downcast_ref::<NavigationPanelRow>() {
                    if let Some(dest) = row.item().destination() {
                        let split = listbox
                            .ancestor(libadwaita::OverlaySplitView::static_type())
                            .and_downcast::<libadwaita::OverlaySplitView>();
                        if let Some(split) = split.filter(|s| s.is_collapsed()) {
                            split.set_show_sidebar(false);
                        }
                        model.navigate(dest);
                    }
                }
            }
        ));

        let context_menu = gtk::PopoverMenu::from_model(None::<&gio::MenuModel>);
        // Parent the popover to the Box above the ScrolledWindow to avoid
        // inheriting any scroll constraints that would add a scrollbar.
        let navigation_panel_box = listbox
            .ancestor(gtk::ScrolledWindow::static_type())
            .and_then(|sw| sw.parent())
            .and_downcast::<gtk::Box>()
            .unwrap();
        context_menu.set_parent(&navigation_panel_box);

        // The layout marks the sidebar when it collapses to covers and icons
        if let Some(sidebar) = navigation_panel_box.parent() {
            sidebar.connect_css_classes_notify(clone!(
                #[weak]
                listbox,
                #[strong]
                rail,
                move |sidebar| {
                    let collapsed = sidebar.has_css_class("navigation-panel--rail");
                    if rail.replace(collapsed) != collapsed {
                        let mut child = listbox.first_child();
                        while let Some(row) = child {
                            Self::set_row_rail(&row, collapsed);
                            child = row.next_sibling();
                        }
                    }
                }
            ));
        }
        context_menu.set_has_arrow(false);

        let context_row: Rc<RefCell<Option<NavigationPanelRow>>> = Default::default();

        context_menu.connect_closed(clone!(
            #[strong]
            context_row,
            move |_| {
                if let Some(row) = context_row.borrow_mut().take() {
                    row.unset_state_flags(gtk::StateFlags::SELECTED);
                }
            }
        ));

        let show_context_menu = clone!(
            #[weak]
            listbox,
            #[weak]
            model,
            #[weak]
            context_menu,
            #[strong]
            context_row,
            move |x: f64, y: f64| {
                let Some(row) = listbox.row_at_y(y as i32) else {
                    return;
                };
                let Some(row) = row.downcast_ref::<NavigationPanelRow>() else {
                    return;
                };
                let Some((prefix, actions, menu)) = row
                    .item()
                    .destination()
                    .and_then(|destination| build_context_menu(&destination, &model))
                else {
                    return;
                };

                row.set_state_flags(gtk::StateFlags::SELECTED, false);
                context_row.replace(Some(row.clone()));

                context_menu.insert_action_group(prefix, Some(&actions));
                context_menu.set_menu_model(Some(&menu));

                // Translate coordinates from listbox space to the popover parent (navigation panel Box) space
                let popover_parent = context_menu.parent().unwrap();
                let translated = listbox
                    .compute_point(
                        &popover_parent,
                        &gtk::graphene::Point::new(x as f32, y as f32),
                    )
                    .unwrap_or_else(|| gtk::graphene::Point::new(x as f32, y as f32));
                let rect = gdk::Rectangle::new(translated.x() as i32, translated.y() as i32, 1, 1);
                context_menu.set_pointing_to(Some(&rect));
                context_menu.popup();
            }
        );

        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        right_click.connect_pressed(clone!(
            #[strong]
            show_context_menu,
            move |_, _, x, y| {
                show_context_menu(x, y);
            }
        ));
        listbox.add_controller(right_click);

        let long_press = gtk::GestureLongPress::new();
        long_press.set_touch_only(false);
        long_press.connect_pressed(clone!(
            #[strong]
            show_context_menu,
            move |_, x, y| {
                show_context_menu(x, y);
            }
        ));
        listbox.add_controller(long_press);

        let scrolled_window = listbox
            .ancestor(gtk::ScrolledWindow::static_type())
            .and_downcast::<gtk::ScrolledWindow>()
            .unwrap();
        scrolled_window.connect_edge_reached(clone!(
            #[weak]
            model,
            move |_, pos| {
                if pos == gtk::PositionType::Bottom {
                    model.load_more_playlists();
                }
            }
        ));

        let num_fixed_entries = list_store.n_items();

        model.apply_navigation_panel_items(&list_store, num_fixed_entries);

        let panel = Self {
            listbox,
            list_store,
            model,
            _context_menu: context_menu,
            num_fixed_entries,
        };
        panel
    }

    fn make_navigatable(item: &NavigationPanelItem, model: Option<&NavigationPanelModel>) -> gtk::Widget {
        let row = NavigationPanelRow::new(item.clone());
        row.set_selectable(false);
        if let (Some(model), Some((image, url))) = (model, row.cover_target()) {
            let scale = image.scale_factor().max(2);
            model.load_cover(&image, &url, COVER_PX * scale);
        }
        row.upcast()
    }

    /// Covers and icons only while the sidebar is collapsed.
    fn set_row_rail(row: &gtk::Widget, rail: bool) {
        if let Some(row) = row.downcast_ref::<NavigationPanelRow>() {
            row.set_rail(rail);
        } else if row.has_css_class("navigation-section") {
            row.set_visible(!rail);
        }
    }

    fn make_section_label(
        item: &NavigationPanelItem,
        create_playlist: Option<&Rc<NavigationPanelModel>>,
    ) -> gtk::Widget {
        let label = gtk::Label::builder()
            .label(item.title())
            .xalign(0.0)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        content.append(&label);

        if let Some(model) = create_playlist {
            let button = gtk::Button::builder()
                .icon_name("list-add-symbolic")
                .tooltip_text(gettext("New Playlist"))
                .valign(gtk::Align::Center)
                .build();
            button.add_css_class("flat");
            button.add_css_class("dim-label");
            button.connect_clicked(clone!(
                #[weak]
                model,
                move |button| {
                    let dialog = PlaylistDialog::new();
                    dialog.connect_save(clone!(
                        #[weak]
                        model,
                        #[upgrade_or_else]
                        || tokio::sync::oneshot::channel().1,
                        move |playlist| model.create_new_playlist(playlist)
                    ));
                    dialog.present(Some(button));
                }
            ));
            content.append(&button);
        }

        let row = gtk::ListBoxRow::builder()
            .activatable(false)
            .selectable(false)
            .focusable(false)
            .sensitive(create_playlist.is_some())
            .child(&content)
            .build();
        row.add_css_class("navigation-section");
        row.upcast()
    }

    fn select_home_page(&self, page: HomePage) {
        let index = (0..self.list_store.n_items()).find(|&i| {
            self.list_store
                .item(i)
                .and_downcast::<NavigationPanelItem>()
                .and_then(|item| item.destination())
                .and_then(|dest| dest.home_page())
                == Some(page)
        });
        let row = index.and_then(|i| self.listbox.row_at_index(i as i32));
        if let Some(row) = row {
            self.listbox.select_row(Some(&row));
        }
    }

    fn update_playlists_in_navigation_panel(&self) {
        self.model
            .apply_navigation_panel_items(&self.list_store, self.num_fixed_entries);
    }
}

impl Component for NavigationPanel {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.listbox.upcast_ref()
    }
}

impl EventListener for NavigationPanel {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::BrowserEvent(
                BrowserEvent::SavedPlaylistsUpdated | BrowserEvent::PinnedPlaylistsUpdated,
            ) => self.update_playlists_in_navigation_panel(),
            AppEvent::BrowserEvent(BrowserEvent::NavigationChanged(Transition {
                to: Route::Home(page),
                ..
            })) => self.select_home_page(*page),
            _ => {}
        }
    }
}

/// A sidebar with a few covered playlists, to look at its rendering.
#[cfg(test)]
pub(crate) fn sample_for_tests(cover: &gtk::gdk::Paintable, rail: bool) -> gtk::Widget {
    use crate::app::models::PlaylistSummary;

    let listbox = gtk::ListBox::new();
    listbox.add_css_class("navigation-sidebar");
    let mut rows = vec![
        NavigationPanel::make_navigatable(
            &NavigationPanelItem::from_destination(NavigationPanelDestination::SavedArtists),
            None,
        ),
        NavigationPanel::make_navigatable(
            &NavigationPanelItem::from_destination(NavigationPanelDestination::SavedTracks),
            None,
        ),
        NavigationPanel::make_section_label(&NavigationPanelItem::playlists_section(), None),
    ];
    for (i, title) in ["Rap Nacional", "Treino", "Madrugada"].iter().enumerate() {
        let item = NavigationPanelItem::from_destination(NavigationPanelDestination::Playlist(
            PlaylistSummary {
                id: i.to_string(),
                title: title.to_string(),
            },
        ));
        item.set_image("https://example.com/cover.jpg");
        let row = NavigationPanel::make_navigatable(&item, None);
        if let Some((image, _)) = row
            .downcast_ref::<NavigationPanelRow>()
            .and_then(|r| r.cover_target())
        {
            image.set_paintable(Some(cover));
        }
        rows.push(row);
    }
    for row in &rows {
        NavigationPanel::set_row_rail(row, rail);
        listbox.append(row);
    }
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    if rail {
        sidebar.add_css_class("navigation-panel--rail");
    }
    sidebar.append(&listbox);
    sidebar.upcast()
}
