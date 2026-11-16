use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gettextrs::gettext;
use gio::prelude::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, CompositeTemplate};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use riff_api::models::Device as ConnectDevice;
use riff_config::settings::{APP_THEME, SETTINGS};

use crate::app::components::sleep_timer::SleepTimerController;
use crate::app::components::track_radio::start_track_radio;
use crate::app::components::utils::Debouncer;
use crate::app::models::Track;
use crate::app::state::{
    load_context, start_actions, BrowserAction, Device, HomePage, PlaybackAction, Route,
};
use crate::app::{AppAction, AppModel, Dispatcher, SongsSource};

#[derive(Clone)]
enum PaletteAction {
    Navigate(Route),
    OpenPreferences,
    ShowNowPlaying,
    ToggleQueue,
    ToggleLyrics,
    FullscreenLyrics,
    MiniPlayer,
    StartTrackRadio(Track),
    SetTheme(&'static str),
    SetSleepTimerOff,
    SetSleepTimerEndOfTrack,
    SetSleepTimerMinutes(u32),
    SwitchDevice(Device),
    PlayTrack(Track),
    ViewAlbum(String),
    ViewArtist(String),
    ViewPlaylist(String),
}

#[derive(Clone)]
struct PaletteEntry {
    title: String,
    subtitle: String,
    category: String,
    icon_name: &'static str,
    shortcut_hint: Option<&'static str>,
    keywords: String,
    action: PaletteAction,
}

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/command_palette.ui")]
    pub struct CommandPaletteDialog {
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub search_spinner: TemplateChild<gtk::Spinner>,
        #[template_child]
        pub results_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub scroller: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub results_list: TemplateChild<gtk::ListBox>,

