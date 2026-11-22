use gettextrs::{gettext, ngettext};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::{QueuePageWidget, RecentRow, RECENT_COVER_PX};
use crate::app::components::now_panel::toggle_track_like;
use crate::app::components::{
    dispatch_api_call_many, dispatch_api_read_many, labels, play_in_album_actions, Component,
    EventListener, PlaylistDialog, QueueList, QueueListModel, Saved,
};
use crate::app::models::{Track, TrackExt};
use crate::app::state::{liked_statuses, PlaybackEvent};
use crate::app::{load, AppAction, AppEvent, AppModel, BrowserAction, BrowserEvent, Dispatcher};
use canto_api::models::is_resource_url;
use libadwaita::prelude::AdwDialogExt;

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
        widget.connect_save_playlist(clone!(
            #[weak(rename_to = model)]
            queue_list.model(),
            #[weak]
            app_model,
            #[strong]
            dispatcher,
            move |button| {
                save_queue_as_playlist(button, &model, &app_model, &dispatcher);
            }
        ));
        let recent = RecentlyPlayed::new(widget.clone(), app_model, dispatcher);
        let page = Self {
            widget,
            queue_list,
            recent,
        };
        page.update_save_playlist();
        page
    }

    fn update_save_playlist(&self) {
        let model = self.queue_list.model();
        let visible = model.can_save_as_playlist() && !model.is_selecting();
        self.widget.set_save_playlist_visible(visible);
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
            self.update_save_playlist();
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
            self.update_save_playlist();
        }
        if let AppEvent::LoginEvent(_) = event {
            self.update_save_playlist();
        }
        if let AppEvent::PlaybackEvent(ev) = event {
            match ev {
                PlaybackEvent::TrackChanged(_)
                | PlaybackEvent::TrackSynced
                | PlaybackEvent::SourceChanged
                | PlaybackEvent::PlaybackResumed
                | PlaybackEvent::PlayStateSynced(true)
                | PlaybackEvent::PlaybackStopped => {
                    self.recent.on_playback_event(ev);
                }
                _ => {}
            }
        }
        if let AppEvent::BrowserEvent(BrowserEvent::SavedTracksUpdated) = event {
            self.recent.refresh_likes();
        }
    }
}

fn save_queue_as_playlist(
    button: &gtk::Button,
    model: &Rc<QueueListModel>,
    app_model: &Rc<AppModel>,
    dispatcher: &Dispatcher,
) {
    let dialog = PlaylistDialog::for_saving_queue(&gettext("Queue"));
    let weak_model = Rc::downgrade(model);
    let weak_app = Rc::downgrade(app_model);
    let dispatcher = dispatcher.clone();
    dialog.connect_save(move |form| {
        let (saved, saved_rx) = Saved::channel();
        let (Some(model), Some(app_model)) = (weak_model.upgrade(), weak_app.upgrade()) else {
            return saved_rx;
        };
        let Some(user_id) = app_model.get_state().logged_user.user.clone() else {
            return saved_rx;
        };
        let uris = model.playable_uris();
        if uris.is_empty() {
            return saved_rx;
        }
        let api = app_model.api();
        let cover_dispatcher = dispatcher.clone();
        let load = load::visible();
        dispatch_api_call_many(&dispatcher, move || async move {
            let playlist = api
                .create_playlist(
                    &user_id,
                    &form.name,
                    form.description.as_deref(),
                    form.public,
                )
                .await?;
            for chunk in uris.chunks(100) {
                api.add_to_playlist(&playlist.rri.id, chunk.to_vec(), load)
                    .await?;
            }
            saved.tell();
            let id = playlist.rri.id.clone();
            if let Some(cover) = form.cover {
                crate::app::components::upload_playlist_cover(&cover_dispatcher, api, id, cover);
            }
            Ok(vec![
                AppAction::CreatePlaylist(playlist),
                // translators: Toast shown after saving the current play queue as a new playlist.
                AppAction::ShowNotification(gettext("Queue saved as playlist")),
            ])
        });
        saved_rx
    });
    dialog.present(Some(button));
}

/// The songs played lately, folded above the queue. Loaded only while
/// unfolded: when it opens, and again when the song changes.
struct RecentlyPlayed {
    widget: QueuePageWidget,
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    tracks: RefCell<Vec<Track>>,
    session_recent: RefCell<Vec<canto_api::models::PlayedTrack>>,
    last_remote: RefCell<Vec<canto_api::models::PlayedTrack>>,
    active_track: RefCell<Option<Track>>,
    /// Bumped by each load; an older answer is dropped.
    generation: Cell<u32>,
    loaded_at: Cell<Option<Instant>>,
}

