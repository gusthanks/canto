use gio::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::app::components::{
    AppHeaderBar, BottomSheet, Component, EventListener, FullscreenLyrics, LyricsPage, MiniPlayer,
    NowPanel, QueuePage, QueuePageWidget, UtilityPanel,
};
use crate::app::state::PlaybackEvent;
use crate::app::{AppAction, AppEvent, AppModel, Dispatcher};
use crate::settings::{QUEUE_SIDE_PANEL, SETTINGS};

const TOGGLE_ACTION: &str = "utility-panel";
const SHOW_QUEUE_ACTION: &str = "show-queue";
const SHOW_LYRICS_ACTION: &str = "show-lyrics";
const SHOW_NOW_PLAYING_ACTION: &str = "show-now-playing";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placement {
    Side,
    Sheet,
}

impl Placement {
    fn from_settings(settings: &gio::Settings) -> Self {
        if settings.boolean(QUEUE_SIDE_PANEL) {
            Self::Side
        } else {
            Self::Sheet
        }
    }
}

struct Panels {
    now: gtk::Widget,
    queue: gtk::Widget,
    lyrics: gtk::Widget,
    panel: UtilityPanel,
    sheet: BottomSheet,
    app_header: AppHeaderBar,
    action: gio::SimpleAction,
    placement: Cell<Option<Placement>>,
}

impl Panels {
    fn is_open(&self) -> bool {
        match self.placement.get() {
            Some(Placement::Side) => self.panel.is_open(),
            Some(Placement::Sheet) => self.sheet.is_open(),
            None => false,
        }
    }

    fn set_open(&self, open: bool) {
        match self.placement.get() {
            Some(Placement::Side) => self.panel.set_open(open),
            Some(Placement::Sheet) => self.sheet.set_open(open),
            None => {}
        }
    }

    fn sync_header(&self) {
        self.app_header
            .set_utility_panel_beside(self.panel.is_open());
    }

    /// Whether the side panel holds `tab` ("now", "queue" or "lyrics"). The
    /// queue is in the bottom sheet instead when that's where it's placed.
    fn panel_has(&self, tab: &str) -> bool {
        tab != "queue" || self.placement.get() == Some(Placement::Side)
    }

    /// What the player bar's cover, queue and lyrics buttons do: an open
    /// side panel switches to that tab, or closes on the tab it already
    /// shows; a closed one opens on it.
    fn toggle_view(&self, tab: &'static str) {
        if self.panel.is_open() && self.panel_has(tab) {
            if self.panel.active_tab() == tab {
                self.panel.set_open(false);
            } else {
                self.panel.show_tab(tab);
            }
            return;
        }
        if self.panel_has(tab) {
            self.panel.show_tab(tab);
            self.panel.set_open(true);
        } else {
            self.sheet.set_open(!self.sheet.is_open());
        }
    }

    fn place(&self, placement: Placement) {
        if self.placement.get() == Some(placement) {
            return;
        }
        self.set_open(false);
        self.panel.set_open(false);
        match self.placement.get() {
            Some(Placement::Side) => {
                self.panel.set_selection(None, false);
                self.panel.set_pages(None, None, None);
            }
            Some(Placement::Sheet) => {
                self.sheet.set_selecting(false);
                self.sheet.set_page(None);
                self.panel.set_pages(None, None, None);
            }
            None => {}
        }
        self.placement.set(Some(placement));
        if let Some(page) = self.queue.downcast_ref::<QueuePageWidget>() {
            page.set_selection_bar_hidden(placement == Placement::Side);
        }
        match placement {
            Placement::Side => {
                self.panel
                    .set_pages(Some(&self.now), Some(&self.queue), Some(&self.lyrics))
            }
            Placement::Sheet => {
                self.sheet.set_page(self.queue.downcast_ref());
                self.panel
                    .set_pages(Some(&self.now), None, Some(&self.lyrics));
            }
        }
        let open = self.is_open();
        if self.action.state().and_then(|s| s.get::<bool>()) != Some(open) {
            self.action.set_state(&open.to_variant());
        }
        self.sync_header();
    }
}

pub struct PanelManager {
    now: NowPanel,
    fullscreen: FullscreenLyrics,
    mini: MiniPlayer,
    queue: QueuePage,
    lyrics: LyricsPage,
    panels: Rc<Panels>,
    app_model: Rc<AppModel>,
    _settings: gio::Settings,
}

