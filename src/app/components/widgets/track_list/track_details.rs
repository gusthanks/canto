//! Detailed metadata, Genius credits/story, and quick actions dialog for any track.

use std::rc::Rc;

use canto_api::models::{is_resource_url, ContentRating, Provider};
use canto_api::scrobbler::LastFmClient;
use canto_config::settings::{GENIUS_ACCESS_TOKEN, LASTFM_API_KEY, LASTFM_USERNAME, SETTINGS};
use gettextrs::gettext;
use gio::prelude::*;
use gtk::prelude::*;
use gtk::{glib, pango};
use libadwaita::prelude::*;

use crate::app::components::labels;
use crate::app::components::utils::{decode_px, format_duration};
use crate::app::models::{Track, TrackExt};
use crate::app::{load, AppAction, AppModel, Dispatcher};

const COVER_SIZE_PX: i32 = 88;

fn format_count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        let rem = digits.len() - i;
        if i > 0 && rem % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn provider_label(provider: &Provider) -> String {
    match provider {
        Provider::Spotify => "Spotify".to_string(),
        // translators: Music source label in the Track Details dialog for local audio files.
        Provider::Local => gettext("Local Files"),
        Provider::Subsonic => "OpenSubsonic / Navidrome".to_string(),
        Provider::YouTubeMusic => "YouTube Music".to_string(),
        Provider::AppleMusic => "Apple Music".to_string(),
        Provider::Tidal => "TIDAL".to_string(),
        Provider::Other(name) if !name.is_empty() => name.clone(),
        // translators: Fallback source label in the Track Details dialog when the provider is unknown.
        _ => gettext("Unknown"),
    }
}

fn format_track_position(disc: Option<u32>, track: Option<u32>) -> Option<String> {
    match (disc, track) {
        (Some(d), Some(t)) => Some(labels::fill(
            // translators: Position of a song on a multi-disc album, e.g. "Disc 1 · Track 3".
            gettext("Disc {} · Track {}"),
            &[&d.to_string(), &t.to_string()],
        )),
        (None, Some(t)) => Some(labels::fill(
            // translators: Position of a song on an album, e.g. "Track 3".
            gettext("Track {}"),
            &[&t.to_string()],
        )),
        (Some(d), None) => Some(labels::fill(
            // translators: Disc number when the track number is absent, e.g. "Disc 2".
            gettext("Disc {}"),
            &[&d.to_string()],
        )),
        (None, None) => None,
    }
}

fn shareable_track_link(song: &Track) -> Option<String> {
    song.url.clone().or_else(|| {
        (song.rri.provider == Provider::Spotify && !song.rri.id.is_empty())
            .then(|| format!("https://open.spotify.com/track/{}", song.rri.id))
    })
}

fn add_property_row(group: &libadwaita::PreferencesGroup, title: &str, value: &str) {
    if value.trim().is_empty() {
        return;
    }
    let row = libadwaita::ActionRow::builder()
        .title(glib::markup_escape_text(title))
        .subtitle(glib::markup_escape_text(value))
        .subtitle_selectable(true)
        .css_classes(["property"])
        .build();
    group.add(&row);
}

