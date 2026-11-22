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
use std::time::Duration;

use gettextrs::gettext;
use gtk::glib;
use gtk::prelude::*;
use libadwaita::prelude::*;

use crate::app::components::labels;

/// Playback controls supplied by the lyrics page to the 1-button Sync Studio.
#[derive(Clone)]
pub struct SyncPlayback {
    pub position_ms: Rc<dyn Fn() -> u64>,
    pub is_playing: Rc<dyn Fn() -> bool>,
    pub toggle_play: Rc<dyn Fn()>,
    pub seek_ms: Rc<dyn Fn(u32)>,
}

/// One lyric line inside the 1-button Sync Studio session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncLine {
    pub text: String,
    pub time_ms: Option<u64>,
}

/// Stateful line-by-line timing model backing the Sync Studio UI.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncSession {
    lines: Vec<SyncLine>,
    cursor: usize,
}

pub fn format_lrc_timestamp(ms: u64) -> String {
    let total_cs = ms / 10;
    let cs = total_cs % 100;
    let total_secs = total_cs / 100;
    let secs = total_secs % 60;
    let mins = total_secs / 60;
    format!("[{mins:02}:{secs:02}.{cs:02}]")
}

fn format_clock_readout(ms: u64) -> String {
    let total_cs = ms / 10;
    let cs = total_cs % 100;
    let total_secs = total_cs / 100;
    let secs = total_secs % 60;
    let mins = total_secs / 60;
    format!("{mins:02}:{secs:02}.{cs:02}")
}

fn parse_lrc_timestamp_ms(line: &str) -> Option<(u64, &str)> {
    let rest = line.trim_start();
    let after_open = rest.strip_prefix('[')?;
    let (tag, _) = after_open.split_once(']')?;
    let (mins_str, secs_part) = tag.split_once(':')?;
    let mins: u64 = mins_str.parse().ok()?;
    let (whole_str, frac_str) = secs_part
        .split_once('.')
        .or_else(|| secs_part.split_once(','))
        .unwrap_or((secs_part, ""));
    let whole: u64 = whole_str.parse().ok()?;
    let frac_ms = match frac_str.len() {
        0 => 0,
        1 => frac_str.parse::<u64>().ok()? * 100,
        2 => frac_str.parse::<u64>().ok()? * 10,
        _ => frac_str[..3].parse::<u64>().ok()?,
    };
    let text = split_time(line)?;
    let ms = mins
        .saturating_mul(60_000)
        .saturating_add(whole.saturating_mul(1000))
        .saturating_add(frac_ms);
    Some((ms, text))
}

