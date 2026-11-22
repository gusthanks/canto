//! The side panel's About tab: everything Genius knows about the playing
//! song (its story, every credit, samples and versions, translations) and
//! about its artist.

use gettextrs::gettext;
use gio::prelude::SettingsExt;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::info_page::{
    caption, compact_link_button, flow, folding_text, link_button, row, setup_button, InfoPage,
};
use super::{card, heading, number_columns, wrapping_label};
use crate::app::components::EventListener;
use crate::app::models::TrackExt;
use crate::app::state::PlaybackEvent;
use crate::app::{AppEvent, AppModel};
use canto_api::{GeniusArtistInfo, GeniusBioStatus, GeniusTrackInfo};
use canto_config::settings::{GENIUS_ACCESS_TOKEN, SETTINGS};

const ICON: &str = "canto-genius-symbolic";

pub struct AboutPanel {
    page: Rc<InfoPage>,
    app_model: Rc<AppModel>,
    settings: gio::Settings,
    /// The track shown (or loading)
    loaded_for: Rc<RefCell<Option<String>>>,
}

impl AboutPanel {
    pub fn new(app_model: Rc<AppModel>) -> Self {
        let panel = Self {
            page: Rc::new(InfoPage::new("about-panel")),
            app_model,
            settings: gio::Settings::new(SETTINGS),
            loaded_for: Rc::new(RefCell::new(None)),
        };
        // Loads only while the tab is on screen
        let page = Rc::downgrade(&panel.page);
        let model = Rc::downgrade(&panel.app_model);
        let loaded_for = Rc::downgrade(&panel.loaded_for);
        let settings = panel.settings.clone();
        panel.page.stack.connect_map(move |_| {
            if let (Some(page), Some(model), Some(loaded_for)) =
                (page.upgrade(), model.upgrade(), loaded_for.upgrade())
            {
                load(&page, &model, &settings, &loaded_for);
            }
        });
        panel.update();
        panel
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.page.widget()
    }

    fn update(&self) {
        if self.app_model.get_state().playback.current_song().is_none() {
            *self.loaded_for.borrow_mut() = None;
            self.page.show_nothing_playing();
        } else if self.page.is_shown() {
            load(
                &self.page,
                &self.app_model,
                &self.settings,
                &self.loaded_for,
            );
        }
    }
}

impl EventListener for AboutPanel {
    fn on_event(&mut self, event: &AppEvent) {
        if let AppEvent::PlaybackEvent(
            PlaybackEvent::TrackChanged(_)
            | PlaybackEvent::TrackSynced
            | PlaybackEvent::PlaybackStopped,
        ) = event
        {
            self.update();
        }
    }
}

