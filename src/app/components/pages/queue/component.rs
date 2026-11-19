use gettextrs::{gettext, ngettext};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::{QueuePageWidget, RecentRow, RECENT_COVER_PX};
use crate::app::components::{
    dispatch_api_read_many, labels, play_in_album_actions, Component, EventListener, QueueList,
    QueueListModel,
};
use crate::app::models::{Track, TrackExt};
use crate::app::state::PlaybackEvent;
use crate::app::{load, AppEvent, AppModel, Dispatcher};
use canto_api::models::is_resource_url;

/// How many songs Recently Played lists (Spotify keeps 50).
const RECENT_LIMIT: usize = 50;
/// Spotify writes a song into its history a moment after it ends.
const RECENT_SETTLE: Duration = Duration::from_secs(5);
/// Unfolding again soon after a load keeps what is there.
const RECENT_FRESH: Duration = Duration::from_secs(20);

pub struct QueuePage {
    widget: QueuePageWidget,
    queue_list: QueueList,
    recent: Rc<RecentlyPlayed>,
}

impl QueuePage {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        let queue_model = QueueListModel::new(app_model.clone(), dispatcher.clone(), true);
        let queue_list = QueueList::new(Rc::new(queue_model));
        let widget = QueuePageWidget::default();
        widget.set_queue_list(queue_list.widget());
        queue_list.connect_scrolling();
        widget.set_queue_empty(!queue_list.model().has_queue());
        widget.connect_selection_cancel(clone!(
            #[weak(rename_to = model)]
            queue_list.model(),
            move || model.cancel_selection()
        ));
        widget.connect_select_all(clone!(
            #[weak(rename_to = model)]
            queue_list.model(),
            move || model.toggle_select_all()
        ));
        let recent = RecentlyPlayed::new(widget.clone(), app_model, dispatcher);
        Self {
            widget,
            queue_list,
            recent,
        }
    }

    pub fn model(&self) -> &Rc<QueueListModel> {
        self.queue_list.model()
    }

    pub fn selection(&self) -> (Option<usize>, bool) {
        let model = self.queue_list.model();
        let selected = model.is_selecting().then(|| model.selected_count());
        (selected, model.all_selected())
    }

    fn update_selection(&self) {
        let (selected, all_selected) = self.selection();
        self.widget.set_selection(selected, all_selected);
    }

    pub fn widget(&self) -> &QueuePageWidget {
        &self.widget
    }

    #[allow(dead_code)]
    pub fn title() -> String {
        // translators: Title of the queue, listing what plays next, when it's shown beside the content.
        gettext("Queue")
    }
}

impl Component for QueuePage {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.widget.upcast_ref()
    }
}

impl EventListener for QueuePage {
    fn on_event(&mut self, event: &AppEvent) {
        self.queue_list.on_event(event);
        if let AppEvent::SelectionEvent(_) = event {
            self.update_selection();
        }
        if let AppEvent::PlaybackEvent(
            PlaybackEvent::TrackChanged(_)
            | PlaybackEvent::TrackSynced
            | PlaybackEvent::PlaybackStopped
            | PlaybackEvent::PlaylistChanged
            | PlaybackEvent::SourceChanged,
        ) = event
        {
            self.widget
                .set_queue_empty(!self.queue_list.model().has_queue());
            self.update_selection();
        }
        if let AppEvent::PlaybackEvent(PlaybackEvent::TrackChanged(_)) = event {
            self.recent.track_changed();
        }
    }
}

/// The songs played lately, folded above the queue. Loaded only while
/// unfolded: when it opens, and again when the song changes.
struct RecentlyPlayed {
    widget: QueuePageWidget,
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    tracks: RefCell<Vec<Track>>,
    /// Bumped by each load; an older answer is dropped.
    generation: Cell<u32>,
    loaded_at: Cell<Option<Instant>>,
}

