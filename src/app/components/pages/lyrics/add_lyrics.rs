//! "Add Lyrics": lyrics a listener types or pastes for a song no source
//! has, sent to LRCLIB (a free, public lyrics library) only after they
//! confirm exactly what goes out. Nothing here fills the text in.
//!
//! Not reachable yet: nothing opens it outside the tests. The window is
//! shown for approval first, and sending comes after it.

use gettextrs::gettext;
use gtk::glib;
use gtk::prelude::*;
use libadwaita::prelude::*;

/// The song the lyrics are for, as LRCLIB files it.
#[derive(Clone, Debug, PartialEq)]
pub struct SongForLyrics {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
}

/// What would be sent: the plain lyrics, and the synced ones when the
/// lines carry times.
#[derive(Clone, Debug, PartialEq)]
pub struct LyricsDraft {
    pub plain: String,
    pub synced: Option<String>,
    pub lines: usize,
}

/// A line's time, as "[01:23.45]" or "[01:23]", and the text after it.
fn split_time(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('[')?;
    let (time, text) = rest.split_once(']')?;
    let (minutes, seconds) = time.split_once(':')?;
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    let (whole, fraction) = seconds.split_once('.').unwrap_or((seconds, "0"));
    (digits(minutes) && digits(whole) && digits(fraction)).then_some(text.trim_start())
}

/// The draft for `text`: synced when every line with words carries a time.
pub fn parse_draft(text: &str) -> LyricsDraft {
    let text = text.trim();
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    let worded: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| !l.trim().is_empty())
        .collect();
    let synced = !worded.is_empty() && worded.iter().all(|l| split_time(l).is_some());
    let plain = if synced {
        lines
            .iter()
            .map(|l| split_time(l).unwrap_or(l).to_string())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        lines.join("\n")
    };
    LyricsDraft {
        plain,
        synced: synced.then(|| lines.join("\n")),
        lines: worded.len(),
    }
}

fn duration_label(ms: u32) -> String {
    let seconds = ms / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The window, over `parent`. `on_publish` gets the draft once the listener
/// confirmed it; the text box starts empty.
pub fn open(
    parent: &impl IsA<gtk::Widget>,
    song: &SongForLyrics,
    on_publish: impl Fn(LyricsDraft) + 'static,
) -> libadwaita::Dialog {
    let dialog = libadwaita::Dialog::builder()
        // translators: Title of the window where a listener adds the lyrics of a song no source has.
        .title(gettext("Add Lyrics"))
        .content_width(480)
        .content_height(640)
        .build();
    dialog.add_css_class("add-lyrics");
    let toolbar = libadwaita::ToolbarView::new();
    toolbar.add_top_bar(&libadwaita::HeaderBar::new());

    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_margin_top(6);
    body.set_margin_bottom(18);
    body.set_margin_start(18);
    body.set_margin_end(18);

    let title = gtk::Label::new(Some(&song.title));
    title.add_css_class("title-3");
    title.set_halign(gtk::Align::Start);
    title.set_wrap(true);
    let about = gtk::Label::new(Some(&format!(
        "{} · {} · {}",
        song.artist,
        song.album,
        duration_label(song.duration_ms)
    )));
    about.add_css_class("dim-label");
    about.set_halign(gtk::Align::Start);
    about.set_wrap(true);
    body.append(&title);
    body.append(&about);

    // translators: Above the box where the lyrics are pasted or typed in "Add Lyrics".
    let hint = gtk::Label::new(Some(&gettext(
        "Paste or type the lyrics, one line per line. Lines that start with a time, like [01:23.45], follow the song.",
    )));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    hint.add_css_class("caption");
    body.append(&hint);

    let text = gtk::TextView::new();
    text.set_wrap_mode(gtk::WrapMode::WordChar);
    text.set_top_margin(10);
    text.set_bottom_margin(10);
    text.set_left_margin(10);
    text.set_right_margin(10);
    text.add_css_class("add-lyrics__text");
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_child(Some(&text));
    scroller.set_vexpand(true);
    scroller.set_min_content_height(260);
    scroller.add_css_class("card");
    body.append(&scroller);

    // translators: Under the lyrics box in "Add Lyrics": where they go and who can see them.
    let notice = gtk::Label::new(Some(&gettext(
        "Lyrics you add go to LRCLIB, a free public lyrics library, without an account: anyone can read and use them. Add only lyrics you typed yourself or may share.",
    )));
    notice.set_wrap(true);
    notice.set_xalign(0.0);
    notice.add_css_class("dim-label");
    notice.add_css_class("caption");
    body.append(&notice);

    // translators: Button in "Add Lyrics" that shows what will be sent, before sending it.
    let review = gtk::Button::with_label(&gettext("Review and Publish…"));
    review.add_css_class("pill");
    review.add_css_class("suggested-action");
    review.set_halign(gtk::Align::Center);
    review.set_sensitive(false);
    body.append(&review);

    let buffer = text.buffer();
    buffer.connect_changed(glib::clone!(
        #[weak]
        review,
        move |buffer| {
            let (start, end) = buffer.bounds();
            review.set_sensitive(!buffer.text(&start, &end, false).trim().is_empty());
        }
    ));
    let song = song.clone();
    let on_publish = std::rc::Rc::new(on_publish);
    review.connect_clicked(glib::clone!(
        #[weak]
        dialog,
        #[weak]
        buffer,
        move |_| {
            let (start, end) = buffer.bounds();
            let draft = parse_draft(&buffer.text(&start, &end, false));
            let confirm = confirmation(&song, &draft);
            let on_publish = on_publish.clone();
            confirm.connect_response(Some("publish"), move |_, _| on_publish(draft.clone()));
            confirm.present(Some(&dialog));
        }
    ));

    toolbar.set_content(Some(&body));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(parent));
    dialog
}