/// Loads the playing song from Genius, once per song (the answers are
/// cached on disk, shared with the Now Playing panel's Credits card).
fn load(
    page: &Rc<InfoPage>,
    app_model: &Rc<AppModel>,
    settings: &gio::Settings,
    loaded_for: &Rc<RefCell<Option<String>>>,
) {
    let Some(track) = app_model.get_state().playback.current_song() else {
        page.show_nothing_playing();
        return;
    };
    let track_id = track.rri.id.clone();
    if loaded_for.borrow().as_deref() == Some(track_id.as_str()) {
        return;
    }
    let token = settings.string(GENIUS_ACCESS_TOKEN).trim().to_string();
    if token.is_empty() {
        page.show_status(
            ICON,
            // translators: Title in the About tab when no Genius token was added.
            &gettext("Genius Isn't Set Up"),
            &gettext("Add a Genius token to see who made this song, its story and the artist."),
            Some(setup_button(&gettext("Set Up Genius"))),
        );
        return;
    }
    *loaded_for.borrow_mut() = Some(track_id.clone());
    page.show_loading();

    let api = app_model.api();
    let artist = track.artists_name();
    let title = track.title.clone();
    let performers: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
    let weak_page = Rc::downgrade(page);
    let weak_loaded = Rc::downgrade(loaded_for);
    glib::spawn_future_local(async move {
        let result = api
            .fetch_genius_track_info(&track_id, &artist, &title, &token)
            .await;
        let still_wanted = |loaded: &Rc<RefCell<Option<String>>>| {
            loaded.borrow().as_deref() == Some(track_id.as_str())
        };
        let (Some(page), Some(loaded)) = (weak_page.upgrade(), weak_loaded.upgrade()) else {
            return;
        };
        if !still_wanted(&loaded) {
            return;
        }
        let info = match result {
            Ok(info) if info.is_exact_match => info,
            Ok(info) => {
                let url = info.url.clone();
                page.show_status(
                    ICON,
                    // translators: Title in the About tab when Genius doesn't have the playing song.
                    &gettext("Not on Genius"),
                    &gettext("Genius doesn't have a page for this song yet."),
                    Some(link_button(&gettext("Search on Genius"), url)),
                );
                return;
            }
            Err(canto_api::DomainError::AuthExpired) => {
                *loaded.borrow_mut() = None;
                page.show_status(
                    ICON,
                    &gettext("Genius Isn't Set Up"),
                    &gettext(
                        "Genius didn't accept your token. Paste a new one in Settings to see the credits again.",
                    ),
                    Some(setup_button(&gettext("Set Up Genius"))),
                );
                return;
            }
            Err(_) => {
                // Tried again the next time the tab opens
                *loaded.borrow_mut() = None;
                page.show_status(
                    ICON,
                    // translators: Title in the About tab when Genius can't be reached.
                    &gettext("Couldn't Reach Genius"),
                    &gettext("Check your connection, then open this tab again."),
                    Some(link_button(
                        &gettext("Search on Genius"),
                        api.genius_search_url(&artist, &title),
                    )),
                );
                return;
            }
        };
        render_song(&page, &title, &performers, &info);
        let Some(artist_id) = info.artist_id else {
            return;
        };
        let artist_info = api.fetch_genius_artist_info(artist_id, &token).await;
        if let (Some(page), Some(loaded)) = (weak_page.upgrade(), weak_loaded.upgrade()) {
            if still_wanted(&loaded) {
                match artist_info {
                    Ok(Some(artist)) => render_artist(&page, &artist),
                    Ok(None) => {}
                    Err(e) => debug!("Genius artist failed: {e}"),
                }
            }
        }
    });
}

/// "Samples", "Covered by"... in the interface's language.
fn relationship_label(kind: &str, fallback: &str) -> String {
    match kind {
        // translators: About tab: songs this song samples.
        "samples" => gettext("Samples"),
        // translators: About tab: songs that sample this song.
        "sampled_in" => gettext("Sampled in"),
        "interpolates" => gettext("Interpolates"),
        "interpolated_by" => gettext("Interpolated in"),
        "cover_of" => gettext("Cover of"),
        "covered_by" => gettext("Covered by"),
        "remix_of" => gettext("Remix of"),
        "remixed_by" => gettext("Remixed by"),
        "live_version_of" => gettext("Live version of"),
        "performed_live_as" => gettext("Performed live as"),
        _ => fallback.to_string(),
    }
}

/// Where the bio comes from, and whether Genius reviewed it.
fn bio_source(status: Option<GeniusBioStatus>) -> String {
    match status {
        Some(GeniusBioStatus::Unreviewed) => gettext("Song bio from Genius · Unreviewed"),
        Some(GeniusBioStatus::Reviewed) => gettext("Song bio from Genius · Reviewed by Genius"),
        Some(GeniusBioStatus::Verified) => gettext("Song bio from Genius · Verified by the artist"),
        None => gettext("Song bio from Genius"),
    }
}

/// A Genius text as Pango markup: the HTML version (with links) when there.
fn markup_of(markup: &Option<String>, plain: &Option<String>) -> Option<String> {
    match (markup, plain) {
        (Some(markup), _) if !markup.is_empty() => Some(markup.clone()),
        (_, Some(plain)) if !plain.trim().is_empty() => {
            Some(glib::markup_escape_text(plain).to_string())
        }
        _ => None,
    }
}

