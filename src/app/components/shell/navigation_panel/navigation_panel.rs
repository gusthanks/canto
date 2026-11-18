use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::{
    context_menu::build_context_menu,
    navigation_panel_row::{NavigationPanelRow, COVER_PX},
    LibraryFilter, LibrarySort, LibraryView, NavigationPanelItem, NavigationPanelModel,
    LIBRARY_SECTION, PINNED_SECTION, SAVED_PLAYLISTS_SECTION,
};
use crate::app::components::PlaylistDialog;
use crate::app::state::{HomePage, PlaybackEvent, Route, Transition};
use crate::app::{AppEvent, BrowserEvent, Component, EventListener};

/// Side of a cover in the grid view: three side by side in the usual width.
const GRID_COVER_PX: i32 = 72;

pub struct NavigationPanel {
    listbox: gtk::ListBox,
    /// The same items as covers ("View As" > Grid)
    grid: gtk::FlowBox,
    grid_items: gtk::FilterListModel,
    list_store: gio::ListStore,
    model: Rc<NavigationPanelModel>,
    _context_menu: gtk::PopoverMenu,
    /// A refill is already queued for the next idle moment.
    refill_queued: Rc<Cell<bool>>,
}

impl NavigationPanel {
    pub fn new(listbox: gtk::ListBox, model: Rc<NavigationPanelModel>) -> Self {
        let list_store = gio::ListStore::new::<NavigationPanelItem>();

        let rail = Rc::new(Cell::new(false));
        let weak_model = Rc::downgrade(&model);
        listbox.bind_model(
            Some(&list_store),
            clone!(
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
                            PINNED_SECTION | LIBRARY_SECTION => {
                                Self::make_section_label(item, None)
                            }
                            _ => unimplemented!(),
                        }
                    };
                    Self::set_row_rail(&row, rail.get());
                    row
                }
            ),
        );

        listbox.connect_row_activated(clone!(
            #[weak]
            model,
            move |listbox, row| {
                if let Some(row) = row.downcast_ref::<NavigationPanelRow>() {
                    if let Some(dest) = row.item().destination() {
                        Self::close_overlay_sidebar(listbox.upcast_ref());
                        model.navigate(dest);
                    }
                }
            }
        ));

        // The grid lives next to the list, in the same scrolled area
        let (grid, grid_items) = Self::make_grid(&list_store, &model);
        if let Some(viewport) = listbox.parent().and_downcast::<gtk::Viewport>() {
            viewport.set_child(None::<&gtk::Widget>);
            viewport.set_child(Some(&Self::hold_list_and_grid(&listbox, &grid)));
        }
        grid.connect_child_activated(clone!(
            #[weak]
            model,
            #[weak]
            grid_items,
            move |grid, child| {
                let dest = Self::grid_item(&grid_items, child).and_then(|item| item.destination());
                if let Some(dest) = dest {
                    Self::close_overlay_sidebar(grid.upcast_ref());
                    model.navigate(dest);
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

        // The layout marks the sidebar when it collapses to covers and icons.
        // It may already have, before this panel existed (a narrow window or
        // a remembered rail at start): rows then start as covers only, or
        // their squeezed titles show as "…"
        if let Some(sidebar) = navigation_panel_box.parent() {
            rail.set(sidebar.has_css_class("navigation-panel--rail"));
            Self::set_scrollbar_rail(&listbox, rail.get());
            sidebar.connect_css_classes_notify(clone!(
                #[weak]
                listbox,
                #[weak]
                grid,
                #[weak]
                model,
                #[strong]
                rail,
                move |sidebar| {
                    let collapsed = sidebar.has_css_class("navigation-panel--rail");
                    if rail.replace(collapsed) != collapsed {
                        Self::set_scrollbar_rail(&listbox, collapsed);
                        Self::apply_view(&listbox, &grid, model.view(), collapsed);
                        let mut child = listbox.first_child();
                        while let Some(row) = child {
                            Self::set_row_rail(&row, collapsed);
                            child = row.next_sibling();
                        }
                    }
                }
            ));
        }
        Self::apply_view(&listbox, &grid, model.view(), rail.get());
        context_menu.set_has_arrow(false);

        let context_row: Rc<RefCell<Option<gtk::Widget>>> = Default::default();

        context_menu.connect_closed(clone!(
            #[strong]
            context_row,
            move |_| {
                if let Some(row) = context_row.borrow_mut().take() {
                    row.unset_state_flags(gtk::StateFlags::SELECTED);
                }
            }
        ));

        // `area` is the list or the grid, where (x, y) is; `row` is lit
        // while the menu is open
        let open_menu = clone!(
            #[weak]
            model,
            #[weak]
            context_menu,
            #[strong]
            context_row,
            move |area: &gtk::Widget,
                  row: &gtk::Widget,
                  item: &NavigationPanelItem,
                  x: f64,
                  y: f64| {
                let Some((prefix, actions, menu)) = item
                    .destination()
                    .and_then(|destination| build_context_menu(&destination, &model))
                else {
                    return;
                };

                row.set_state_flags(gtk::StateFlags::SELECTED, false);
                context_row.replace(Some(row.clone()));

                context_menu.insert_action_group(prefix, Some(&actions));
                context_menu.set_menu_model(Some(&menu));

                // Translate coordinates from the list's (or grid's) space to the popover parent (navigation panel Box) space
                let popover_parent = context_menu.parent().unwrap();
                let translated = area
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

        let show_context_menu = clone!(
            #[weak]
            listbox,
            #[strong]
            open_menu,
            move |x: f64, y: f64| {
                let Some(row) = listbox
                    .row_at_y(y as i32)
                    .and_downcast::<NavigationPanelRow>()
                else {
                    return;
                };
                open_menu(listbox.upcast_ref(), row.upcast_ref(), &row.item(), x, y);
            }
        );

        let show_grid_menu = clone!(
            #[weak]
            grid,
            #[weak]
            grid_items,
            #[strong]
            open_menu,
            move |x: f64, y: f64| {
                let Some(child) = grid.child_at_pos(x as i32, y as i32) else {
                    return;
                };
                let Some(item) = Self::grid_item(&grid_items, &child) else {
                    return;
                };
                open_menu(grid.upcast_ref(), child.upcast_ref(), &item, x, y);
            }
        );

        // Double click on a row plays it from the first track
        let double_click = gtk::GestureClick::new();
        double_click.set_button(1);
        double_click.connect_pressed(clone!(
            #[weak]
            model,
            #[weak]
            listbox,
            move |_, n_press, _, y| {
                if n_press != 2 {
                    return;
                }
                let destination = listbox
                    .row_at_y(y as i32)
                    .and_downcast::<NavigationPanelRow>()
                    .and_then(|row| row.item().destination());
                if let Some(dest) = destination {
                    model.play_destination(dest);
                }
            }
        ));
        listbox.add_controller(double_click);

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

        // The same gestures on the grid
        let grid_double_click = gtk::GestureClick::new();
        grid_double_click.set_button(1);
        grid_double_click.connect_pressed(clone!(
            #[weak]
            model,
            #[weak]
            grid,
            #[weak]
            grid_items,
            move |_, n_press, x, y| {
                if n_press != 2 {
                    return;
                }
                let destination = grid
                    .child_at_pos(x as i32, y as i32)
                    .and_then(|child| Self::grid_item(&grid_items, &child))
                    .and_then(|item| item.destination());
                if let Some(dest) = destination {
                    model.play_destination(dest);
                }
            }
        ));
        grid.add_controller(grid_double_click);

        let grid_right_click = gtk::GestureClick::new();
        grid_right_click.set_button(3);
        grid_right_click.connect_pressed(clone!(
            #[strong]
            show_grid_menu,
            move |_, _, x, y| show_grid_menu(x, y)
        ));
        grid.add_controller(grid_right_click);

        let grid_long_press = gtk::GestureLongPress::new();
        grid_long_press.set_touch_only(false);
        grid_long_press.connect_pressed(clone!(
            #[strong]
            show_grid_menu,
            move |_, x, y| show_grid_menu(x, y)
        ));
        grid.add_controller(grid_long_press);

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

        model.apply_navigation_panel_items(&list_store);
        Self::add_library_controls(
            &navigation_panel_box,
            &model,
            &list_store,
            (&listbox, &grid, &rail),
        );
        model.refresh_recents(&list_store);

        Self {
            listbox,
            grid,
            grid_items,
            list_store,
            model,
            _context_menu: context_menu,
            refill_queued: Default::default(),
        }
    }

    /// A "+" that opens the new playlist dialog.
    fn new_playlist_button(model: &Rc<NavigationPanelModel>) -> gtk::Button {
        let add = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text(gettext("New Playlist"))
            .valign(gtk::Align::Center)
            .focus_on_click(false)
            .build();
        add.connect_clicked(clone!(
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
        add
    }

    /// The "+" (new playlist) in the sidebar's header, and the filter pills
    /// (Playlists, Albums, Artists) above the list.
    fn add_library_controls(
        panel_box: &gtk::Box,
        model: &Rc<NavigationPanelModel>,
        list_store: &gio::ListStore,
        views: (&gtk::ListBox, &gtk::FlowBox, &Rc<Cell<bool>>),
    ) {
        let mut child = panel_box.first_child();
        let mut header = None;
        let mut rail_header = None;
        let mut list = None;
        while let Some(widget) = child {
            if widget.has_css_class("rail-header") {
                rail_header = widget.downcast_ref::<gtk::Box>().cloned();
            }
            if let Some(h) = widget.downcast_ref::<libadwaita::HeaderBar>() {
                header = Some(h.clone());
            }
            if widget.is::<gtk::ScrolledWindow>() {
                list = Some(widget.clone());
            }
            child = widget.next_sibling();
        }

        if let Some(header) = header {
            header.pack_end(&Self::new_playlist_button(model));
        }
        if let Some(rail_header) = rail_header {
            let add = Self::new_playlist_button(model);
            add.add_css_class("circular");
            add.add_css_class("rail-header__add");
            add.set_halign(gtk::Align::Center);
            rail_header.append(&add);
        }

        let filters = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        filters.add_css_class("library-filters");
        let buttons: Rc<RefCell<Vec<(gtk::ToggleButton, LibraryFilter)>>> = Default::default();
        for (label, filter) in [
            (gettext("Playlists"), LibraryFilter::Playlists),
            (gettext("Albums"), LibraryFilter::Albums),
            (gettext("Artists"), LibraryFilter::Artists),
        ] {
            let button = gtk::ToggleButton::with_label(&label);
            button.add_css_class("pill");
            button.add_css_class("library-filter");
            button.set_focus_on_click(false);
            button.connect_clicked(clone!(
                #[weak]
                model,
                #[weak]
                list_store,
                #[strong]
                buttons,
                move |button| {
                    let chosen = if button.is_active() {
                        filter
                    } else {
                        LibraryFilter::All
                    };
                    for (other, other_filter) in buttons.borrow().iter() {
                        if *other_filter != filter && other.is_active() {
                            other.set_active(false);
                        }
                    }
                    if model.filter() != chosen {
                        model.set_filter(chosen);
                        model.apply_navigation_panel_items(&list_store);
                    }
                }
            ));
            filters.append(&button);
            buttons.borrow_mut().push((button, filter));
        }
        let tools = Self::library_tools(model, list_store, views);
        match list {
            Some(list) => {
                panel_box.insert_child_after(&filters, list.prev_sibling().as_ref());
                panel_box.insert_child_after(&tools, Some(&filters));
            }
            None => {
                panel_box.append(&filters);
                panel_box.append(&tools);
            }
        }
        // No room for them in the collapsed sidebar
        if let Some(sidebar) = panel_box.parent() {
            let sync = clone!(
                #[weak]
                filters,
                #[weak]
                tools,
                move |sidebar: &gtk::Widget| {
                    let full = !sidebar.has_css_class("navigation-panel--rail");
                    filters.set_visible(full);
                    tools.set_visible(full);
                }
            );
            sync(&sidebar);
            sidebar.connect_css_classes_notify(sync);
        }
    }

    /// A search in the library on the left, the sort order and the view
    /// (list or grid) on the right.
    fn library_tools(
        model: &Rc<NavigationPanelModel>,
        list_store: &gio::ListStore,
        (listbox, grid, rail): (&gtk::ListBox, &gtk::FlowBox, &Rc<Cell<bool>>),
    ) -> gtk::Box {
        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        tools.add_css_class("library-tools");

        // translators: Tooltip and placeholder of the search inside Your Library.
        let search_label = gettext("Search in Your Library");
        let search_toggle = gtk::ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text(&search_label)
            .valign(gtk::Align::Center)
            .build();
        search_toggle.add_css_class("flat");
        search_toggle.add_css_class("circular");
        let entry = gtk::SearchEntry::new();
        entry.set_placeholder_text(Some(&search_label));
        entry.set_hexpand(true);
        entry.set_visible(false);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);

        search_toggle.connect_toggled(clone!(
            #[weak]
            entry,
            #[weak]
            spacer,
            move |toggle| {
                let on = toggle.is_active();
                entry.set_visible(on);
                spacer.set_visible(!on);
                if on {
                    entry.grab_focus();
                } else {
                    entry.set_text("");
                }
            }
        ));
        entry.connect_search_changed(clone!(
            #[weak]
            model,
            #[weak]
            list_store,
            move |entry| {
                model.set_query(&entry.text());
                model.apply_navigation_panel_items(&list_store);
            }
        ));
        entry.connect_stop_search(clone!(
            #[weak]
            search_toggle,
            move |_| search_toggle.set_active(false)
        ));

        let sort_label = gtk::Label::new(Some(&model.sort().label()));
        let sort_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        sort_content.append(&sort_label);
        // Shows the chosen view, as in big music apps
        let view_icon = gtk::Image::from_icon_name(model.view().icon());
        sort_content.append(&view_icon);
        let sort_button = gtk::MenuButton::builder()
            .child(&sort_content)
            .valign(gtk::Align::Center)
            // translators: Tooltip of the button that picks how Your Library is ordered.
            .tooltip_text(gettext("Sort By"))
            .build();
        sort_button.add_css_class("flat");
        sort_button.add_css_class("library-sort");

        let options = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let heading = gtk::Label::new(Some(&gettext("Sort By")));
        heading.set_xalign(0.0);
        heading.add_css_class("dim-label");
        heading.add_css_class("caption-heading");
        heading.set_margin_start(8);
        heading.set_margin_bottom(4);
        options.append(&heading);
        let checks: Rc<RefCell<Vec<(gtk::Image, LibrarySort)>>> = Default::default();
        let popover = gtk::Popover::new();
        for sort in LibrarySort::ALL {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            let label = gtk::Label::new(Some(&sort.label()));
            label.set_xalign(0.0);
            label.set_hexpand(true);
            let check = gtk::Image::from_icon_name("object-select-symbolic");
            check.set_opacity(if model.sort() == sort { 1.0 } else { 0.0 });
            row.append(&label);
            row.append(&check);
            checks.borrow_mut().push((check, sort));
            let button = gtk::Button::builder().child(&row).build();
            button.add_css_class("flat");
            button.connect_clicked(clone!(
                #[weak]
                model,
                #[weak]
                list_store,
                #[weak]
                sort_label,
                #[weak]
                popover,
                #[strong]
                checks,
                move |_| {
                    model.set_sort(sort);
                    sort_label.set_text(&sort.label());
                    for (check, which) in checks.borrow().iter() {
                        check.set_opacity(if *which == sort { 1.0 } else { 0.0 });
                    }
                    model.apply_navigation_panel_items(&list_store);
                    popover.popdown();
                }
            ));
            options.append(&button);
        }

        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        separator.set_margin_top(6);
        separator.set_margin_bottom(6);
        options.append(&separator);
        // translators: Heading in Your Library's sort menu, above the list and grid buttons.
        let view_heading = gtk::Label::new(Some(&gettext("View As")));
        view_heading.set_xalign(0.0);
        view_heading.add_css_class("dim-label");
        view_heading.add_css_class("caption-heading");
        view_heading.set_margin_start(8);
        view_heading.set_margin_bottom(4);
        options.append(&view_heading);
        let views = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        views.add_css_class("linked");
        views.set_halign(gtk::Align::Start);
        views.set_margin_start(8);
        views.set_margin_bottom(4);
        let mut first: Option<gtk::ToggleButton> = None;
        for view in LibraryView::ALL {
            let button = gtk::ToggleButton::builder()
                .icon_name(view.icon())
                .tooltip_text(view.label())
                .active(model.view() == view)
                .build();
            button.update_property(&[gtk::accessible::Property::Label(&view.label())]);
            match &first {
                Some(first) => button.set_group(Some(first)),
                None => first = Some(button.clone()),
            }
            button.connect_toggled(clone!(
                #[weak]
                model,
                #[weak]
                view_icon,
                #[weak]
                listbox,
                #[weak]
                grid,
                #[strong]
                rail,
                #[weak]
                popover,
                move |button| {
                    if !button.is_active() || model.view() == view {
                        return;
                    }
                    model.set_view(view);
                    view_icon.set_icon_name(Some(view.icon()));
                    Self::apply_view(&listbox, &grid, view, rail.get());
                    popover.popdown();
                }
            ));
            views.append(&button);
        }
        options.append(&views);
        popover.set_child(Some(&options));
        sort_button.set_popover(Some(&popover));

        tools.append(&search_toggle);
        tools.append(&entry);
        tools.append(&spacer);
        tools.append(&sort_button);
        tools
    }

    /// Hides the sidebar after a choice when it opened over the content.
    fn close_overlay_sidebar(widget: &gtk::Widget) {
        let split = widget
            .ancestor(libadwaita::OverlaySplitView::static_type())
            .and_downcast::<libadwaita::OverlaySplitView>();
        if let Some(split) = split.filter(|s| s.is_collapsed()) {
            split.set_show_sidebar(false);
        }
    }

    /// List or grid. The collapsed sidebar is always a column of covers.
    fn apply_view(listbox: &gtk::ListBox, grid: &gtk::FlowBox, view: LibraryView, rail: bool) {
        let grid_on = view == LibraryView::Grid && !rail;
        grid.set_visible(grid_on);
        listbox.set_visible(!grid_on);
    }

    /// The grid and the items it shows: the list's items without the
    /// section titles (a FlowBox with a model ignores filter functions, so
    /// the model itself is filtered).
    fn make_grid(
        list_store: &gio::ListStore,
        model: &Rc<NavigationPanelModel>,
    ) -> (gtk::FlowBox, gtk::FilterListModel) {
        let grid = Self::new_grid();
        let grid_items = Self::covers_only(list_store);
        let weak_model = Rc::downgrade(model);
        grid.bind_model(Some(&grid_items), move |obj| {
            let item = obj.downcast_ref::<NavigationPanelItem>().unwrap();
            Self::make_tile(item, weak_model.upgrade().as_deref())
        });
        (grid, grid_items)
    }

    /// One box for the list and the grid. It must fill the sidebar's height
    /// itself: with the list hidden, nothing else stretches the scrolled area
    /// and the grid got cut after one row (18/11).
    fn hold_list_and_grid(listbox: &gtk::ListBox, grid: &gtk::FlowBox) -> gtk::Box {
        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        holder.set_vexpand(true);
        holder.append(listbox);
        holder.append(grid);
        holder
    }

    /// The list's items without the section titles.
    fn covers_only(list_store: &gio::ListStore) -> gtk::FilterListModel {
        let filter = gtk::CustomFilter::new(|obj| {
            obj.downcast_ref::<NavigationPanelItem>()
                .is_some_and(|item| item.navigatable())
        });
        gtk::FilterListModel::new(Some(list_store.clone()), Some(filter))
    }

    fn new_grid() -> gtk::FlowBox {
        let grid = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(true)
            .min_children_per_line(2)
            .max_children_per_line(8)
            .column_spacing(2)
            .row_spacing(2)
            .valign(gtk::Align::Start)
            .visible(false)
            .build();
        grid.add_css_class("library-grid");
        grid
    }

    /// The item a grid cell shows (cells follow the grid's items).
    fn grid_item(
        grid_items: &gtk::FilterListModel,
        child: &gtk::FlowBoxChild,
    ) -> Option<NavigationPanelItem> {
        let index = child.index();
        if index < 0 {
            return None;
        }
        grid_items.item(index as u32).and_downcast()
    }

    /// A cover with the name and the kind under it.
    fn make_tile(item: &NavigationPanelItem, model: Option<&NavigationPanelModel>) -> gtk::Widget {
        let tile = gtk::Box::new(gtk::Orientation::Vertical, 6);
        tile.add_css_class("library-tile");
        tile.set_halign(gtk::Align::Center);
        tile.set_tooltip_text(Some(item.title().as_str()));

        let clip = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        clip.set_overflow(gtk::Overflow::Hidden);
        clip.set_halign(gtk::Align::Start);
        clip.add_css_class("navigation-row__cover");
        let image = gtk::Image::new();
        image.set_size_request(GRID_COVER_PX, GRID_COVER_PX);
        let url = item.image();
        if item.id() == "saved_tracks" {
            image.set_icon_name(Some("canto-heart-filled-symbolic"));
            image.set_pixel_size(28);
            clip.add_css_class("navigation-row__liked");
        } else if !url.is_empty() {
            image.set_pixel_size(GRID_COVER_PX);
            if item.round_cover() {
                clip.add_css_class("navigation-row__cover--round");
            }
            if let Some(model) = model {
                let scale = image.scale_factor().max(2);
                model.load_cover(&image, &url, GRID_COVER_PX * scale);
            }
        } else {
            image.set_icon_name(item.icon());
            image.set_pixel_size(28);
            clip.add_css_class("library-tile__placeholder");
        }
        clip.append(&image);
        tile.append(&clip);

        let title = gtk::Label::builder()
            .label(item.title())
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(1)
            .hexpand(true)
            .build();
        title.add_css_class("library-tile__title");
        tile.append(&title);

        let line = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        if item.pinned() {
            let pin = gtk::Image::from_icon_name("view-pin-symbolic");
            pin.set_pixel_size(12);
            pin.add_css_class("navigation-row__pin");
            line.append(&pin);
        }
        let subtitle = item.subtitle();
        if !subtitle.is_empty() {
            let label = gtk::Label::builder()
                .label(subtitle.as_str())
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .max_width_chars(1)
                .hexpand(true)
                .build();
            label.add_css_class("caption");
            label.add_css_class("dim-label");
            line.append(&label);
        }
        tile.append(&line);

        if let Some(model) = model {
            Self::set_tile_playing(tile.upcast_ref(), model.is_playing_from(item));
        }
        tile.upcast()
    }

    /// The name in the accent color while its music plays.
    fn set_tile_playing(tile: &gtk::Widget, playing: bool) {
        if playing {
            tile.add_css_class("library-tile--playing");
        } else {
            tile.remove_css_class("library-tile--playing");
        }
    }

    fn make_navigatable(
        item: &NavigationPanelItem,
        model: Option<&NavigationPanelModel>,
    ) -> gtk::Widget {
        let row = NavigationPanelRow::new(item.clone());
        row.set_selectable(false);
        if let Some(model) = model {
            row.set_playing(model.is_playing_from(item));
        }
        if let (Some(model), Some((image, url))) = (model, row.cover_target()) {
            let scale = image.scale_factor().max(2);
            model.load_cover(&image, &url, COVER_PX * scale);
        }
        row.upcast()
    }

    /// The collapsed sidebar is as wide as a cover: a scrollbar there sits
    /// on top of the covers. It still scrolls (wheel, touch), without one.
    fn set_scrollbar_rail(listbox: &gtk::ListBox, collapsed: bool) {
        if let Some(scrolled) = listbox
            .ancestor(gtk::ScrolledWindow::static_type())
            .and_downcast::<gtk::ScrolledWindow>()
        {
            scrolled.set_vscrollbar_policy(if collapsed {
                gtk::PolicyType::External
            } else {
                gtk::PolicyType::Automatic
            });
        }
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

    /// Highlights the item whose album, playlist, artist or Saved Tracks is
    /// playing (while it plays).
    /// What plays now goes to the top of "Recents".
    fn note_recent_play(&self) {
        let moved = self.model.bump_playing_to_recents();
        if moved && self.model.sort() == LibrarySort::Recents {
            self.model.apply_navigation_panel_items(&self.list_store);
        }
        self.model.refresh_recents(&self.list_store);
    }

    fn mark_playing(&self) {
        let mut child = self.listbox.first_child();
        while let Some(widget) = child {
            if let Some(row) = widget.downcast_ref::<NavigationPanelRow>() {
                row.set_playing(self.model.is_playing_from(&row.item()));
            }
            child = widget.next_sibling();
        }
        let mut child = self.grid.first_child();
        while let Some(widget) = child {
            if let Some(cell) = widget.downcast_ref::<gtk::FlowBoxChild>() {
                if let (Some(item), Some(tile)) =
                    (Self::grid_item(&self.grid_items, cell), cell.child())
                {
                    Self::set_tile_playing(&tile, self.model.is_playing_from(&item));
                }
            }
            child = widget.next_sibling();
        }
    }

    /// Refills the list once the current burst of updates is over: artists
    /// and albums arrive page after page.
    fn update_playlists_in_navigation_panel(&self) {
        if self.refill_queued.replace(true) {
            return;
        }
        let model = Rc::downgrade(&self.model);
        let list_store = self.list_store.downgrade();
        let queued = Rc::clone(&self.refill_queued);
        glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
            queued.set(false);
            if let (Some(model), Some(list_store)) = (model.upgrade(), list_store.upgrade()) {
                model.apply_navigation_panel_items(&list_store);
            }
        });
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
                BrowserEvent::SavedPlaylistsUpdated
                | BrowserEvent::PinnedPlaylistsUpdated
                | BrowserEvent::LibraryUpdated
                | BrowserEvent::SavedArtistsUpdated,
            ) => {
                self.update_playlists_in_navigation_panel();
                // The library loads once logged in: the history can come now
                self.model.refresh_recents(&self.list_store);
            }
            AppEvent::PlaybackEvent(
                PlaybackEvent::TrackChanged(_)
                | PlaybackEvent::SourceChanged
                | PlaybackEvent::PlaybackPaused
                | PlaybackEvent::PlaybackResumed
                | PlaybackEvent::PlaybackStopped
                | PlaybackEvent::PlayStateSynced(_)
                | PlaybackEvent::TrackSynced,
            ) => {
                self.mark_playing();
                if matches!(
                    event,
                    AppEvent::PlaybackEvent(
                        PlaybackEvent::TrackChanged(_) | PlaybackEvent::SourceChanged
                    )
                ) {
                    self.note_recent_play();
                }
            }
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
    use super::NavigationPanelDestination;
    use crate::app::models::PlaylistSummary;

    let listbox = gtk::ListBox::new();
    listbox.add_css_class("navigation-sidebar");
    let saved = NavigationPanelItem::from_destination(NavigationPanelDestination::SavedTracks);
    saved.set_subtitle("Playlist".to_string());
    let mut rows = vec![NavigationPanel::make_navigatable(&saved, None)];
    let samples = [
        ("Rap Nacional", "Playlist • gusthx", None),
        ("XTRANHO", "Album • Niink", Some("album")),
        ("Matuê", "Artist", Some("artist")),
    ];
    for (i, (title, subtitle, kind)) in samples.iter().enumerate() {
        let destination = match kind {
            Some("album") => NavigationPanelDestination::Album {
                id: i.to_string(),
                title: title.to_string(),
            },
            Some(_) => NavigationPanelDestination::Artist {
                id: i.to_string(),
                title: title.to_string(),
            },
            None => NavigationPanelDestination::Playlist(PlaylistSummary {
                id: i.to_string(),
                title: title.to_string(),
            }),
        };
        let item = NavigationPanelItem::from_destination(destination);
        item.set_image("https://example.com/cover.jpg");
        item.set_subtitle(subtitle.to_string());
        item.set_pinned(i == 0);
        let row = NavigationPanel::make_navigatable(&item, None);
        if i == 1 {
            if let Some(row) = row.downcast_ref::<NavigationPanelRow>() {
                row.set_playing(true);
            }
        }
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

/// "View As" > Grid with a few covers, to look at its rendering.
#[cfg(test)]
pub(crate) fn grid_sample_for_tests(cover: &gtk::gdk::Paintable) -> gtk::Widget {
    use super::NavigationPanelDestination;
    use crate::app::models::PlaylistSummary;

    let grid = NavigationPanel::new_grid();
    grid.set_visible(true);
    let saved = NavigationPanelItem::from_destination(NavigationPanelDestination::SavedTracks);
    saved.set_subtitle("Playlist".to_string());
    grid.append(&NavigationPanel::make_tile(&saved, None));
    let samples = [
        ("Rap Nacional", "Playlist • gusthx", None),
        ("XTRANHO", "Album • Niink", Some("album")),
        ("Matuê", "Artist", Some("artist")),
        (
            "A very long playlist name that does not fit",
            "Playlist • gusthx",
            None,
        ),
        ("Discover Weekly", "Playlist • Spotify", None),
    ];
    for (i, (title, subtitle, kind)) in samples.iter().enumerate() {
        let destination = match kind {
            Some("album") => NavigationPanelDestination::Album {
                id: i.to_string(),
                title: title.to_string(),
            },
            Some(_) => NavigationPanelDestination::Artist {
                id: i.to_string(),
                title: title.to_string(),
            },
            None => NavigationPanelDestination::Playlist(PlaylistSummary {
                id: i.to_string(),
                title: title.to_string(),
            }),
        };
        let item = NavigationPanelItem::from_destination(destination);
        item.set_image("https://example.com/cover.jpg");
        item.set_subtitle(subtitle.to_string());
        item.set_pinned(i == 0);
        let tile = NavigationPanel::make_tile(&item, None);
        NavigationPanel::set_tile_playing(&tile, i == 1);
        if let Some(image) = tile
            .first_child()
            .and_then(|clip| clip.first_child())
            .and_downcast::<gtk::Image>()
        {
            image.set_paintable(Some(cover));
        }
        grid.append(&tile);
    }
    // Built like the app's sidebar: a heading over a scrolled area whose
    // list is hidden while the grid shows
    let listbox = gtk::ListBox::new();
    listbox.set_vexpand(true);
    listbox.set_visible(false);
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_child(Some(&NavigationPanel::hold_list_and_grid(&listbox, &grid)));
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar.append(&gtk::Label::new(Some("Your Library")));
    sidebar.append(&scrolled);
    sidebar.upcast()
}

/// Titles of the grid's items for a list with section titles in it.
#[cfg(test)]
pub(crate) fn grid_titles_for_tests() -> Vec<String> {
    use super::NavigationPanelDestination;

    let store = gio::ListStore::new::<NavigationPanelItem>();
    store.append(&NavigationPanelItem::pinned_section());
    store.append(&NavigationPanelItem::from_destination(
        NavigationPanelDestination::SavedTracks,
    ));
    store.append(&NavigationPanelItem::library_section());
    store.append(&NavigationPanelItem::from_destination(
        NavigationPanelDestination::Artist {
            id: "1".to_string(),
            title: "Matuê".to_string(),
        },
    ));
    store.append(&NavigationPanelItem::playlists_section());
    let items = NavigationPanel::covers_only(&store);
    (0..items.n_items())
        .filter_map(|i| items.item(i).and_downcast::<NavigationPanelItem>())
        .map(|item| item.title().to_string())
        .collect()
}