impl RecentlyPlayed {
    fn new(widget: QueuePageWidget, app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Rc<Self> {
        let recent = Rc::new(Self {
            widget: widget.clone(),
            app_model,
            dispatcher,
            tracks: Default::default(),
            generation: Default::default(),
            loaded_at: Default::default(),
        });
        let weak = Rc::downgrade(&recent);
        widget.connect_recent_toggled(move |expanded| {
            if let Some(recent) = weak.upgrade() {
                if expanded {
                    recent.load(false);
                } else {
                    // Folded: an answer on its way is no longer wanted
                    recent
                        .generation
                        .set(recent.generation.get().wrapping_add(1));
                }
            }
        });
        let weak = Rc::downgrade(&recent);
        widget.connect_recent_activated(move |index| {
            if let Some(recent) = weak.upgrade() {
                recent.play(index);
            }
        });
        recent
    }

    fn track_changed(self: &Rc<Self>) {
        // Folded: nothing to fetch; the next unfold loads it fresh
        self.loaded_at.set(None);
        if self.widget.is_recent_expanded() {
            self.load(true);
        }
    }

    fn load(self: &Rc<Self>, settle: bool) {
        let fresh = self
            .loaded_at
            .get()
            .is_some_and(|at| at.elapsed() < RECENT_FRESH);
        if fresh && !settle {
            return;
        }
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        // Keep the old list in view while a newer one loads
        if self.tracks.borrow().is_empty() {
            self.widget.show_recent_loading();
        }
        let api = self.app_model.api();
        let weak = Rc::downgrade(self);
        glib::MainContext::default().spawn_local(async move {
            if settle {
                glib::timeout_future(RECENT_SETTLE).await;
                let wanted = weak.upgrade().is_some_and(|recent| {
                    recent.generation.get() == generation && recent.widget.is_recent_expanded()
                });
                if !wanted {
                    return;
                }
            }
            let result = api.get_recently_played(RECENT_LIMIT, load::visible()).await;
            let Some(recent) = weak.upgrade() else {
                return;
            };
            if recent.generation.get() != generation {
                return;
            }
            match result {
                Ok(played) => {
                    recent.loaded_at.set(Some(Instant::now()));
                    recent.show(played);
                }
                Err(err) => {
                    log::debug!("recently played failed: {err:?}");
                    if recent.tracks.borrow().is_empty() {
                        // translators: Shown in Recently Played, above the queue, when Spotify didn't answer.
                        let text = gettext("Couldn't load your recent songs.");
                        recent.widget.show_recent_message(&text);
                    }
                }
            }
        });
    }

    fn show(&self, played: Vec<canto_api::models::PlayedTrack>) {
        if played.is_empty() {
            self.tracks.borrow_mut().clear();
            // translators: Shown in Recently Played, above the queue, when the account has no history yet.
            let text = gettext("Songs you play will show up here.");
            self.widget.show_recent_message(&text);
            return;
        }
        let now = glib::DateTime::now_utc().map(|d| d.to_unix()).unwrap_or(0);
        let rows: Vec<RecentRow> = played
            .iter()
            .map(|p| RecentRow {
                title: p.track.title.clone(),
                subtitle: recent_subtitle(
                    &p.track.artists_name(),
                    p.played_at
                        .as_deref()
                        .and_then(|at| glib::DateTime::from_iso8601(at, None).ok())
                        .map(|at| now - at.to_unix()),
                ),
            })
            .collect();
        let covers = self.widget.show_recent_rows(&rows);
        for (cover, p) in covers.iter().zip(&played) {
            self.load_cover(
                cover,
                p.track.art.best_for_width(RECENT_COVER_PX as u32 * 2),
            );
        }
        *self.tracks.borrow_mut() = played.into_iter().map(|p| p.track).collect();
    }

    fn load_cover(&self, image: &gtk::Image, url: Option<&str>) {
        let Some(url) = url.filter(|url| !is_resource_url(url)).map(str::to_owned) else {
            return;
        };
        let api = self.app_model.api();
        let weak = image.downgrade();
        let px = RECENT_COVER_PX * 2;
        let tag = load::visible();
        glib::MainContext::default().spawn_local(async move {
            let texture = api.load_image(&url, px, px, tag).await;
            if let (Some(image), Some(texture)) = (weak.upgrade(), texture) {
                image.set_paintable(Some(&texture));
            }
        });
    }

    /// Plays the song from its album, as picking it anywhere else does.
    fn play(&self, index: usize) {
        let Some(track) = self.tracks.borrow().get(index).cloned() else {
            return;
        };
        let api = self.app_model.api();
        dispatch_api_read_many(&self.dispatcher, move |tag| async move {
            play_in_album_actions(&api, track, tag).await
        });
    }
}

/// "Artists · 5 min ago"; just the artists when the time is unknown.
fn recent_subtitle(artists: &str, seconds_ago: Option<i64>) -> String {
    match seconds_ago.map(time_ago) {
        Some(ago) => format!("{artists} · {ago}"),
        None => artists.to_string(),
    }
}

/// How long ago, in a few words: "Just now", "5 min ago", "3 h ago", "2 days ago".
fn time_ago(seconds: i64) -> String {
    let minutes = seconds.max(0) / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    if minutes < 1 {
        // translators: When a song in Recently Played was played: less than a minute ago.
        gettext("Just now")
    } else if hours < 1 {
        // translators: When a song in Recently Played was played, in minutes. Keep it short.
        labels::fill(
            ngettext("{} min ago", "{} min ago", minutes as u32),
            &[minutes.to_string().as_str()],
        )
    } else if days < 1 {
        // translators: When a song in Recently Played was played, in hours. Keep it short.
        labels::fill(
            ngettext("{} h ago", "{} h ago", hours as u32),
            &[hours.to_string().as_str()],
        )
    } else {
        // translators: When a song in Recently Played was played, in days.
        labels::fill(
            ngettext("{} day ago", "{} days ago", days as u32),
            &[days.to_string().as_str()],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_ago_rounds_down_to_the_largest_unit() {
        assert_eq!(time_ago(-5), "Just now");
        assert_eq!(time_ago(59), "Just now");
        assert_eq!(time_ago(60), "1 min ago");
        assert_eq!(time_ago(59 * 60 + 59), "59 min ago");
        assert_eq!(time_ago(3600), "1 h ago");
        assert_eq!(time_ago(23 * 3600 + 3599), "23 h ago");
        assert_eq!(time_ago(24 * 3600), "1 day ago");
        assert_eq!(time_ago(3 * 24 * 3600), "3 days ago");
    }

    #[test]
    fn subtitle_leaves_out_an_unknown_time() {
        assert_eq!(recent_subtitle("Niink", Some(300)), "Niink · 5 min ago");
        assert_eq!(recent_subtitle("Niink, Jotapê", None), "Niink, Jotapê");
    }
}
