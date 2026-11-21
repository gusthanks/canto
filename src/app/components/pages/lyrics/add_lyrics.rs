//! "Add Lyrics": lyrics a listener types or pastes for a song no source
//! has, sent to LRCLIB (a free, public lyrics library) only after they
//! confirm exactly what goes out. Nothing here fills the text in.
//!
//! Opened from the lyrics page when no source has the song. Each sending
//! is confirmed first.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

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

fn is_time_tag(tag: &str) -> bool {
    let Some((minutes, seconds)) = tag.split_once(':') else {
        return false;
    };
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    let (whole, fraction) = seconds
        .split_once('.')
        .or_else(|| seconds.split_once(','))
        .unwrap_or((seconds, "0"));
    digits(minutes) && digits(whole) && digits(fraction)
}

/// One or more leading line times, as "[01:23.45]", "[01:23,45]", or
/// "[01:23]", and the text after them.
fn split_time(line: &str) -> Option<&str> {
    let mut rest = line.trim_start();
    let mut matched = false;
    while let Some(after_open) = rest.strip_prefix('[') {
        let Some((tag, after_close)) = after_open.split_once(']') else {
            break;
        };
        if !is_time_tag(tag) {
            break;
        }
        matched = true;
        rest = after_close.trim_start();
    }
    matched.then_some(rest)
}

/// Whether `line` is an LRC metadata header such as `[ar: Artist]` or
/// `[length: 03:45]`, matching the LRC parser's rule.
fn is_lrc_metadata_tag(line: &str) -> bool {
    let line = line.trim();
    let Some(inner) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
        return false;
    };
    if inner.contains(']') {
        return false;
    }
    let Some((key, _)) = inner.split_once(':') else {
        return false;
    };
    let key = key.trim();
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphabetic())
}

fn trim_blank_edges(lines: Vec<String>) -> Vec<String> {
    let start = lines
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map_or(0, |i| i + 1);
    if start >= end {
        Vec::new()
    } else {
        lines.into_iter().skip(start).take(end - start).collect()
    }
}

/// The draft for `text`: synced when every non-metadata line with words
/// carries a time. LRC metadata headers are ignored and timestamps are
/// always stripped from `plain` so they never leak into plain lyrics.
pub fn parse_draft(text: &str) -> LyricsDraft {
    let text = text.trim();
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    let content_lines: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| !l.trim().is_empty() && !is_lrc_metadata_tag(l))
        .collect();
    if content_lines.is_empty() {
        return LyricsDraft {
            plain: String::new(),
            synced: None,
            lines: 0,
        };
    }
    let has_any_time = content_lines.iter().any(|l| split_time(l).is_some());
    let synced = content_lines.iter().all(|l| split_time(l).is_some());
    let plain_lines = if synced || has_any_time {
        trim_blank_edges(
            lines
                .iter()
                .copied()
                .filter(|l| !is_lrc_metadata_tag(l))
                .map(|l| split_time(l).unwrap_or(l).to_string())
                .collect(),
        )
    } else {
        lines.iter().map(|l| (*l).to_string()).collect()
    };
    let plain = plain_lines.join("\n");
    let synced_out = if synced {
        let synced_lines = trim_blank_edges(
            lines
                .iter()
                .copied()
                .filter(|l| !is_lrc_metadata_tag(l))
                .map(|l| l.trim_start().to_string())
                .collect(),
        );
        Some(synced_lines.join("\n"))
    } else {
        None
    };
    let worded = plain.lines().filter(|l| !l.trim().is_empty()).count();
    LyricsDraft {
        plain,
        synced: synced_out,
        lines: worded,
    }
}

/// The window while its lyrics are being sent: `done` closes it, `failed`
/// says why and lets the listener try again, `cancel_flag` lets closing the
/// dialog abort the proof-of-work and HTTP send.
#[derive(Clone)]
pub struct Sending {
    dialog: glib::WeakRef<libadwaita::Dialog>,
    status: glib::WeakRef<gtk::Label>,
    spinner: glib::WeakRef<libadwaita::Spinner>,
    review: glib::WeakRef<gtk::Button>,
    text: glib::WeakRef<gtk::TextView>,
    busy: Rc<Cell<bool>>,
    cancel: Arc<AtomicBool>,
    active_cancel: Rc<RefCell<Option<Arc<AtomicBool>>>>,
}

impl Sending {
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn start(&self) {
        if let Some(status) = self.status.upgrade() {
            // translators: In "Add Lyrics" while the lyrics are sent; LRCLIB first asks the computer to solve a small puzzle.
            status.set_label(&gettext(
                "Preparing and sending… This can take a minute on a slower computer.",
            ));
            status.remove_css_class("error");
            status.set_visible(true);
        }
        self.busy(true);
    }