impl RecentlyPlayed {
    fn new(widget: QueuePageWidget, app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Rc<Self> {
        let initial_active = {
            let state = app_model.get_state();
            if state.playback.is_playing() {
                state.playback.current_song()
            } else {
                None
            }
        };
        let recent = Rc::new(Self {
            widget: widget.clone(),
            app_model,
            dispatcher,
            tracks: Default::default(),
            session_recent: Default::default(),
            last_remote: Default::default(),
            active_track: RefCell::new(initial_active),
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
        let weak = Rc::downgrade(&recent);
        widget.connect_recent_like(move |index| {
            if let Some(recent) = weak.upgrade() {
                recent.toggle_like(index);
            }
        });
        recent
    }

    fn record_session_track(&self, track: Track) {
        let played_at = glib::DateTime::now_utc()
            .and_then(|d| d.format_iso8601())
            .ok()
            .map(|s| s.to_string());
        let mut session = self.session_recent.borrow_mut();
        session.insert(0, canto_api::models::PlayedTrack { track, played_at });
        session.truncate(RECENT_LIMIT);
    }

    fn on_playback_event(self: &Rc<Self>, event: &PlaybackEvent) {
        let recorded = match event {
            PlaybackEvent::PlaybackStopped => {
                if let Some(prev) = self.active_track.borrow_mut().take() {
                    self.record_session_track(prev);
                    true
                } else {
                    false
                }
            }
            PlaybackEvent::TrackChanged(_)
            | PlaybackEvent::TrackSynced
            | PlaybackEvent::SourceChanged
            | PlaybackEvent::PlaybackResumed
            | PlaybackEvent::PlayStateSynced(true) => {
                let (current, is_playing) = {
                    let state = self.app_model.get_state();
                    (state.playback.current_song(), state.playback.is_playing())
                };
                let should_track = is_playing
                    || matches!(
                        event,
                        PlaybackEvent::TrackChanged(_) | PlaybackEvent::SourceChanged
                    );
                let mut recorded = false;
                if should_track {
                    let mut active = self.active_track.borrow_mut();
                    let next_id = current.as_ref().map(|t| t.rri.id.as_str());
                    if let Some(prev) = active.as_ref() {
                        if next_id != Some(prev.rri.id.as_str()) {
                            self.record_session_track(prev.clone());
                            recorded = true;
                        }
                    }
                    *active = current;
                }
                recorded
            }
            _ => false,
        };

        if matches!(
            event,
            PlaybackEvent::TrackChanged(_)
                | PlaybackEvent::TrackSynced
                | PlaybackEvent::SourceChanged
                | PlaybackEvent::PlaybackStopped
        ) || recorded
        {
            self.loaded_at.set(None);
            if self.widget.is_recent_expanded() {
                if recorded {
                    let immediate = merge_recent_tracks(
                        &self.session_recent.borrow(),
                        &self.last_remote.borrow(),
                        RECENT_LIMIT,
                    );
                    if !immediate.is_empty() {
                        self.show(immediate);
                    }
                }
                if !matches!(event, PlaybackEvent::PlaybackStopped) {
                    self.load(true);
                }
            }
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
        let immediate = merge_recent_tracks(
            &self.session_recent.borrow(),
            &self.last_remote.borrow(),
            RECENT_LIMIT,
        );
        if !immediate.is_empty() {
            self.show(immediate);
        } else if self.tracks.borrow().is_empty() {
            // Keep the old list in view while a newer one loads
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
                    *recent.last_remote.borrow_mut() = played.clone();
                    recent.loaded_at.set(Some(Instant::now()));
                    let merged =
                        merge_recent_tracks(&recent.session_recent.borrow(), &played, RECENT_LIMIT);
                    recent.show(merged);
                }
                Err(err) => {
                    log::debug!("recently played failed: {err:?}");
                    let fallback = merge_recent_tracks(
                        &recent.session_recent.borrow(),
                        &recent.last_remote.borrow(),
                        RECENT_LIMIT,
                    );
                    if !fallback.is_empty() {
                        recent.show(fallback);
                    } else if recent.tracks.borrow().is_empty() {
                        // translators: Shown in Recently Played, above the queue, when Spotify didn't answer.
                        let text = gettext("Couldn't load your recent songs.");
                        recent.widget.show_recent_message(&text);
                    }
                }
            }
        });
    }

    fn show(&self, mut played: Vec<canto_api::models::PlayedTrack>) {
        // Spotify answers newest first; the list reads like a timeline that
        // flows into Now Playing, so the newest goes last
        played.reverse();
        if played.is_empty() {
            self.tracks.borrow_mut().clear();
            // translators: Shown in Recently Played, above the queue, when the account has no history yet.
            let text = gettext("Songs you play will show up here.");
            self.widget.show_recent_message(&text);
            return;
        }
        // Spotify said which of these are saved: the rest of the app learns
        // it too (a later like or unlike elsewhere is not overridden)
        let tracks: Vec<Track> = played.iter().map(|p| p.track.clone()).collect();
        let statuses: Vec<(String, bool)> = liked_statuses(&tracks).collect();
        if !statuses.is_empty() {
            self.dispatcher
                .dispatch(BrowserAction::SetTracksLikedStatus(statuses).into());
        }
        let now = glib::DateTime::now_utc().map(|d| d.to_unix()).unwrap_or(0);
        let rows: Vec<RecentRow> = played
            .iter()
            .map(|p| RecentRow {
                liked: p
                    .track
                    .saved
                    .unwrap_or_else(|| self.is_liked(&p.track.rri.id)),
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

    /// Whether the app knows the song as saved.
    fn is_liked(&self, id: &str) -> bool {
        self.app_model
            .get_state()
            .browser
            .home_state()
            .is_some_and(|home| home.is_track_liked(id))
    }

    /// The hearts follow a like or unlike made anywhere (the same song twice
    /// in the list changes twice).
    fn refresh_likes(&self) {
        let liked: Vec<bool> = self
            .tracks
            .borrow()
            .iter()
            .map(|track| self.is_liked(&track.rri.id))
            .collect();
        self.widget.set_recent_liked(&liked);
    }

    /// Saves the song, or removes it from the saved tracks.
    fn toggle_like(&self, index: usize) {
        let Some(track) = self.tracks.borrow().get(index).cloned() else {
            return;
        };
        let liked = self.is_liked(&track.rri.id);
        toggle_track_like(&self.app_model, &self.dispatcher, track, liked);
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

/// Merge session-tracked plays (including skipped tracks and non-Spotify
/// sources) with remote history, keeping newest-first order and deduplicating
/// overlapping remote entries for the same track within 3 minutes.
fn merge_recent_tracks(
    session: &[canto_api::models::PlayedTrack],
    remote: &[canto_api::models::PlayedTrack],
    limit: usize,
) -> Vec<canto_api::models::PlayedTrack> {
    let parse_unix = |at: Option<&str>| -> Option<i64> {
        at.and_then(|s| glib::DateTime::from_iso8601(s, None).ok())
            .map(|dt| dt.to_unix())
    };
    const OVERLAP_SECS: i64 = 180;

    let mut merged: Vec<canto_api::models::PlayedTrack> = session.iter().cloned().collect();
    for item in remote {
        let item_unix = parse_unix(item.played_at.as_deref());
        let overlaps_session = session.iter().any(|s| {
            if s.track.rri.id != item.track.rri.id {
                return false;
            }
            match (parse_unix(s.played_at.as_deref()), item_unix) {
                (Some(su), Some(ru)) => (su - ru).abs() <= OVERLAP_SECS,
                _ => true,
            }
        });
        if !overlaps_session {
            merged.push(item.clone());
        }
    }

    merged.sort_by(|a, b| {
        match (
            parse_unix(a.played_at.as_deref()),
            parse_unix(b.played_at.as_deref()),
        ) {
            (Some(au), Some(bu)) => bu.cmp(&au),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });
    merged.truncate(limit);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use canto_api::models::{PlayedTrack, Provider, ResourceId};

    fn played(id: &str, played_at: Option<&str>) -> PlayedTrack {
        PlayedTrack {
            track: Track {
                rri: ResourceId::new(Provider::Local, id),
                title: id.to_string(),
                artists: vec![],
                album: None,
                duration_ms: 180_000,
                track_number: None,
                disc_number: None,
                content_rating: canto_api::models::ContentRating::None,
                isrc: None,
                art: canto_api::models::ImageSet::default(),
                playable: true,
                popularity: None,
                saved: None,
                preview_url: None,
                url: None,
            },
            played_at: played_at.map(str::to_string),
        }
    }

    #[test]
    fn merge_recent_tracks_keeps_session_tracks_when_remote_empty() {
        let session = vec![
            played("b", Some("2026-11-22T10:02:00Z")),
            played("a", Some("2026-11-22T10:01:00Z")),
        ];
        let merged = merge_recent_tracks(&session, &[], 50);
        let ids: Vec<&str> = merged.iter().map(|p| p.track.rri.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "a"]);
    }

    #[test]
    fn merge_recent_tracks_deduplicates_remote_overlap_and_orders_newest_first() {
        let session = vec![
            played("c", Some("2026-11-22T10:05:00Z")),
            played("b", Some("2026-11-22T10:03:00Z")),
        ];
        let remote = vec![
            // Duplicate of "b" reported ~20s earlier by Spotify
            played("b", Some("2026-11-22T10:02:40Z")),
            // Earlier play of "a" from before session
            played("a", Some("2026-11-22T09:50:00Z")),
            // Distinct much earlier play of "b" (2 hours earlier) stays
            played("b", Some("2026-11-22T08:00:00Z")),
        ];
        let merged = merge_recent_tracks(&session, &remote, 3);
        let ids: Vec<&str> = merged.iter().map(|p| p.track.rri.id.as_str()).collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
    }

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