impl PanelManager {
    pub fn new(
        window: &libadwaita::ApplicationWindow,
        panel: UtilityPanel,
        sheet: BottomSheet,
        app_header: AppHeaderBar,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
    ) -> Self {
        let queue = QueuePage::new(app_model.clone(), dispatcher.clone());
        let lyrics = LyricsPage::new(app_model.clone(), dispatcher.clone());
        let now = NowPanel::new(app_model.clone(), dispatcher.clone());
        let fullscreen = FullscreenLyrics::new(window, app_model.clone(), dispatcher.clone());
        let mini = MiniPlayer::new(window, app_model.clone(), dispatcher.clone());
        panel.connect_selection_cancel(clone!(
            #[weak(rename_to = model)]
            queue.model(),
            move || model.cancel_selection()
        ));
        panel.connect_select_all(clone!(
            #[weak(rename_to = model)]
            queue.model(),
            move || model.toggle_select_all()
        ));
        let panels = Rc::new(Panels {
            now: now.widget().clone(),
            queue: queue.widget().clone().upcast(),
            lyrics: lyrics.widget().clone().upcast(),
            panel,
            sheet,
            app_header,
            action: gio::SimpleAction::new_stateful(TOGGLE_ACTION, None, &false.to_variant()),
            placement: Cell::new(None),
        });
        let settings = gio::Settings::new(SETTINGS);
        panels.place(Placement::from_settings(&settings));
        settings.connect_changed(
            Some(QUEUE_SIDE_PANEL),
            clone!(
                #[weak]
                panels,
                move |settings, _| panels.place(Placement::from_settings(settings))
            ),
        );

        let manager = Self {
            now,
            fullscreen,
            mini,
            queue,
            lyrics,
            panels,
            app_model,
            _settings: settings,
        };
        manager.wire_toggle(window, dispatcher);
        manager
    }

    fn wire_toggle(&self, window: &libadwaita::ApplicationWindow, dispatcher: Dispatcher) {
        let action = self.panels.action.clone();
        let panels = Rc::downgrade(&self.panels);
        action.connect_change_state(move |action, state| {
            if let (Some(panels), Some(open)) = (panels.upgrade(), state.and_then(|s| s.get())) {
                panels.set_open(open);
                let actual = panels.is_open();
                if action.state().and_then(|s| s.get::<bool>()) != Some(actual) {
                    action.set_state(&actual.to_variant());
                }
            }
        });
        window.add_action(&action);

        let show_queue = gio::SimpleAction::new(SHOW_QUEUE_ACTION, None);
        let panels_weak = Rc::downgrade(&self.panels);
        show_queue.connect_activate(move |_, _| {
            if let Some(panels) = panels_weak.upgrade() {
                panels.toggle_view("queue");
            }
        });
        window.add_action(&show_queue);

        let show_lyrics = gio::SimpleAction::new(SHOW_LYRICS_ACTION, None);
        let panels_weak = Rc::downgrade(&self.panels);
        show_lyrics.connect_activate(move |_, _| {
            if let Some(panels) = panels_weak.upgrade() {
                panels.toggle_view("lyrics");
            }
        });
        window.add_action(&show_lyrics);

        let show_now = gio::SimpleAction::new(SHOW_NOW_PLAYING_ACTION, None);
        let panels_weak = Rc::downgrade(&self.panels);
        show_now.connect_activate(move |_, _| {
            if let Some(panels) = panels_weak.upgrade() {
                panels.toggle_view("now");
            }
        });
        window.add_action(&show_now);

        let panels = Rc::downgrade(&self.panels);
        let app_model = self.app_model.clone();
        let open_changed = Rc::new(move || {
            let Some(panels) = panels.upgrade() else {
                return;
            };
            let actual = panels.is_open();
            if action.state().and_then(|s| s.get::<bool>()) != Some(actual) {
                action.set_state(&actual.to_variant());
            }
            panels.sync_header();
            if !panels.is_open() && app_model.get_state().selection.is_selecting_in(true) {
                dispatcher.dispatch(AppAction::CancelSelection);
            }
        });
        self.panels.panel.connect_open_notify(clone!(
            #[strong]
            open_changed,
            move || open_changed()
        ));
        self.panels
            .sheet
            .connect_open_notify(move || open_changed());
    }
}

impl PanelManager {
    fn sync_selection(&self) {
        let (selected, all_selected) = self.queue.selection();
        match self.panels.placement.get() {
            Some(Placement::Side) => self.panels.panel.set_selection(selected, all_selected),
            Some(Placement::Sheet) => self.panels.sheet.set_selecting(selected.is_some()),
            None => {}
        }
    }
}

impl Component for PanelManager {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.queue.get_root_widget()
    }
}

impl EventListener for PanelManager {
    fn on_event(&mut self, event: &AppEvent) {
        self.now.on_event(event);
        self.fullscreen.on_event(event);
        self.mini.on_event(event);
        self.queue.on_event(event);
        self.lyrics.on_event(event);
        if let AppEvent::SelectionEvent(_)
        | AppEvent::PlaybackEvent(PlaybackEvent::PlaylistChanged) = event
        {
            self.sync_selection();
        }
        if let AppEvent::PlaybackEvent(
            PlaybackEvent::PlaybackPaused
            | PlaybackEvent::PlaybackResumed
            | PlaybackEvent::PlaybackStopped
            | PlaybackEvent::TrackChanged(_),
        ) = event
        {
            let playing = self.app_model.get_state().playback.is_playing();
            self.panels.panel.set_now_playing(playing);
        }
    }
}