    fn busy(&self, busy: bool) {
        self.busy.set(busy);
        if let Some(spinner) = self.spinner.upgrade() {
            spinner.set_visible(busy);
        }
        if let Some(review) = self.review.upgrade() {
            review.set_sensitive(!busy);
        }
        if let Some(text) = self.text.upgrade() {
            text.set_editable(!busy);
        }
    }

    pub fn done(&self) {
        *self.active_cancel.borrow_mut() = None;
        self.busy(false);
        if let Some(dialog) = self.dialog.upgrade() {
            dialog.force_close();
        }
    }

    pub fn failed(&self, error: &str) {
        *self.active_cancel.borrow_mut() = None;
        self.busy(false);
        if let Some(status) = self.status.upgrade() {
            // translators: In "Add Lyrics" when sending failed. {} is the reason, from LRCLIB.
            status.set_label(&gettext("Couldn't publish: {}").replace("{}", error));
            status.add_css_class("error");
            status.set_visible(true);
        }
    }
}

fn duration_label(ms: u32) -> String {
    let seconds = ms / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The window, over `parent`. `on_publish` gets the draft once the listener
/// confirmed it, and the window's `Sending` to tell how it went; the text
/// box starts empty.
pub fn open(
    parent: &impl IsA<gtk::Widget>,
    song: &SongForLyrics,
    on_publish: impl Fn(LyricsDraft, Sending) + 'static,
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

    let spinner = libadwaita::Spinner::new();
    spinner.set_visible(false);
    let status = gtk::Label::new(None);
    status.set_wrap(true);
    status.set_visible(false);
    status.add_css_class("caption");
    let sending_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    sending_row.set_halign(gtk::Align::Center);
    sending_row.append(&spinner);
    sending_row.append(&status);
    body.append(&sending_row);
    body.append(&review);
    let busy = Rc::new(Cell::new(false));
    let active_cancel: Rc<RefCell<Option<Arc<AtomicBool>>>> = Rc::new(RefCell::new(None));
    let cancel_on_close = Rc::clone(&active_cancel);
    dialog.connect_closed(move |_| {
        if let Some(flag) = cancel_on_close.borrow_mut().take() {
            flag.store(true, Ordering::Relaxed);
        }
    });

    let dialog_weak = dialog.downgrade();
    let status_weak = status.downgrade();
    let spinner_weak = spinner.downgrade();
    let review_weak = review.downgrade();
    let text_weak = text.downgrade();

    let buffer = text.buffer();
    let busy_changed = Rc::clone(&busy);
    buffer.connect_changed(glib::clone!(
        #[weak]
        review,
        move |buffer| {
            let (start, end) = buffer.bounds();
            let ready =
                !busy_changed.get() && parse_draft(&buffer.text(&start, &end, false)).lines > 0;
            review.set_sensitive(ready);
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
            let dialog_weak = dialog_weak.clone();
            let status_weak = status_weak.clone();
            let spinner_weak = spinner_weak.clone();
            let review_weak = review_weak.clone();
            let text_weak = text_weak.clone();
            let busy = Rc::clone(&busy);
            let active_cancel = Rc::clone(&active_cancel);
            confirm.connect_response(Some("publish"), move |_, _| {
                let cancel = Arc::new(AtomicBool::new(false));
                *active_cancel.borrow_mut() = Some(Arc::clone(&cancel));
                let sending = Sending {
                    dialog: dialog_weak.clone(),
                    status: status_weak.clone(),
                    spinner: spinner_weak.clone(),
                    review: review_weak.clone(),
                    text: text_weak.clone(),
                    busy: Rc::clone(&busy),
                    cancel,
                    active_cancel: Rc::clone(&active_cancel),
                };
                sending.start();
                on_publish(draft.clone(), sending);
            });
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
        assert_eq!(draft.plain, "First sample line\nSecond sample line");
        assert_eq!(parse_draft("  ").lines, 0);
        assert_eq!(parse_draft("[1:2x] no").synced, None);
    }

    #[test]
    fn lrc_metadata_headers_stay_synced_and_leave_plain_clean() {
        let draft = parse_draft(
            "[ar: Niink]\n[ti: Disco Voador]\n[al: Flowra FM]\n[length: 03:45]\n\n[00:12.40] First line\n[00:15,10] Second line\n",
        );
        assert_eq!(draft.lines, 2);
        assert_eq!(draft.plain, "First line\nSecond line");
        assert_eq!(
            draft.synced.as_deref(),
            Some("[00:12.40] First line\n[00:15,10] Second line")
        );
    }

    #[test]
    fn only_metadata_tags_count_as_empty() {
        let draft = parse_draft("[ar: Niink]\n[ti: Disco Voador]");
        assert_eq!(draft.lines, 0);
        assert_eq!(draft.synced, None);
        assert!(draft.plain.is_empty());
    }

    #[test]
    fn durations_read_as_minutes_and_seconds() {
        assert_eq!(duration_label(225_000), "3:45");
        assert_eq!(duration_label(61_000), "1:01");
    }
}