/// "Website", "Instagram"... for an artist's link.
fn link_label(kind: &str) -> String {
    match kind {
        // translators: An artist's own website, in the About tab.
        "website" => gettext("Website"),
        "instagram" => "Instagram".into(),
        "twitter" => "X".into(),
        "facebook" => "Facebook".into(),
        "youtube" => "YouTube".into(),
        "tiktok" => "TikTok".into(),
        "soundcloud" => "SoundCloud".into(),
        "spotify" => "Spotify".into(),
        "apple_music" => "Apple Music".into(),
        "tidal" => "TIDAL".into(),
        "deezer" => "Deezer".into(),
        "amazon_music" => "Amazon Music".into(),
        "shazam" => "Shazam".into(),
        other => {
            let words: Vec<String> = other
                .split('_')
                .filter(|w| !w.is_empty())
                .map(|w| {
                    let mut chars = w.chars();
                    chars
                        .next()
                        .map(|c| c.to_uppercase().chain(chars).collect())
                        .unwrap_or_default()
                })
                .collect();
            words.join(" ")
        }
    }
}

/// Symbolic brand icon for a known artist link kind.
fn link_icon(kind: &str) -> Option<&'static str> {
    match kind {
        "website" => Some("canto-social-web-symbolic"),
        "instagram" => Some("canto-social-instagram-symbolic"),
        "twitter" => Some("canto-social-x-symbolic"),
        "facebook" => Some("canto-social-facebook-symbolic"),
        "youtube" => Some("canto-social-youtube-symbolic"),
        "tiktok" => Some("canto-social-tiktok-symbolic"),
        "soundcloud" => Some("canto-social-soundcloud-symbolic"),
        "spotify" => Some("canto-social-spotify-symbolic"),
        "apple_music" => Some("canto-social-apple-music-symbolic"),
        "tidal" => Some("canto-social-tidal-symbolic"),
        "deezer" => Some("canto-social-deezer-symbolic"),
        "amazon_music" => Some("canto-social-amazon-music-symbolic"),
        "shazam" => Some("canto-social-shazam-symbolic"),
        _ => None,
    }
}

/// The song's cards; the artist's comes later (see [`render_artist`]).
fn render_song(page: &InfoPage, title: &str, performers: &[String], info: &GeniusTrackInfo) {
    page.clear();
    let content = &page.content;

    // What song this is
    let header = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let name = wrapping_label(&["title-3"]);
    name.set_text(title);
    header.append(&name);
    let mut details: Vec<String> = Vec::new();
    if !performers.is_empty() {
        details.push(performers.join(", "));
    }
    details.extend(info.album.iter().filter(|a| !a.is_empty()).cloned());
    details.extend(info.release_date.iter().filter(|d| !d.is_empty()).cloned());
    if !details.is_empty() {
        let line = wrapping_label(&["dim-label"]);
        line.set_text(&details.join(" · "));
        header.append(&line);
    }
    content.append(&header);

    // The song's story
    if let Some(full) = markup_of(&info.description_markup, &info.description) {
        let bio = card(&["about-panel__bio"]);
        // translators: Heading of the Genius song bio.
        bio.append(&heading(&gettext("About This Song")));
        bio.append(&folding_text(full));
        bio.append(&caption(&bio_source(info.description_status)));
        content.append(&bio);
    }

    // Everyone who made it
    let credits = card(&["about-panel__credits"]);
    credits.append(&heading(&gettext("Credits")));
    let mut any = false;
    if !performers.is_empty() {
        credits.append(&row(&gettext("Performed by"), &performers.join(", ")));
    }
    for (role, names) in [
        (gettext("Featured Artists"), &info.featured_artists),
        (gettext("Producer(s)"), &info.producers),
        (gettext("Writer(s)"), &info.writers),
    ] {
        if !names.is_empty() {
            credits.append(&row(&role, &names.join(", ")));
            any = true;
        }
    }
    for credit in &info.credits {
        credits.append(&row(&credit.role, &credit.names.join(", ")));
        any = true;
    }
    if let Some(place) = info.recording_location.as_deref().filter(|p| !p.is_empty()) {
        // translators: About tab: the studio or place where the song was recorded.
        credits.append(&row(&gettext("Recorded At"), place));
        any = true;
    }
    if any {
        content.append(&credits);
    }

    // Samples, covers, remixes, live versions
    let relationships: Vec<_> = info
        .relationships
        .iter()
        .filter(|r| !r.tracks.is_empty())
        .collect();
    if !relationships.is_empty() {
        let connections = card(&["about-panel__connections"]);
        // translators: Heading in the About tab: samples, covers, remixes and live versions of the song.
        connections.append(&heading(&gettext("Samples and Versions")));
        for rel in relationships {
            connections.append(&row(
                &relationship_label(&rel.kind, &rel.label),
                &rel.tracks.join("\n"),
            ));
        }
        content.append(&connections);
    }

    // Translations by Genius users: links only
    if !info.translations.is_empty() {
        let translations = card(&["about-panel__translations"]);
        translations.append(&heading(&gettext("Translations")));
        translations.append(&caption(&gettext(
            "Made by Genius users. They open in your browser.",
        )));
        translations.append(&flow(
            info.translations
                .iter()
                .map(|t| link_button(&t.label, t.url.clone()).upcast())
                .collect(),
        ));
        content.append(&translations);
    }

    // The song's page on Genius
    let genius = card(&["about-panel__genius"]);
    // translators: Heading of the About tab's card with the song's numbers on Genius and its link.
    genius.append(&heading(&gettext("On Genius")));
    if let Some(numbers) = number_columns(vec![
        // translators: Under a number in the About tab: how many times the song's Genius page was seen.
        (info.stats.pageviews, gettext("page views")),
        // translators: Under a number in the About tab: how many people worked on the song's Genius page.
        (info.stats.contributors, gettext("contributors")),
        // translators: Under a number in the About tab: reviewed explanations of lyric lines.
        (info.stats.annotations, gettext("annotations")),
    ]) {
        genius.append(&numbers);
    }
    genius.append(&link_button(&gettext("Open on Genius"), info.url.clone()));
    content.append(&genius);
}