/// Presents a dialog showing metadata, quick navigation/link actions, and
/// asynchronous Genius credits & story for `song`.
pub fn present_track_details(
    parent: Option<&gtk::Widget>,
    song: Track,
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
) -> libadwaita::Dialog {
    let dialog = libadwaita::Dialog::builder()
        .title(&*labels::TRACK_DETAILS)
        .content_width(460)
        .content_height(600)
        .css_classes(["track-details-dialog"])
        .build();

    let toolbar = libadwaita::ToolbarView::new();
    toolbar.add_top_bar(&libadwaita::HeaderBar::new());

    let root_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(12)
        .margin_bottom(24)
        .margin_start(18)
        .margin_end(18)
        .build();

    // Header: square cover + selectable title/artists + quick actions.
    let header_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(16)
        .build();

    let cover_frame = gtk::Box::builder()
        .overflow(gtk::Overflow::Hidden)
        .valign(gtk::Align::Center)
        .width_request(COVER_SIZE_PX)
        .height_request(COVER_SIZE_PX)
        .css_classes(["track-details__cover", "card"])
        .build();
    let cover_image = gtk::Image::builder()
        .icon_name("audio-x-generic-symbolic")
        .pixel_size(36)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .vexpand(true)
        .build();
    cover_frame.append(&cover_image);
    header_box.append(&cover_frame);

    if let Some(url) = song
        .art
        .best_for_width((COVER_SIZE_PX * 2) as u32)
        .filter(|u| !is_resource_url(u))
        .map(str::to_owned)
    {
        let api = app_model.api();
        let img_weak = cover_image.downgrade();
        let px = decode_px(COVER_SIZE_PX);
        glib::MainContext::default().spawn_local(async move {
            let texture = api.load_image(&url, px, px, load::visible()).await;
            if let (Some(img), Some(tex)) = (img_weak.upgrade(), texture) {
                img.set_pixel_size(COVER_SIZE_PX);
                img.set_paintable(Some(&tex));
            }
        });
    }

    let summary_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();

    let title_label = gtk::Label::builder()
        .label(&song.title)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(pango::WrapMode::WordChar)
        .selectable(true)
        .css_classes(["title-3"])
        .build();
    summary_box.append(&title_label);

    let artists = song.artists_name();
    if !artists.is_empty() || song.is_explicit() {
        let artist_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .valign(gtk::Align::Center)
            .build();
        if song.is_explicit() {
            let badge = gtk::Label::builder()
                .label("E")
                .valign(gtk::Align::Center)
                .tooltip_text(gettext("Explicit"))
                .css_classes(["explicit-badge"])
                .build();
            artist_row.append(&badge);
        }
        if !artists.is_empty() {
            let artists_label = gtk::Label::builder()
                .label(&artists)
                .xalign(0.0)
                .wrap(true)
                .wrap_mode(pango::WrapMode::WordChar)
                .selectable(true)
                .hexpand(true)
                .css_classes(["dim-label"])
                .build();
            artist_row.append(&artists_label);
        }
        summary_box.append(&artist_row);
    }

    let action_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_top(6)
        .build();

    if let Some(ref album) = song.album {
        let album_btn_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        album_btn_content.append(&gtk::Image::from_icon_name(
            "media-optical-cd-audio-symbolic",
        ));
        album_btn_content.append(&gtk::Label::new(Some(&*labels::VIEW_ALBUM)));
        let album_btn = gtk::Button::builder()
            .child(&album_btn_content)
            .css_classes(["pill"])
            .build();
        let album_id = album.rri.id.clone();
        let disp = dispatcher.clone();
        let dialog_weak = dialog.downgrade();
        album_btn.connect_clicked(move |_| {
            disp.dispatch(AppAction::ViewAlbum(album_id.clone()));
            if let Some(d) = dialog_weak.upgrade() {
                d.close();
            }
        });
        action_row.append(&album_btn);
    }

    if let Some(link) = shareable_track_link(&song) {
        let copy_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        copy_content.append(&gtk::Image::from_icon_name("edit-copy-symbolic"));
        copy_content.append(&gtk::Label::new(Some(&*labels::COPY_LINK)));
        let copy_btn = gtk::Button::builder()
            .child(&copy_content)
            .css_classes(["pill"])
            .build();
        copy_btn.connect_clicked(move |_| {
            crate::app::components::copy_link_to_clipboard(&link);
        });
        action_row.append(&copy_btn);
    }

    if action_row.first_child().is_some() {
        summary_box.append(&action_row);
    }
    header_box.append(&summary_box);
    root_box.append(&header_box);

    // Track metadata card.
    let info_group = libadwaita::PreferencesGroup::builder()
        // translators: Section title in the Track Details dialog for core track metadata.
        .title(gettext("Track Info"))
        .build();

    if song.duration_ms > 0 {
        // translators: Row title for track duration in the Track Details dialog.
        add_property_row(
            &info_group,
            &gettext("Duration"),
            &format_duration(song.duration_ms as f64),
        );
    }
    if let Some(ref album) = song.album {
        // translators: Row title for album name in the Track Details dialog.
        add_property_row(&info_group, &gettext("Album"), &album.name);
    }
    if let Some(pos) = format_track_position(song.disc_number, song.track_number) {
        // translators: Row title for disc and track number in the Track Details dialog.
        add_property_row(&info_group, &gettext("Track"), &pos);
    }
    if song.content_rating == ContentRating::Explicit {
        // translators: Row title and value when a song is marked explicit.
        add_property_row(&info_group, &gettext("Content"), &gettext("Explicit"));
    }
    if let Some(popularity) = song.popularity {
        // translators: Row title for popularity score in the Track Details dialog.
        add_property_row(
            &info_group,
            &gettext("Popularity"),
            &format!("{popularity} / 100"),
        );
    }
    if let Some(ref isrc) = song.isrc {
        add_property_row(&info_group, "ISRC", isrc);
    }
    // translators: Row title for music provider/source in the Track Details dialog.
    add_property_row(
        &info_group,
        &gettext("Source"),
        &provider_label(&song.rri.provider),
    );
    root_box.append(&info_group);

    // Album tracklists return simplified Spotify tracks without popularity/ISRC:
    // fetch the full track lazily so opening Track Details from an album still shows them.
    let has_tokio = tokio::runtime::Handle::try_current().is_ok();
    if has_tokio
        && song.rri.provider == Provider::Spotify
        && !song.rri.id.is_empty()
        && (song.popularity.is_none() || song.isrc.is_none())
    {
        let api = app_model.api();
        let track_id = song.rri.id.clone();
        let need_popularity = song.popularity.is_none();
        let need_isrc = song.isrc.is_none();
        let info_weak = info_group.downgrade();
        glib::MainContext::default().spawn_local(async move {
            let Ok(full) = api.get_track(&track_id, load::visible()).await else {
                return;
            };
            let Some(group) = info_weak.upgrade() else {
                return;
            };
            if need_popularity {
                if let Some(popularity) = full.popularity {
                    add_property_row(
                        &group,
                        &gettext("Popularity"),
                        &format!("{popularity} / 100"),
                    );
                }
            }
            if need_isrc {
                if let Some(ref isrc) = full.isrc {
                    add_property_row(&group, "ISRC", isrc);
                }
            }
        });
    }

    // Plays & Reach section (Last.fm global plays/listeners/user plays + Genius pageviews).
    let settings = gio::Settings::new(SETTINGS);
    let lastfm_key = settings.string(LASTFM_API_KEY).trim().to_string();
    let lastfm_user = settings.string(LASTFM_USERNAME).trim().to_string();
    let primary_artist = song
        .artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| artists.clone());
    let stats_group = libadwaita::PreferencesGroup::builder()
        // translators: Section title in the Track Details dialog for global plays and views.
        .title(gettext("Plays & Reach"))
        .visible(false)
        .build();
    root_box.append(&stats_group);

    if has_tokio && !lastfm_key.is_empty() && !primary_artist.is_empty() {
        let title = song.title.clone();
        let artist = primary_artist.clone();
        let stats_weak = stats_group.downgrade();
        glib::MainContext::default().spawn_local(async move {
            let Ok(stats) =
                LastFmClient::track_stats(&lastfm_key, &lastfm_user, &artist, &title).await
            else {
                return;
            };
            let Some(group) = stats_weak.upgrade() else {
                return;
            };
            let mut added = false;
            if let Some(plays) = stats.plays.filter(|&n| n > 0) {
                // translators: Global scrobble play count from Last.fm in Track Details.
                add_property_row(&group, &gettext("Last.fm Plays"), &format_count(plays));
                added = true;
            }
            if let Some(listeners) = stats.listeners.filter(|&n| n > 0) {
                // translators: Global listener count from Last.fm in Track Details.
                add_property_row(
                    &group,
                    &gettext("Last.fm Listeners"),
                    &format_count(listeners),
                );
                added = true;
            }
            if let Some(user_plays) = stats.user_plays {
                // translators: The user's own play count on Last.fm in Track Details.
                add_property_row(&group, &gettext("Your Plays"), &format_count(user_plays));
                added = true;
            }
            if added {
                group.set_visible(true);
            }
        });
    }

    // Optional Genius credits & story section (loaded asynchronously when token is configured).
    let genius_token = settings.string(GENIUS_ACCESS_TOKEN).trim().to_string();
    if has_tokio && !genius_token.is_empty() && !artists.is_empty() {
        let credits_group = libadwaita::PreferencesGroup::builder()
            // translators: Section title in the Track Details dialog for Genius credits and song bio.
            .title(gettext("Credits & Story"))
            .build();
        let loading_box = gtk::Box::builder()
            .halign(gtk::Align::Center)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        loading_box.append(&libadwaita::Spinner::new());
        let loading_row = libadwaita::PreferencesRow::builder()
            .activatable(false)
            .child(&loading_box)
            .build();
        credits_group.add(&loading_row);
        root_box.append(&credits_group);

        let api = app_model.api();
        let track_id = song.rri.id.clone();
        let title = song.title.clone();
        let performers = artists.clone();
        let group_weak = credits_group.downgrade();
        let stats_weak = stats_group.downgrade();
        let row_weak = loading_row.downgrade();
        glib::MainContext::default().spawn_local(async move {
            let result = api
                .fetch_genius_track_info(&track_id, &performers, &title, &genius_token)
                .await;
            let Some(group) = group_weak.upgrade() else {
                return;
            };
            if let Some(row) = row_weak.upgrade() {
                group.remove(&row);
            }
            let Ok(info) = result else {
                group.set_visible(false);
                return;
            };
            if !info.is_exact_match {
                group.set_visible(false);
                return;
            }
            if let Some(views) = info.stats.pageviews.filter(|&n| n > 0) {
                if let Some(stats_group) = stats_weak.upgrade() {
                    // translators: Song page view count from Genius in Track Details.
                    add_property_row(&stats_group, &gettext("Genius Views"), &format_count(views));
                    stats_group.set_visible(true);
                }
            }
            let mut added = false;
            if let Some(ref release_date) = info.release_date {
                if !release_date.is_empty() {
                    // translators: Release date row in the Track Details dialog.
                    add_property_row(&group, &gettext("Release Date"), release_date);
                    added = true;
                }
            }
            if !info.featured_artists.is_empty() {
                add_property_row(
                    &group,
                    &gettext("Featured Artists"),
                    &info.featured_artists.join(", "),
                );
                added = true;
            }
            if !info.producers.is_empty() {
                add_property_row(&group, &gettext("Producer(s)"), &info.producers.join(", "));
                added = true;
            }
            if !info.writers.is_empty() {
                add_property_row(&group, &gettext("Writer(s)"), &info.writers.join(", "));
                added = true;
            }
            for credit in &info.credits {
                add_property_row(&group, &credit.role, &credit.names.join(", "));
                added = true;
            }
            if let Some(ref place) = info.recording_location {
                if !place.is_empty() {
                    add_property_row(&group, &gettext("Recorded At"), place);
                    added = true;
                }
            }
            if let Some(bio) = info
                .description
                .as_deref()
                .map(str::trim)
                .filter(|b| !b.is_empty())
            {
                let bio_label = gtk::Label::builder()
                    .label(bio)
                    .wrap(true)
                    .wrap_mode(pango::WrapMode::WordChar)
                    .xalign(0.0)
                    .selectable(true)
                    .margin_top(10)
                    .margin_bottom(10)
                    .margin_start(12)
                    .margin_end(12)
                    .css_classes(["body"])
                    .build();
                let bio_row = libadwaita::PreferencesRow::builder()
                    .activatable(false)
                    .child(&bio_label)
                    .build();
                group.add(&bio_row);
                added = true;
            }
            if !info.url.is_empty() {
                let btn = gtk::Button::builder()
                    .label(gettext("Open on Genius"))
                    .halign(gtk::Align::Start)
                    .margin_top(10)
                    .margin_bottom(6)
                    .margin_start(12)
                    .css_classes(["pill"])
                    .build();
                let url = info.url;
                btn.connect_clicked(move |_| {
                    let _ = open::that(&url);
                });
                let btn_row = libadwaita::PreferencesRow::builder()
                    .activatable(false)
                    .child(&btn)
                    .build();
                group.add(&btn_row);
                added = true;
            }
            if !added {
                group.set_visible(false);
            }
        });
    }

    let clamp = libadwaita::Clamp::builder()
        .maximum_size(460)
        .child(&root_box)
        .build();
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build();
    toolbar.set_content(Some(&scroller));
    dialog.set_child(Some(&toolbar));
    dialog.present(parent);
    dialog
}