        pub(super) ctx: RefCell<Option<Rc<PaletteRuntime>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CommandPaletteDialog {
        const NAME: &'static str = "CommandPaletteDialog";
        type Type = super::CommandPaletteDialog;
        type ParentType = libadwaita::Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for CommandPaletteDialog {
        fn constructed(&self) {
            self.parent_constructed();
            crate::app::components::display_add_css_provider(resource!(
                "/components/command_palette.css"
            ));
        }
    }
    impl WidgetImpl for CommandPaletteDialog {}
    impl AdwDialogImpl for CommandPaletteDialog {}
}

struct PaletteRuntime {
    window: libadwaita::ApplicationWindow,
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    sleep_timer: Rc<SleepTimerController>,
    settings: gio::Settings,
    debouncer: Debouncer,
    search_gen: Cell<u64>,
    displayed_actions: RefCell<Vec<PaletteAction>>,
    remote_entries: RefCell<Vec<PaletteEntry>>,
}

glib::wrapper! {
    pub struct CommandPaletteDialog(ObjectSubclass<imp::CommandPaletteDialog>)
        @extends gtk::Widget, libadwaita::Dialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl CommandPaletteDialog {
    pub fn open(
        window: &libadwaita::ApplicationWindow,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
        sleep_timer: Rc<SleepTimerController>,
    ) {
        if let Some(existing) = window
            .visible_dialog()
            .and_then(|d| d.downcast::<Self>().ok())
        {
            existing.close();
            return;
        }

        let dialog: Self = glib::Object::new();
        let runtime = Rc::new(PaletteRuntime {
            window: window.clone(),
            app_model: Rc::clone(&app_model),
            dispatcher: dispatcher.clone(),
            sleep_timer,
            settings: gio::Settings::new(SETTINGS),
            debouncer: Debouncer::new(),
            search_gen: Cell::new(0),
            displayed_actions: RefCell::new(Vec::new()),
            remote_entries: RefCell::new(Vec::new()),
        });
        *dialog.imp().ctx.borrow_mut() = Some(Rc::clone(&runtime));

        let rt_closed = Rc::clone(&runtime);
        dialog.connect_closed(move |_| {
            rt_closed.debouncer.stop();
            rt_closed.search_gen.set(rt_closed.search_gen.get().wrapping_add(1));
        });

        dialog.setup_signals(&runtime);
        dialog.refresh_devices_async(&runtime);
        dialog.rebuild_list("", false);
        dialog.present(Some(window));
        dialog.imp().search_entry.grab_focus();
    }

    fn setup_signals(&self, runtime: &Rc<PaletteRuntime>) {
        let imp = self.imp();

        // Esc closes immediately even while SearchEntry has focus.
        let weak_close = self.downgrade();
        imp.search_entry.connect_stop_search(move |_| {
            if let Some(d) = weak_close.upgrade() {
                d.close();
            }
        });

        let key_ctrl = gtk::EventControllerKey::new();
        key_ctrl.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak_key = self.downgrade();
        key_ctrl.connect_key_pressed(move |_, key, _, _| {
            let Some(dialog) = weak_key.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match key {
                gdk::Key::Escape => {
                    dialog.close();
                    glib::Propagation::Stop
                }
                gdk::Key::Down | gdk::Key::KP_Down => {
                    dialog.move_selection(1);
                    glib::Propagation::Stop
                }
                gdk::Key::Up | gdk::Key::KP_Up => {
                    dialog.move_selection(-1);
                    glib::Propagation::Stop
                }
                gdk::Key::Return | gdk::Key::KP_Enter => {
                    dialog.activate_selected();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.add_controller(key_ctrl);

        let weak_change = self.downgrade();
        let rt_change = Rc::clone(runtime);
        imp.search_entry.connect_search_changed(move |entry| {
            let Some(dialog) = weak_change.upgrade() else {
                return;
            };
            let query = entry.text().to_string();
            dialog.rebuild_list(&query, false);
            dialog.schedule_catalog_search(&rt_change, &query);
        });

        let weak_act = self.downgrade();
        imp.search_entry.connect_activate(move |_| {
            if let Some(dialog) = weak_act.upgrade() {
                dialog.activate_selected();
            }
        });

        let weak_row = self.downgrade();
        imp.results_list.connect_row_activated(move |_, row| {
            if let Some(dialog) = weak_row.upgrade() {
                let idx = row.index();
                if idx >= 0 {
                    dialog.execute_index(idx as usize);
                }
            }
        });
    }

    fn refresh_devices_async(&self, runtime: &Rc<PaletteRuntime>) {
        if !runtime.app_model.api().provider_capabilities().supports_connect {
            return;
        }
        let api = runtime.app_model.api();
        let dispatcher = runtime.dispatcher.clone();
        let weak = self.downgrade();
        glib::MainContext::default().spawn_local(async move {
            if let Ok(devices) = api.get_devices(riff_api::Load::background()).await {
                dispatcher.dispatch(PlaybackAction::SetAvailableDevices(devices).into());
                if let Some(dialog) = weak.upgrade() {
                    let query = dialog.imp().search_entry.text().to_string();
                    dialog.rebuild_list(&query, true);
                }
            }
        });
    }

    fn schedule_catalog_search(&self, runtime: &Rc<PaletteRuntime>, query: &str) {
        let trimmed = query.trim().to_string();
        let next_gen = runtime.search_gen.get().wrapping_add(1);
        runtime.search_gen.set(next_gen);

        if trimmed.chars().count() < 2 {
            runtime.debouncer.stop();
            runtime.remote_entries.borrow_mut().clear();
            self.imp().search_spinner.set_visible(false);
            self.imp().search_spinner.set_spinning(false);
            let current = self.imp().search_entry.text().to_string();
            self.rebuild_list(&current, false);
            return;
        }

        self.imp().search_spinner.set_visible(true);
        self.imp().search_spinner.set_spinning(true);

        let weak = self.downgrade();
        let rt = Rc::clone(runtime);
        runtime.debouncer.debounce(200, move || {
            let api = rt.app_model.api();
            let weak_inner = weak.clone();
            let rt_inner = Rc::clone(&rt);
            let q = trimmed.clone();
            glib::MainContext::default().spawn_local(async move {
                let res = api.search(&q, 0, 4, crate::app::load::visible()).await;
                if rt_inner.search_gen.get() != next_gen {
                    return;
                }
                let Some(dialog) = weak_inner.upgrade() else {
                    return;
                };
                dialog.imp().search_spinner.set_visible(false);
                dialog.imp().search_spinner.set_spinning(false);

                let entries = match res {
                    Ok(results) => build_remote_entries(&results),
                    Err(e) => {
                        debug!("command_palette: search failed: {e}");
                        Vec::new()
                    }
                };
                *rt_inner.remote_entries.borrow_mut() = entries;
                let latest_query = dialog.imp().search_entry.text().to_string();
                dialog.rebuild_list(&latest_query, true);
            });
        });
    }

    fn move_selection(&self, delta: i32) {
        let list = &self.imp().results_list;
        let count = count_rows(list);
        if count == 0 {
            return;
        }
        let current = list.selected_row().map(|r| r.index()).unwrap_or(-1);
        let next = if current < 0 {
            if delta >= 0 { 0 } else { count as i32 - 1 }
        } else {
            (current + delta).clamp(0, count as i32 - 1)
        };
        if let Some(row) = list.row_at_index(next) {
            list.select_row(Some(&row));
            row.grab_focus();
            // Keep typing cursor smooth if user presses keys again by restoring focus to entry?
            // Keeping selection visual and focusing SearchEntry lets Backspace/typing work seamlessly!
            self.imp().search_entry.grab_focus();
            self.imp().search_entry.set_position(-1);
            scroll_row_into_view(&self.imp().scroller, &row);
        }
    }

    fn activate_selected(&self) {
        let list = &self.imp().results_list;
        let idx = list
            .selected_row()
            .map(|r| r.index())
            .filter(|&i| i >= 0)
            .unwrap_or(0);
        self.execute_index(idx as usize);
    }

    fn execute_index(&self, idx: usize) {
        let Some(runtime) = self.imp().ctx.borrow().as_ref().cloned() else {
            return;
        };
        let action = runtime.displayed_actions.borrow().get(idx).cloned();
        let Some(action) = action else {
            return;
        };
        self.close();
        execute_action(&runtime, action);
    }

    fn rebuild_list(&self, query: &str, preserve_selection: bool) {
        let Some(runtime) = self.imp().ctx.borrow().as_ref().cloned() else {
            return;
        };
        let prev_selected = if preserve_selection {
            self.imp()
                .results_list
                .selected_row()
                .map(|r| r.index())
                .filter(|&i| i >= 0)
        } else {
            None
        };

        let mut items = build_local_entries(&runtime);
        let filtered = filter_entries(&items, query);
        items.clear();
        items.extend(filtered);

        if query.trim().chars().count() >= 2 {
            items.extend(runtime.remote_entries.borrow().iter().cloned());
        }

        let imp = self.imp();
        while let Some(child) = imp.results_list.first_child() {
            imp.results_list.remove(&child);
        }

        let mut actions = Vec::with_capacity(items.len());
        for entry in items {
            let row = build_row(&entry);
            imp.results_list.append(&row);
            actions.push(entry.action);
        }
        let item_count = actions.len();
        *runtime.displayed_actions.borrow_mut() = actions;

        if item_count == 0 {
            imp.results_stack.set_visible_child_name("empty");
        } else {
            imp.results_stack.set_visible_child_name("list");
            let target_idx = prev_selected
                .map(|idx| idx.min(item_count as i32 - 1))
                .unwrap_or(0);
            if let Some(row) = imp.results_list.row_at_index(target_idx) {
                imp.results_list.select_row(Some(&row));
            }
        }
    }
}

fn count_rows(list: &gtk::ListBox) -> usize {
    let mut n = 0;
    let mut child = list.first_child();
    while let Some(w) = child {
        n += 1;
        child = w.next_sibling();
    }
    n
}

fn scroll_row_into_view(scroller: &gtk::ScrolledWindow, row: &gtk::ListBoxRow) {
    let Some(pt) = row.compute_point(scroller, &gtk::graphene::Point::zero()) else {
        return;
    };
    let y = f64::from(pt.y());
    let row_h = f64::from(row.height());
    let view_h = f64::from(scroller.height());
    let vadj = scroller.vadjustment();
    let cur = vadj.value();
    if y < 0.0 {
        vadj.set_value((cur + y).max(0.0));
    } else if y + row_h > view_h && view_h > 1.0 {
        vadj.set_value(cur + (y + row_h - view_h));
    }
}

fn build_row(entry: &PaletteEntry) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);

    let icon = gtk::Image::from_icon_name(entry.icon_name);
    icon.set_pixel_size(16);
    hbox.append(&icon);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 1);
    vbox.set_hexpand(true);

    let title = gtk::Label::new(Some(&entry.title));
    title.set_halign(gtk::Align::Start);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    vbox.append(&title);

    if !entry.subtitle.is_empty() {
        let sub = gtk::Label::new(Some(&entry.subtitle));
        sub.set_halign(gtk::Align::Start);
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        sub.add_css_class("caption");
        sub.add_css_class("dim-label");
        vbox.append(&sub);
    }
    hbox.append(&vbox);

    let right = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    right.set_valign(gtk::Align::Center);
    if let Some(hint) = entry.shortcut_hint {
        let hint_lbl = gtk::Label::new(Some(hint));
        hint_lbl.add_css_class("caption");
        hint_lbl.add_css_class("dim-label");
        right.append(&hint_lbl);
    }
    let badge = gtk::Label::new(Some(&entry.category));
    badge.add_css_class("command-palette__badge");
    badge.add_css_class("dim-label");
    right.append(&badge);
    hbox.append(&right);

    row.set_child(Some(&hbox));
    row
}

fn build_local_entries(runtime: &PaletteRuntime) -> Vec<PaletteEntry> {
    let mut entries = Vec::new();
    let nav_cat = gettext("Navigation");
    let theme_cat = gettext("Theme");
    let action_cat = gettext("Action");
    let timer_cat = gettext("Sleep Timer");
    let device_cat = gettext("Device");

    entries.push(PaletteEntry {
        title: gettext("Now Playing"),
        subtitle: gettext("Show the current track in the side panel"),
        category: nav_cat.clone(),
        icon_name: "now-playing-symbolic",
        shortcut_hint: Some("Alt+1"),
        keywords: "home now playing current track cover credits".into(),
        action: PaletteAction::ShowNowPlaying,
    });
    entries.push(PaletteEntry {
        title: gettext("Library"),
        subtitle: gettext("Browse saved albums"),
        category: nav_cat.clone(),
        icon_name: "library-music-symbolic",
        shortcut_hint: Some("Alt+3"),
        keywords: "library albums collection".into(),
        action: PaletteAction::Navigate(Route::Home(HomePage::Library)),
    });
    entries.push(PaletteEntry {
        title: gettext("Search"),
        subtitle: gettext("Open search page"),
        category: nav_cat.clone(),
        icon_name: "system-search-symbolic",
        shortcut_hint: Some("Ctrl+F"),
        keywords: "search find catalog".into(),
        action: PaletteAction::Navigate(Route::Search),
    });
    entries.push(PaletteEntry {
        title: gettext("Liked Songs"),
        subtitle: gettext("Open your saved tracks"),
        category: nav_cat.clone(),
        icon_name: "emote-love-symbolic",
        shortcut_hint: Some("Alt+5"),
        keywords: "liked saved tracks favorites".into(),
        action: PaletteAction::Navigate(Route::Home(HomePage::SavedTracks)),
    });
    entries.push(PaletteEntry {
        title: gettext("Artists"),
        subtitle: gettext("Open followed artists"),
        category: nav_cat.clone(),
        icon_name: "avatar-default-symbolic",
        shortcut_hint: Some("Alt+2"),
        keywords: "artists followed".into(),
        action: PaletteAction::Navigate(Route::Home(HomePage::SavedArtists)),
    });
    entries.push(PaletteEntry {
        title: gettext("Playlists"),
        subtitle: gettext("Open saved playlists"),
        category: nav_cat.clone(),
        icon_name: "playlist2-symbolic",
        shortcut_hint: Some("Alt+4"),
        keywords: "playlists".into(),
        action: PaletteAction::Navigate(Route::Home(HomePage::SavedPlaylists)),
    });
    entries.push(PaletteEntry {
        title: gettext("Settings"),
        subtitle: gettext("Open Preferences dialog"),
        category: nav_cat,
        icon_name: "preferences-other-symbolic",
        shortcut_hint: Some("Ctrl+,"),
        keywords: "settings preferences integrations scrobble genius theme".into(),
        action: PaletteAction::OpenPreferences,
    });

    // Playback / Panel Toggles
    entries.push(PaletteEntry {
        title: gettext("Toggle Queue"),
        subtitle: gettext("Show or hide the playback queue panel"),
        category: action_cat.clone(),
        icon_name: "queue-symbolic",
        shortcut_hint: None,
        keywords: "toggle queue panel".into(),
        action: PaletteAction::ToggleQueue,
    });
    entries.push(PaletteEntry {
        title: gettext("Toggle Lyrics"),
        subtitle: gettext("Show or hide Lyrics and Track Notes"),
        category: action_cat.clone(),
        icon_name: "canto-lyrics-symbolic",
        shortcut_hint: None,
        keywords: "toggle lyrics about track genius notes".into(),
        action: PaletteAction::ToggleLyrics,
    });
    entries.push(PaletteEntry {
        title: gettext("Full-Screen Lyrics"),
        subtitle: gettext("Show the lyrics big, over the cover"),
        category: action_cat.clone(),
        icon_name: "view-fullscreen-symbolic",
        shortcut_hint: None,
        keywords: "fullscreen full screen lyrics karaoke big".into(),
        action: PaletteAction::FullscreenLyrics,
    });
    entries.push(PaletteEntry {
        title: gettext("Mini Player"),
        subtitle: gettext("Keep the controls in a small window"),
        category: action_cat.clone(),
        icon_name: "view-dual-symbolic",
        shortcut_hint: None,
        keywords: "mini player small window compact pip".into(),
        action: PaletteAction::MiniPlayer,
    });

    if let Some(current) = runtime.app_model.get_state().playback.current_song() {
        entries.push(PaletteEntry {
            title: format!("{}: {}", gettext("Start Track Radio"), current.title),
            subtitle: current.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
            category: action_cat,
            icon_name: "media-optical-cd-audio-symbolic",
            shortcut_hint: None,
            keywords: format!("track radio similar {}", current.title),
            action: PaletteAction::StartTrackRadio(current),
        });
    }

    // Sleep Timer presets
    entries.push(PaletteEntry {
        title: gettext("Sleep Timer: Off"),
        subtitle: gettext("Disable active sleep timer"),
        category: timer_cat.clone(),
        icon_name: "alarm-symbolic",
        shortcut_hint: None,
        keywords: "sleep timer off cancel disable".into(),
        action: PaletteAction::SetSleepTimerOff,
    });
    entries.push(PaletteEntry {
        title: gettext("Sleep Timer: End of Current Track"),
        subtitle: gettext("Pause playback when this song finishes"),
        category: timer_cat.clone(),
        icon_name: "alarm-symbolic",
        shortcut_hint: None,
        keywords: "sleep timer end track song finish".into(),
        action: PaletteAction::SetSleepTimerEndOfTrack,
    });
    for mins in [15_u32, 30, 45, 60] {
        entries.push(PaletteEntry {
            title: format!("{}: {mins} min", gettext("Sleep Timer")),
            subtitle: format!("{} {mins} {}", gettext("Pause playback after"), gettext("minutes")),
            category: timer_cat.clone(),
            icon_name: "alarm-symbolic",
            shortcut_hint: None,
            keywords: format!("sleep timer {mins} min minutes"),
            action: PaletteAction::SetSleepTimerMinutes(mins),
        });
    }

    // Live Themes
    let themes: [(&'static str, String); 6] = [
        ("omarchy", gettext("Omarchy Live")),
        ("neutral", gettext("Neutral")),
        ("spotify-green", gettext("Spotify Green")),
        ("oled-dark", gettext("OLED Dark")),
        ("warm-vinyl", gettext("Warm Vinyl")),
        ("purple-night", gettext("Purple Night")),
    ];
    let current_theme = runtime.settings.string(APP_THEME).to_string();
    for (key, label) in themes {
        let active_marker = if current_theme == key {
            gettext("Active theme")
        } else {
            gettext("Switch theme live")
        };
        entries.push(PaletteEntry {
            title: label.clone(),
            subtitle: active_marker,
            category: theme_cat.clone(),
            icon_name: "preferences-other-symbolic",
            shortcut_hint: None,
            keywords: format!("theme {label} {key}"),
            action: PaletteAction::SetTheme(key),
        });
    }

    // Playback Devices
    let state = runtime.app_model.get_state();
    let is_local = matches!(state.playback.current_device(), Device::Local);
    entries.push(PaletteEntry {
        title: gettext("This Device"),
        subtitle: if is_local {
            gettext("Active playback device")
        } else {
            gettext("Transfer playback locally")
        },
        category: device_cat.clone(),
        icon_name: "audio-speakers-symbolic",
        shortcut_hint: None,
        keywords: "device this local speaker".into(),
        action: PaletteAction::SwitchDevice(Device::Local),
    });
    for dev in state.playback.available_devices() {
        entries.push(device_entry(dev, &device_cat));
    }

    entries
}

fn device_entry(dev: &ConnectDevice, category: &str) -> PaletteEntry {
    let icon = match dev.kind {
        riff_api::DeviceKind::Computer => "computer-symbolic",
        riff_api::DeviceKind::Phone => "phone-symbolic",
        riff_api::DeviceKind::Tv => "tv-symbolic",
        riff_api::DeviceKind::Speaker | riff_api::DeviceKind::Other => "audio-speakers-symbolic",
    };
    PaletteEntry {
        title: dev.label.clone(),
        subtitle: if dev.is_active {
            gettext("Active Connect device")
        } else {
            gettext("Transfer playback to device")
        },
        category: category.to_string(),
        icon_name: icon,
        shortcut_hint: None,
        keywords: format!("device connect {}", dev.label),
        action: PaletteAction::SwitchDevice(Device::Connect(dev.clone())),
    }
}

fn build_remote_entries(results: &riff_api::models::SearchResults) -> Vec<PaletteEntry> {
    let mut out = Vec::new();
    let track_cat = gettext("Track");
    let album_cat = gettext("Album");
    let artist_cat = gettext("Artist");
    let playlist_cat = gettext("Playlist");

    for track in &results.tracks.items {
        let artists = track
            .artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        out.push(PaletteEntry {
            title: track.title.clone(),
            subtitle: artists,
            category: track_cat.clone(),
            icon_name: "audio-x-generic-symbolic",
            shortcut_hint: None,
            keywords: String::new(),
            action: PaletteAction::PlayTrack(track.clone()),
        });
    }
    for album in &results.albums.items {
        let artists = album
            .artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        out.push(PaletteEntry {
            title: album.title.clone(),
            subtitle: artists,
            category: album_cat.clone(),
            icon_name: "media-optical-cd-audio-symbolic",
            shortcut_hint: None,
            keywords: String::new(),
            action: PaletteAction::ViewAlbum(album.rri.id.clone()),
        });
    }
    for artist in &results.artists.items {
        out.push(PaletteEntry {
            title: artist.name.clone(),
            subtitle: gettext("Open artist page"),
            category: artist_cat.clone(),
            icon_name: "avatar-default-symbolic",
            shortcut_hint: None,
            keywords: String::new(),
            action: PaletteAction::ViewArtist(artist.rri.id.clone()),
        });
    }
    for playlist in &results.playlists.items {
        out.push(PaletteEntry {
            title: playlist.title.clone(),
            subtitle: playlist
                .owner
                .as_ref()
                .map(|o| o.display_name.clone())
                .unwrap_or_default(),
            category: playlist_cat.clone(),
            icon_name: "playlist2-symbolic",
            shortcut_hint: None,
            keywords: String::new(),
            action: PaletteAction::ViewPlaylist(playlist.rri.id.clone()),
        });
    }
    out
}

fn filter_entries(entries: &[PaletteEntry], query: &str) -> Vec<PaletteEntry> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return entries.to_vec();
    }
    let tokens: Vec<&str> = q.split_whitespace().collect();
    let mut scored: Vec<(i32, &PaletteEntry)> = entries
        .iter()
        .filter_map(|e| {
            let title_l = e.title.to_ascii_lowercase();
            let sub_l = e.subtitle.to_ascii_lowercase();
            let cat_l = e.category.to_ascii_lowercase();
            let kw_l = e.keywords.to_ascii_lowercase();
            let all_match = tokens.iter().all(|tok| {
                title_l.contains(tok)
                    || sub_l.contains(tok)
                    || cat_l.contains(tok)
                    || kw_l.contains(tok)
            });
            if !all_match {
                return None;
            }
            let score = if title_l.starts_with(&q) {
                3
            } else if title_l.contains(&q) {
                2
            } else {
                1
            };
            Some((score, e))
        })
        .collect();
    scored.sort_by_key(|a| std::cmp::Reverse(a.0));
    scored.into_iter().map(|(_, e)| e.clone()).collect()
}

fn execute_action(runtime: &PaletteRuntime, action: PaletteAction) {
    match action {
        PaletteAction::Navigate(route) => {
            runtime
                .dispatcher
                .dispatch(BrowserAction::Navigate(route).into());
        }
        PaletteAction::OpenPreferences => {
            let _ = WidgetExt::activate_action(&runtime.window, "win.preferences", None);
        }
        PaletteAction::ShowNowPlaying => {
            let _ = WidgetExt::activate_action(&runtime.window, "win.show-now-playing", None);
        }
        PaletteAction::ToggleQueue => {
            let _ = WidgetExt::activate_action(&runtime.window, "win.show-queue", None);
        }
        PaletteAction::MiniPlayer => {
            let _ = WidgetExt::activate_action(&runtime.window, "win.mini-player", None);
        }
        PaletteAction::FullscreenLyrics => {
            let _ = WidgetExt::activate_action(&runtime.window, "win.fullscreen-lyrics", None);
        }
        PaletteAction::ToggleLyrics => {
            let _ = WidgetExt::activate_action(&runtime.window, "win.show-lyrics", None);
        }
        PaletteAction::StartTrackRadio(track) => {
            start_track_radio(&runtime.app_model, &runtime.dispatcher, track);
        }
        PaletteAction::SetTheme(key) => {
            let _ = runtime.settings.set_string(APP_THEME, key);
        }
        PaletteAction::SetSleepTimerOff => runtime.sleep_timer.set_off(),
        PaletteAction::SetSleepTimerEndOfTrack => runtime.sleep_timer.set_end_of_track(),
        PaletteAction::SetSleepTimerMinutes(m) => runtime.sleep_timer.set_minutes(m),
        PaletteAction::SwitchDevice(device) => {
            runtime
                .dispatcher
                .dispatch(PlaybackAction::SwitchDevice(device).into());
        }
        PaletteAction::PlayTrack(track) => {
            let source = SongsSource::Search(track.title.clone());
            let load = PlaybackAction::LoadContextSongs(source.clone(), vec![track.clone()]);
            for act in start_actions(false, load_context(source, Some(track.title), load)) {
                runtime.dispatcher.dispatch(act);
            }
        }
        PaletteAction::ViewAlbum(id) => {
            runtime.dispatcher.dispatch(AppAction::ViewAlbum(id));
        }
        PaletteAction::ViewArtist(id) => {
            runtime.dispatcher.dispatch(AppAction::ViewArtist(id));
        }
        PaletteAction::ViewPlaylist(id) => {
            runtime.dispatcher.dispatch(AppAction::ViewPlaylist(id));
        }
    }
}

pub fn expose_widgets() {
    CommandPaletteDialog::static_type();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_commands_case_insensitively_across_title_and_keywords() {
        let entries = vec![
            PaletteEntry {
                title: "OLED Dark".into(),
                subtitle: "Switch theme live".into(),
                category: "Theme".into(),
                icon_name: "preferences-other-symbolic",
                shortcut_hint: None,
                keywords: "theme oled dark".into(),
                action: PaletteAction::SetTheme("oled-dark"),
            },
            PaletteEntry {
                title: "Liked Songs".into(),
                subtitle: "Open your saved tracks".into(),
                category: "Navigation".into(),
                icon_name: "emote-love-symbolic",
                shortcut_hint: None,
                keywords: "saved tracks".into(),
                action: PaletteAction::Navigate(Route::Home(HomePage::SavedTracks)),
            },
        ];

        let filtered = filter_entries(&entries, "oled");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "OLED Dark");

        let by_keyword = filter_entries(&entries, "saved");
        assert_eq!(by_keyword.len(), 1);
        assert_eq!(by_keyword[0].title, "Liked Songs");
    }
}