/// The artist's card, before the Genius one at the end.
fn render_artist(page: &InfoPage, artist: &GeniusArtistInfo) {
    let bio = markup_of(&artist.description_markup, &artist.description);
    if bio.is_none() && artist.alternate_names.is_empty() && artist.links.is_empty() {
        return;
    }
    let block = card(&["about-panel__artist"]);
    block.append(&heading(&artist.name));
    if !artist.alternate_names.is_empty() {
        // translators: About tab, under the artist's name: their other names. {} is the list.
        let aka = gettext("Also known as {}").replace("{}", &artist.alternate_names.join(", "));
        block.append(&caption(&aka));
    }
    let has_bio_content = !artist.alternate_names.is_empty() || bio.is_some();
    if let Some(bio) = bio {
        block.append(&folding_text(bio));
    }
    if !artist.links.is_empty() {
        if has_bio_content {
            let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
            sep.add_css_class("info-card__separator");
            block.append(&sep);
        }
        // translators: About tab, inside the artist card above external links (Instagram, YouTube, etc.).
        block.append(&caption(&gettext("Links")));
        block.append(&artist_links_flow(&artist.links));
    }
    let content = &page.content;
    match content.last_child() {
        Some(last) => {
            let before = last.prev_sibling();
            content.insert_child_after(&block, before.as_ref());
        }
        None => content.append(&block),
    }
}

/// Renders artist external links as clean circular brand icons (falling back
/// to compact text chips only for unknown platforms).
fn artist_links_flow(links: &[(String, String)]) -> libadwaita::WrapBox {
    let browser_tip = gettext("Opens in your browser");
    let widgets = links
        .iter()
        .map(|(kind, url)| {
            let label = link_label(kind);
            if let Some(icon) = link_icon(kind) {
                let btn = gtk::Button::from_icon_name(icon);
                btn.add_css_class("flat");
                btn.add_css_class("circular");
                btn.add_css_class("info-social-icon");
                btn.set_tooltip_text(Some(&format!("{label} · {browser_tip}")));
                btn.update_property(&[gtk::accessible::Property::Label(&label)]);
                let url = url.clone();
                btn.connect_clicked(move |button| {
                    crate::app::components::open_external_url(button, &url);
                });
                btn.upcast()
            } else {
                compact_link_button(&label, url.clone()).upcast()
            }
        })
        .collect();
    flow(widgets)
}