/// The question before sending: the song, how many lines, synced or not,
/// and that it becomes public.
pub fn confirmation(song: &SongForLyrics, draft: &LyricsDraft) -> libadwaita::AlertDialog {
    let kind = if draft.synced.is_some() {
        // translators: In the confirmation before publishing lyrics. {} is the number of lines.
        gettext("{} lines, following the song")
    } else {
        // translators: In the confirmation before publishing lyrics. {} is the number of lines.
        gettext("{} lines, without times")
    }
    .replace("{}", &draft.lines.to_string());
    let body = format!(
        "{}\n{} · {} · {}\n{}\n\n{}",
        song.title,
        song.artist,
        song.album,
        duration_label(song.duration_ms),
        kind,
        // translators: Last line of the confirmation before publishing lyrics to LRCLIB.
        gettext("Once published, anyone can see and use them.")
    );
    // translators: Heading of the confirmation before publishing lyrics.
    let alert = libadwaita::AlertDialog::new(Some(&gettext("Publish to LRCLIB?")), Some(&body));
    // translators: Button in the confirmation before publishing lyrics.
    alert.add_response("cancel", &gettext("Cancel"));
    // translators: Button in the confirmation before publishing lyrics: sends them.
    alert.add_response("publish", &gettext("Publish"));
    alert.set_response_appearance("publish", libadwaita::ResponseAppearance::Suggested);
    alert.set_default_response(Some("cancel"));
    alert.set_close_response("cancel");
    alert
}

/// The text box of an open window, for the GTK checks and previews.
#[cfg(test)]
pub fn text_view_for_tests(dialog: &libadwaita::Dialog) -> Option<gtk::TextView> {
    let mut stack = vec![dialog.clone().upcast::<gtk::Widget>()];
    while let Some(widget) = stack.pop() {
        if let Ok(view) = widget.clone().downcast::<gtk::TextView>() {
            return Some(view);
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            stack.push(c);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_on_every_line_make_synced_lyrics() {
        let draft = parse_draft("[00:12.40] First sample line\n\n[00:15] Second sample line\n");
        assert_eq!(draft.lines, 2);
        assert_eq!(draft.plain, "First sample line\n\nSecond sample line");
        assert_eq!(
            draft.synced.as_deref(),
            Some("[00:12.40] First sample line\n\n[00:15] Second sample line")
        );
    }

    #[test]
    fn a_line_without_a_time_keeps_the_lyrics_plain() {
        let draft = parse_draft("[00:12.40] First sample line\nSecond sample line");
        assert_eq!(draft.synced, None);
        assert_eq!(draft.lines, 2);
        assert!(draft.plain.starts_with("[00:12.40]"));
        assert_eq!(parse_draft("  ").lines, 0);
        assert_eq!(parse_draft("[1:2x] no").synced, None);
    }

    #[test]
    fn durations_read_as_minutes_and_seconds() {
        assert_eq!(duration_label(225_000), "3:45");
        assert_eq!(duration_label(61_000), "1:01");
    }
}