impl SyncSession {
    pub fn from_text(raw: &str) -> Self {
        let mut lines = Vec::new();
        for raw_line in raw.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || is_lrc_metadata_tag(trimmed) {
                continue;
            }
            if let Some((ms, text)) = parse_lrc_timestamp_ms(raw_line) {
                let text = text.trim();
                if !text.is_empty() {
                    lines.push(SyncLine {
                        text: text.to_string(),
                        time_ms: Some(ms),
                    });
                }
            } else {
                lines.push(SyncLine {
                    text: trimmed.to_string(),
                    time_ms: None,
                });
            }
        }
        let cursor = lines
            .iter()
            .position(|l| l.time_ms.is_none())
            .unwrap_or(lines.len());
        Self { lines, cursor }
    }

    pub fn lines(&self) -> &[SyncLine] {
        &self.lines
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn total(&self) -> usize {
        self.lines.len()
    }

    pub fn stamped_count(&self) -> usize {
        self.lines.iter().filter(|l| l.time_ms.is_some()).count()
    }

    pub fn is_complete(&self) -> bool {
        !self.lines.is_empty() && self.stamped_count() == self.lines.len()
    }

    pub fn stamp(&mut self, pos_ms: u64) -> bool {
        if self.cursor >= self.lines.len() {
            return false;
        }
        let prev_floor = self.lines[..self.cursor]
            .iter()
            .rev()
            .find_map(|l| l.time_ms)
            .unwrap_or(0);
        let ts = pos_ms.max(prev_floor);
        self.lines[self.cursor].time_ms = Some(ts);
        self.cursor += 1;
        true
    }

    pub fn undo(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        self.lines[self.cursor].time_ms = None;
        true
    }

    pub fn clear_all(&mut self) {
        for line in &mut self.lines {
            line.time_ms = None;
        }
        self.cursor = 0;
    }

    pub fn select_cursor(&mut self, idx: usize) {
        self.cursor = idx.min(self.lines.len());
    }

    pub fn to_lrc(&self) -> String {
        self.lines
            .iter()
            .map(|line| match line.time_ms {
                Some(ms) => format!("{} {}", format_lrc_timestamp(ms), line.text),
                None => line.text.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

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
    open_with_playback(parent, song, None, on_publish)
}

/// Same as [`open`], attaching optional live playback controls for the
/// 1-button Lyrics Sync Studio.
pub fn open_with_playback(
    parent: &impl IsA<gtk::Widget>,
    song: &SongForLyrics,
    playback: Option<SyncPlayback>,
    on_publish: impl Fn(LyricsDraft, Sending) + 'static,
) -> libadwaita::Dialog {
    let dialog = libadwaita::Dialog::builder()
        // translators: Title of the window where a listener adds the lyrics of a song no source has.
        .title(gettext("Add Lyrics"))
        .content_width(520)
        .content_height(660)
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

    // Mode switcher: Write / Paste <-> Sync While Listening
    let mode_bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    mode_bar.add_css_class("linked");
    mode_bar.set_halign(gtk::Align::Center);
    // translators: Mode toggle in Add Lyrics for typing or pasting text.
    let write_btn = gtk::ToggleButton::with_label(&gettext("Write / Paste"));
    write_btn.set_active(true);
    // translators: Mode toggle in Add Lyrics opening the 1-button Lyrics Sync Studio.
    let sync_btn = gtk::ToggleButton::with_label(&gettext("Sync While Listening"));
    sync_btn.set_group(Some(&write_btn));
    sync_btn.set_sensitive(false);
    mode_bar.append(&write_btn);
    mode_bar.append(&sync_btn);
    body.append(&mode_bar);

    let mode_stack = gtk::Stack::new();
    mode_stack.set_vexpand(true);
    mode_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    body.append(&mode_stack);

    // Page 1: Write / Paste
    let write_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    // translators: Above the box where the lyrics are pasted or typed in "Add Lyrics".
    let hint = gtk::Label::new(Some(&gettext(
        "Paste or type the lyrics, one line per line. Lines that start with a time, like [01:23.45], follow the song.",
    )));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    hint.add_css_class("caption");
    write_page.append(&hint);

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
    scroller.set_min_content_height(240);
    scroller.add_css_class("card");
    write_page.append(&scroller);
    mode_stack.add_named(&write_page, Some("write"));

    // Page 2: 1-Button Lyrics Sync Studio
    let sync_page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let status_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let sync_progress = gtk::Label::new(None);
    sync_progress.set_halign(gtk::Align::Start);
    sync_progress.set_hexpand(true);
    sync_progress.set_ellipsize(gtk::pango::EllipsizeMode::End);
    sync_progress.add_css_class("caption");
    sync_progress.add_css_class("dim-label");

    let initial_pos = playback.as_ref().map_or(0, |pb| (pb.position_ms)());
    let sync_clock = gtk::Label::new(Some(&format_clock_readout(initial_pos)));
    sync_clock.set_halign(gtk::Align::End);
    sync_clock.add_css_class("caption");
    sync_clock.add_css_class("dim-label");
    sync_clock.add_css_class("sync-studio__clock");
    status_header.append(&sync_progress);
    status_header.append(&sync_clock);
    sync_page.append(&status_header);

    let sync_list = gtk::ListBox::new();
    sync_list.set_selection_mode(gtk::SelectionMode::Single);
    sync_list.add_css_class("navigation-sidebar");
    sync_list.add_css_class("sync-studio__list");
    let sync_scroller = gtk::ScrolledWindow::new();
    sync_scroller.set_child(Some(&sync_list));
    sync_scroller.set_vexpand(true);
    sync_scroller.set_min_content_height(220);
    sync_scroller.add_css_class("card");
    sync_page.append(&sync_scroller);

    let studio_controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    studio_controls.set_halign(gtk::Align::Center);
    let play_btn = playback.as_ref().map(|pb| {
        let icon = if (pb.is_playing)() {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        };
        let btn = gtk::Button::from_icon_name(icon);
        btn.add_css_class("circular");
        btn.set_focus_on_click(false);
        // translators: Play/pause button inside the Lyrics Sync Studio.
        btn.set_tooltip_text(Some(&gettext("Play / Pause")));
        let toggle = Rc::clone(&pb.toggle_play);
        btn.connect_clicked(move |_| toggle());
        studio_controls.append(&btn);

        let rew = gtk::Button::from_icon_name("media-skip-backward-symbolic");
        rew.add_css_class("circular");
        rew.set_focus_on_click(false);
        // translators: Rewind 3 seconds button inside the Lyrics Sync Studio.
        rew.set_tooltip_text(Some(&gettext("Rewind 3 seconds (Left arrow)")));
        let pos_fn = Rc::clone(&pb.position_ms);
        let seek_fn = Rc::clone(&pb.seek_ms);
        rew.connect_clicked(move |_| {
            let target = pos_fn().saturating_sub(3000).min(u64::from(u32::MAX)) as u32;
            seek_fn(target);
        });
        studio_controls.append(&rew);
        btn
    });

    // translators: Primary button in the Lyrics Sync Studio that stamps the current song time onto the active lyric line.
    let stamp_btn = gtk::Button::with_label(&gettext("Stamp Line"));
    stamp_btn.add_css_class("pill");
    stamp_btn.add_css_class("suggested-action");
    stamp_btn.set_focus_on_click(false);
    // translators: Tooltip of the Stamp Line button in the Lyrics Sync Studio.
    stamp_btn.set_tooltip_text(Some(&gettext("Stamp current time (Space)")));
    studio_controls.append(&stamp_btn);

    let undo_btn = gtk::Button::from_icon_name("edit-clear-symbolic");
    undo_btn.add_css_class("circular");
    undo_btn.set_focus_on_click(false);
    // translators: Undo button in the Lyrics Sync Studio clearing the last timestamp.
    undo_btn.set_tooltip_text(Some(&gettext("Undo last line (Backspace)")));
    studio_controls.append(&undo_btn);

    let reset_btn = gtk::Button::from_icon_name("view-refresh-symbolic");
    reset_btn.add_css_class("circular");
    reset_btn.add_css_class("flat");
    reset_btn.set_focus_on_click(false);
    // translators: Reset button in the Lyrics Sync Studio clearing all line timestamps.
    reset_btn.set_tooltip_text(Some(&gettext("Clear all timestamps")));
    studio_controls.append(&reset_btn);

    sync_page.append(&studio_controls);
    mode_stack.add_named(&sync_page, Some("sync"));

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

    let session = Rc::new(RefCell::new(SyncSession::default()));
    let syncing_buffer = Rc::new(Cell::new(false));
    let refreshing_rows = Rc::new(Cell::new(false));
    let buffer = text.buffer();

    let render_sync_state: Rc<dyn Fn()> = {
        let session = Rc::clone(&session);
        let sync_list = sync_list.downgrade();
        let sync_scroller = sync_scroller.downgrade();
        let sync_progress = sync_progress.downgrade();
        let stamp_btn = stamp_btn.downgrade();
        let undo_btn = undo_btn.downgrade();
        let reset_btn = reset_btn.downgrade();
        let refreshing_rows = Rc::clone(&refreshing_rows);
        Rc::new(move || {
            let s = session.borrow();
            if let Some(lbl) = sync_progress.upgrade() {
                let text = if s.total() == 0 {
                    String::new()
                } else if s.is_complete() {
                    labels::fill(
                        // translators: Shown in Lyrics Sync Studio once all lines have timestamps.
                        gettext("All {} lines synced · ready to review"),
                        &[&s.total().to_string()],
                    )
                } else {
                    let current = (s.cursor() + 1).min(s.total());
                    labels::fill(
                        // translators: Status line in Lyrics Sync Studio. First {} is line index, second {} is total lines.
                        gettext("Line {} of {} · Press Space on each line"),
                        &[&current.to_string(), &s.total().to_string()],
                    )
                };
                lbl.set_label(&text);
            }
            if let Some(btn) = stamp_btn.upgrade() {
                btn.set_sensitive(s.cursor() < s.total());
            }
            if let Some(btn) = undo_btn.upgrade() {
                btn.set_sensitive(s.cursor() > 0);
            }
            if let Some(btn) = reset_btn.upgrade() {
                btn.set_sensitive(s.stamped_count() > 0);
            }
            let Some(list) = sync_list.upgrade() else {
                return;
            };
            refreshing_rows.set(true);
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let mut active_row: Option<gtk::ListBoxRow> = None;
            for (idx, line) in s.lines().iter().enumerate() {
                let row = gtk::ListBoxRow::new();
                let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                hbox.set_margin_top(6);
                hbox.set_margin_bottom(6);
                hbox.set_margin_start(8);
                hbox.set_margin_end(8);

                let ts_label = match line.time_ms {
                    Some(ms) => format_lrc_timestamp(ms),
                    None => "[--:--.--]".to_string(),
                };
                let time_lbl = gtk::Label::new(Some(&ts_label));
                time_lbl.add_css_class("caption");
                time_lbl.add_css_class("sync-studio__time");
                if line.time_ms.is_none() {
                    time_lbl.add_css_class("dim-label");
                }
                time_lbl.set_halign(gtk::Align::Start);

                let text_lbl = gtk::Label::new(Some(&line.text));
                text_lbl.set_halign(gtk::Align::Start);
                text_lbl.set_xalign(0.0);
                text_lbl.set_hexpand(true);
                text_lbl.set_wrap(true);

                hbox.append(&time_lbl);
                hbox.append(&text_lbl);
                row.set_child(Some(&hbox));
                if line.time_ms.is_some() {
                    row.add_css_class("sync-studio__row--stamped");
                }
                if idx == s.cursor() {
                    row.add_css_class("sync-studio__row--active");
                    active_row = Some(row.clone());
                }
                list.append(&row);
            }
            list.select_row(active_row.as_ref());
            refreshing_rows.set(false);
            if let (Some(row), Some(scroller)) = (active_row, sync_scroller.upgrade()) {
                let scroller_ref = scroller.clone();
                let weak_row = row.downgrade();
                glib::idle_add_local_once(move || {
                    if let Some(row) = weak_row.upgrade() {
                        scroll_row_into_viewport(&scroller_ref, &row);
                    }
                });
            }
        })
    };

    let sync_to_buffer: Rc<dyn Fn()> = {
        let session = Rc::clone(&session);
        let buffer = buffer.downgrade();
        let syncing_buffer = Rc::clone(&syncing_buffer);
        let render = Rc::clone(&render_sync_state);
        Rc::new(move || {
            render();
            if let Some(buffer) = buffer.upgrade() {
                syncing_buffer.set(true);
                buffer.set_text(&session.borrow().to_lrc());
                syncing_buffer.set(false);
            }
        })
    };

    let do_stamp: Rc<dyn Fn()> = {
        let session = Rc::clone(&session);
        let sync_to_buffer = Rc::clone(&sync_to_buffer);
        let playback = playback.clone();
        let sync_clock = sync_clock.downgrade();
        Rc::new(move || {
            let pos = playback.as_ref().map_or(0, |pb| (pb.position_ms)());
            if session.borrow_mut().stamp(pos) {
                if let Some(clk) = sync_clock.upgrade() {
                    clk.set_label(&format_clock_readout(pos));
                }
                sync_to_buffer();
            }
        })
    };

    let do_undo: Rc<dyn Fn()> = {
        let session = Rc::clone(&session);
        let sync_to_buffer = Rc::clone(&sync_to_buffer);
        Rc::new(move || {
            if session.borrow_mut().undo() {
                sync_to_buffer();
            }
        })
    };

    let do_reset: Rc<dyn Fn()> = {
        let session = Rc::clone(&session);
        let sync_to_buffer = Rc::clone(&sync_to_buffer);
        Rc::new(move || {
            session.borrow_mut().clear_all();
            sync_to_buffer();
        })
    };

    stamp_btn.connect_clicked({
        let do_stamp = Rc::clone(&do_stamp);
        move |_| do_stamp()
    });
    undo_btn.connect_clicked({
        let do_undo = Rc::clone(&do_undo);
        move |_| do_undo()
    });
    reset_btn.connect_clicked({
        let do_reset = Rc::clone(&do_reset);
        move |_| do_reset()
    });

    sync_list.connect_row_selected({
        let refreshing_rows = Rc::clone(&refreshing_rows);
        let session = Rc::clone(&session);
        let render = Rc::clone(&render_sync_state);
        move |_, row| {
            if refreshing_rows.get() {
                return;
            }
            if let Some(row) = row {
                let idx = row.index();
                if idx >= 0 {
                    session.borrow_mut().select_cursor(idx as usize);
                    render();
                }
            }
        }
    });

    write_btn.connect_toggled(glib::clone!(
        #[weak]
        mode_stack,
        move |btn| {
            if btn.is_active() {
                mode_stack.set_visible_child_name("write");
            }
        }
    ));

    sync_btn.connect_toggled({
        let mode_stack = mode_stack.downgrade();
        let buffer = buffer.downgrade();
        let session = Rc::clone(&session);
        let render = Rc::clone(&render_sync_state);
        move |btn| {
            if !btn.is_active() {
                return;
            }
            if let Some(buf) = buffer.upgrade() {
                let (start, end) = buf.bounds();
                let text = buf.text(&start, &end, false);
                *session.borrow_mut() = SyncSession::from_text(&text);
            }
            render();
            if let Some(stack) = mode_stack.upgrade() {
                stack.set_visible_child_name("sync");
            }
        }
    });

    // Capture-phase keyboard control in Sync Studio: Space stamps cleanly without
    // bubbling to global Play/Pause; Backspace undoes; Left rewinds 3s.
    let key_ctrl = gtk::EventControllerKey::new();
    key_ctrl.set_propagation_phase(gtk::PropagationPhase::Capture);
    key_ctrl.connect_key_pressed({
        let mode_stack = mode_stack.downgrade();
        let busy = Rc::clone(&busy);
        let do_stamp = Rc::clone(&do_stamp);
        let do_undo = Rc::clone(&do_undo);
        let playback = playback.clone();
        move |_, key, _, state| {
            let on_sync = mode_stack
                .upgrade()
                .and_then(|s| s.visible_child_name())
                .is_some_and(|n| n == "sync");
            if !on_sync || busy.get() {
                return glib::Propagation::Proceed;
            }
            let has_mod = state.intersects(
                gtk::gdk::ModifierType::CONTROL_MASK
                    | gtk::gdk::ModifierType::ALT_MASK
                    | gtk::gdk::ModifierType::SUPER_MASK,
            );
            if has_mod {
                return glib::Propagation::Proceed;
            }
            match key {
                gtk::gdk::Key::space => {
                    do_stamp();
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::BackSpace => {
                    do_undo();
                    glib::Propagation::Stop
                }
                gtk::gdk::Key::Left => {
                    if let Some(pb) = &playback {
                        let target = (pb.position_ms)()
                            .saturating_sub(3000)
                            .min(u64::from(u32::MAX)) as u32;
                        (pb.seek_ms)(target);
                    }
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        }
    });
    dialog.add_controller(key_ctrl);

    if let Some(pb) = playback.clone() {
        let dialog_tick = dialog.downgrade();
        let stack_tick = mode_stack.downgrade();
        let clock_tick = sync_clock.downgrade();
        let play_tick = play_btn.as_ref().map(|b| b.downgrade());
        glib::timeout_add_local(Duration::from_millis(60), move || {
            let Some(_) = dialog_tick.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let on_sync = stack_tick
                .upgrade()
                .and_then(|s| s.visible_child_name())
                .is_some_and(|n| n == "sync");
            if on_sync {
                if let Some(clk) = clock_tick.upgrade() {
                    clk.set_label(&format_clock_readout((pb.position_ms)()));
                }
                if let Some(Some(btn)) = play_tick.as_ref().map(|w| w.upgrade()) {
                    btn.set_icon_name(if (pb.is_playing)() {
                        "media-playback-pause-symbolic"
                    } else {
                        "media-playback-start-symbolic"
                    });
                }
            }
            glib::ControlFlow::Continue
        });
    }

    let busy_changed = Rc::clone(&busy);
    let syncing_changed = Rc::clone(&syncing_buffer);
    buffer.connect_changed(glib::clone!(
        #[weak]
        review,
        #[weak]
        sync_btn,
        move |buffer| {
            let (start, end) = buffer.bounds();
            let raw = buffer.text(&start, &end, false);
            let lines = parse_draft(&raw).lines;
            let ready = !busy_changed.get() && lines > 0;
            review.set_sensitive(ready);
            if !syncing_changed.get() {
                sync_btn.set_sensitive(!busy_changed.get() && lines > 0);
            }
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

fn scroll_row_into_viewport(scroller: &gtk::ScrolledWindow, row: &gtk::ListBoxRow) {
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
pub fn studio_controls_for_tests(
    dialog: &libadwaita::Dialog,
) -> Option<(gtk::ToggleButton, gtk::Button, gtk::Button)> {
    let mut stack = vec![dialog.clone().upcast::<gtk::Widget>()];
    let mut sync_btn = None;
    let mut stamp_btn = None;
    let mut undo_btn = None;
    let sync_label = gettext("Sync While Listening");
    let stamp_label = gettext("Stamp Line");
    while let Some(widget) = stack.pop() {
        if let Ok(btn) = widget.clone().downcast::<gtk::ToggleButton>() {
            if btn.label().as_deref() == Some(sync_label.as_str()) {
                sync_btn = Some(btn);
            }
        } else if let Ok(btn) = widget.clone().downcast::<gtk::Button>() {
            if btn.label().as_deref() == Some(stamp_label.as_str()) {
                stamp_btn = Some(btn);
            } else if btn.icon_name().as_deref() == Some("edit-clear-symbolic") {
                undo_btn = Some(btn);
            }
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            stack.push(c);
        }
    }
    Some((sync_btn?, stamp_btn?, undo_btn?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_lrc_timestamps_with_centiseconds() {
        assert_eq!(format_lrc_timestamp(0), "[00:00.00]");
        assert_eq!(format_lrc_timestamp(12_400), "[00:12.40]");
        assert_eq!(format_lrc_timestamp(125_099), "[02:05.09]");
    }

    #[test]
    fn sync_session_stamps_monotonically_undoes_and_exports_valid_lrc() {
        let mut session =
            SyncSession::from_text("[ar: Niink]\nFirst line\n\nSecond line\nThird line");
        assert_eq!(session.total(), 3);
        assert_eq!(session.cursor(), 0);
        assert_eq!(session.stamped_count(), 0);
        assert!(!session.is_complete());

        assert!(session.stamp(12_400));
        assert!(session.stamp(10_000)); // clamped up to 12_400 (monotonic)
        assert_eq!(session.cursor(), 2);
        assert_eq!(session.lines()[1].time_ms, Some(12_400));

        assert!(session.undo());
        assert_eq!(session.cursor(), 1);
        assert_eq!(session.lines()[1].time_ms, None);

        assert!(session.stamp(15_500));
        assert!(session.stamp(19_000));
        assert!(session.is_complete());
        assert!(!session.stamp(20_000));

        let lrc = session.to_lrc();
        assert_eq!(
            lrc,
            "[00:12.40] First line\n[00:15.50] Second line\n[00:19.00] Third line"
        );
        let draft = parse_draft(&lrc);
        assert_eq!(draft.lines, 3);
        assert_eq!(draft.synced.as_deref(), Some(lrc.as_str()));

        session.select_cursor(1);
        assert_eq!(session.cursor(), 1);
        session.clear_all();
        assert_eq!(session.stamped_count(), 0);
        assert_eq!(session.cursor(), 0);
    }

    #[test]
    fn sync_session_resumes_from_partial_timestamps() {
        let session = SyncSession::from_text("[00:10.00] First\nSecond");
        assert_eq!(session.stamped_count(), 1);
        assert_eq!(session.cursor(), 1);
        assert_eq!(session.lines()[0].time_ms, Some(10_000));
        assert_eq!(session.lines()[1].time_ms, None);
    }

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