/// The tab filled with a sample song, to look at its rendering.
#[cfg(test)]
pub(crate) fn sample_for_tests() -> gtk::Widget {
    use canto_api::{GeniusCredit, GeniusRelationship, GeniusSongStats, GeniusTranslation};
    let page = InfoPage::new("about-panel");
    let info = GeniusTrackInfo {
        song_id: Some(1),
        url: "https://genius.com/".into(),
        is_exact_match: true,
        album: Some("Blonde".into()),
        release_date: Some("August 20, 2016".into()),
        producers: vec!["Frank Ocean".into(), "Pharrell Williams".into()],
        writers: vec!["Frank Ocean".into(), "Pharrell Williams".into(), "Tyler, The Creator".into()],
        featured_artists: vec!["Beyoncé".into()],
        credits: vec![
            GeniusCredit {
                role: "Mastering Engineer".into(),
                names: vec!["Mike Dean".into()],
            },
            GeniusCredit {
                role: "Additional Vocals".into(),
                names: vec!["Beyoncé".into()],
            },
        ],
        recording_location: Some("Electric Lady Studios".into()),
        relationships: vec![GeniusRelationship {
            kind: "covered_by".into(),
            label: "Covered by".into(),
            tracks: vec!["Pink + White by Kehlani".into(), "Pink + White by Ruel".into()],
        }],
        description: Some("x".into()),
        description_markup: Some(
            "The ninth track on <a href=\"https://genius.com/\">Blonde</a> reflects on the cycle of life and death.\n\nPharrell produced it with a string section.\n\nA third paragraph that stays folded until Show More is clicked, so the card doesn't run long."
                .into(),
        ),
        description_status: Some(GeniusBioStatus::Reviewed),
        translations: vec![
            GeniusTranslation {
                language: "pt".into(),
                label: "Tradução em Português".into(),
                url: "https://genius.com/pt".into(),
            },
            GeniusTranslation {
                language: "es".into(),
                label: "Traducción al Español".into(),
                url: "https://genius.com/es".into(),
            },
        ],
        stats: GeniusSongStats {
            pageviews: Some(2_555_469),
            contributors: Some(270),
            annotations: Some(13),
            unreviewed_annotations: None,
        },
        artist_id: Some(1985),
        ..Default::default()
    };
    render_song(&page, "Pink + White", &["Frank Ocean".to_string()], &info);
    render_artist(
        &page,
        &GeniusArtistInfo {
            id: 1985,
            name: "Frank Ocean".into(),
            url: "https://genius.com/artists/Frank-ocean".into(),
            description: Some("x".into()),
            description_markup: Some(
                "Christopher Edwin Breaux, known as Frank Ocean, is a singer and songwriter from New Orleans."
                    .into(),
            ),
            alternate_names: vec!["Lonny Breaux".into()],
            links: vec![
                ("website".into(), "https://blonded.co".into()),
                ("instagram".into(), "https://www.instagram.com/blonded".into()),
                ("youtube".into(), "https://www.youtube.com/".into()),
            ],
            followers: None,
        },
    );
    page.stack.clone().upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_kinds_get_readable_names() {
        assert_eq!(link_label("twitter"), "X");
        assert_eq!(link_label("apple_music"), "Apple Music");
        assert_eq!(link_label("new_site"), "New Site");
        assert_eq!(
            link_icon("instagram"),
            Some("canto-social-instagram-symbolic")
        );
        assert_eq!(link_icon("twitter"), Some("canto-social-x-symbolic"));
        assert_eq!(link_icon("shazam"), Some("canto-social-shazam-symbolic"));
        assert_eq!(link_icon("new_site"), None);
    }

    #[test]
    fn plain_bios_are_escaped_and_markup_wins() {
        assert_eq!(
            markup_of(&None, &Some("A & B".into())).as_deref(),
            Some("A &amp; B")
        );
        assert_eq!(
            markup_of(&Some("<i>x</i>".into()), &Some("x".into())).as_deref(),
            Some("<i>x</i>")
        );
        assert_eq!(markup_of(&None, &Some("  ".into())), None);
    }

    #[test]
    fn unknown_relationships_keep_genius_wording() {
        assert_eq!(
            relationship_label("something_new", "Something"),
            "Something"
        );
    }
}
