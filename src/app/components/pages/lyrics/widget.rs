use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::time::Duration;

use canto_api::{Lyrics, LyricsSource};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/io/github/gusthanks/Canto/components/lyrics_page.ui")]
    pub struct LyricsPageWidget {
        #[template_child]
        pub lyrics_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub loading_page: TemplateChild<libadwaita::StatusPage>,
        #[template_child]
        pub lyrics_scroller: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub lyrics_column: TemplateChild<gtk::Box>,
        #[template_child]
        pub lyrics_source: TemplateChild<gtk::Label>,
        #[template_child]
        pub lyrics_container: TemplateChild<gtk::Box>,
        #[template_child]
        pub empty_page: TemplateChild<libadwaita::StatusPage>,
        #[template_child]
        pub reload_empty_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub sync_toolbar: TemplateChild<gtk::Box>,
        #[template_child]
        pub offset_minus_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub offset_reset_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub offset_plus_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub sync_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub reload_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub web_search_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub add_lyrics_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub fullscreen_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub translate_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub translate_language_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub highlight_toggle: TemplateChild<libadwaita::ToggleGroup>,
        #[template_child]
        pub scroll_toggle: TemplateChild<libadwaita::ToggleGroup>,
        #[template_child]
        pub back_to_line_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub back_to_line_button: TemplateChild<gtk::Button>,

        pub line_widgets: RefCell<Vec<gtk::Widget>>,
        /// Where the shown lyrics came from (`None` while none are shown).
        pub source: RefCell<Option<LyricsSource>>,
        pub active_index: RefCell<Option<usize>>,
        /// The synced lines' text, for the word-by-word highlight.
        pub line_texts: RefCell<Vec<String>>,
        /// How much of the active line is lit, in bytes (`usize::MAX`: not
        /// drawn yet).
        pub words_lit: Cell<usize>,
        pub scroll_anim: RefCell<Option<libadwaita::TimedAnimation>>,
        /// The whole line lights at once instead of word by word.
        pub whole_line: Cell<bool>,
        /// After a scroll by hand the lyrics stay until asked back.
        pub stay_put: Cell<bool>,
        /// Scrolled by hand: the lyrics don't follow the song for now.
        pub detached: Cell<bool>,
        /// Brings the lyrics back a moment after the last scroll by hand.
        pub reattach_source: RefCell<Option<glib::SourceId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LyricsPageWidget {
        const NAME: &'static str = "LyricsPageWidget";
        type Type = super::LyricsPageWidget;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LyricsPageWidget {}
    impl WidgetImpl for LyricsPageWidget {}
    impl BinImpl for LyricsPageWidget {}
}

glib::wrapper! {
    pub struct LyricsPageWidget(ObjectSubclass<imp::LyricsPageWidget>)
        @extends gtk::Widget, libadwaita::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LyricsPageWidget {
    fn default() -> Self {
        let widget: Self = glib::Object::new();
        let imp = widget.imp();
        let vadjust = imp.lyrics_scroller.vadjustment();
        let weak_widget = widget.downgrade();
        vadjust.connect_page_size_notify(move |_| {
            if let Some(w) = weak_widget.upgrade() {
                let active = *w.imp().active_index.borrow();
                if let Some(idx) = active {
                    let line_widgets = w.imp().line_widgets.borrow();
                    if let Some(lw) = line_widgets.get(idx) {
                        w.scroll_to_line(lw, false);
                    }
                }
            }
        });

        // Any scroll by hand: wheel or touchpad, dragging the scrollbar,
        // dragging the lyrics on a touch screen
        let scroll_controller =
            gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let weak_widget = widget.downgrade();
        scroll_controller.connect_scroll(move |_, _, dy| {
            if let Some(w) = weak_widget.upgrade().filter(|_| dy != 0.0) {
                w.detach();
            }
            glib::Propagation::Proceed
        });
        imp.lyrics_scroller.add_controller(scroll_controller);
        let bar_drag = gtk::GestureDrag::new();
        bar_drag.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak_widget = widget.downgrade();
        bar_drag.connect_drag_begin(move |_, _, _| {
            if let Some(w) = weak_widget.upgrade() {
                w.detach();
            }
        });
        imp.lyrics_scroller.vscrollbar().add_controller(bar_drag);
        let touch_drag = gtk::GestureDrag::new();
        touch_drag.set_touch_only(true);
        touch_drag.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak_widget = widget.downgrade();
        touch_drag.connect_drag_update(move |_, _, dy| {
            if let Some(w) = weak_widget.upgrade().filter(|_| dy.abs() > 8.0) {
                w.detach();
            }
        });
        imp.lyrics_scroller.add_controller(touch_drag);
        let weak_widget = widget.downgrade();
        imp.back_to_line_button.connect_clicked(move |_| {
            if let Some(w) = weak_widget.upgrade() {
                w.reattach();
            }
        });

        widget
    }
}

pub fn open_external_url(widget: &impl IsA<gtk::Widget>, url: &str) {
    if open::that_detached(url).is_err() {
        let parent = widget.root().and_then(|r| r.downcast::<gtk::Window>().ok());
        let launcher = gtk::UriLauncher::new(url);
        launcher.launch(parent.as_ref(), None::<&gio::Cancellable>, |_| {});
    }
}

impl LyricsPageWidget {
    /// The full-screen view hides its own way in.
    pub fn set_fullscreen_button_visible(&self, visible: bool) {
        self.imp().fullscreen_button.set_visible(visible);
    }

    pub fn set_loading(&self) {
        let imp = self.imp();
        self.forget_source();
        imp.reload_empty_button.set_visible(false);
        imp.sync_toolbar.set_visible(false);
        imp.lyrics_stack.set_visible_child_name("loading");
    }

    pub fn set_empty(&self, title: &str, description: Option<&str>, allow_reload: bool) {
        let imp = self.imp();
        self.forget_source();
        imp.empty_page.set_title(title);
        imp.empty_page.set_description(description);
        imp.reload_empty_button.set_visible(allow_reload);
        // A song was looked up and no source had its lyrics: the web may
        imp.web_search_button.set_visible(allow_reload);
        // ...or the listener, who can add them to LRCLIB
        imp.add_lyrics_button.set_visible(allow_reload);
        imp.sync_toolbar.set_visible(false);
        imp.lyrics_stack.set_visible_child_name("empty");
    }

    pub fn set_offset_ms(&self, offset_ms: i32) {
        let text = if offset_ms > 0 {
            format!("+{offset_ms} ms")
        } else {
            format!("{offset_ms} ms")
        };
        self.imp().offset_reset_button.set_label(&text);
        let sync = &self.imp().sync_button;
        if offset_ms == 0 {
            sync.remove_css_class("lyric-sync-button--shifted");
            sync.set_tooltip_text(Some(&gettext("Lyrics Timing")));
        } else {
            sync.add_css_class("lyric-sync-button--shifted");
            // translators: Tooltip of the lyrics timing button while an offset is set. {} is like "+200 ms".
            sync.set_tooltip_text(Some(&gettext("Lyrics Timing ({})").replace("{}", &text)));
        }
    }

    pub fn connect_offset_minus<F: Fn() + 'static>(&self, f: F) {
        self.imp().offset_minus_button.connect_clicked(move |_| f());
    }

    pub fn connect_offset_plus<F: Fn() + 'static>(&self, f: F) {
        self.imp().offset_plus_button.connect_clicked(move |_| f());
    }

    pub fn connect_offset_reset<F: Fn() + 'static>(&self, f: F) {
        self.imp().offset_reset_button.connect_clicked(move |_| f());
    }

    /// The button shown when no source had the lyrics.
    pub fn connect_web_search<F: Fn(&gtk::Button) + 'static>(&self, f: F) {
        self.imp().web_search_button.connect_clicked(move |b| f(b));
    }

    /// The button that opens "Add Lyrics", shown when no source had them.
    pub fn connect_add_lyrics<F: Fn(&gtk::Button) + 'static>(&self, f: F) {
        self.imp().add_lyrics_button.connect_clicked(move |b| f(b));
    }

    /// Looks the lyrics up again, as the reload button does (after the
    /// listener published some).
    pub fn reload(&self) {
        self.imp().reload_empty_button.emit_clicked();
    }

    pub fn connect_reload<F: Fn() + Clone + 'static>(&self, f: F) {
        let f2 = f.clone();
        self.imp().reload_button.connect_clicked(move |_| f());
        self.imp()
            .reload_empty_button
            .connect_clicked(move |_| f2());
    }

    /// Shows `lyrics`, with one quiet line above them saying where they
    /// came from (`source`).
    pub fn set_lyrics<F>(&self, lyrics: &Lyrics, source: &LyricsSource, on_seek: F)
    where
        F: Fn(u32) + Clone + 'static,
    {
        let imp = self.imp();
        imp.lyrics_source
            .set_text(&source_line(lyrics.is_synced(), source));
        imp.lyrics_source.set_visible(true);
        imp.source.replace(Some(source.clone()));
        if let Some(anim) = imp.scroll_anim.borrow_mut().take() {
            anim.pause();
        }
        self.forget_detach();
        while let Some(child) = imp.lyrics_container.first_child() {
            imp.lyrics_container.remove(&child);
        }
        imp.line_widgets.borrow_mut().clear();
        imp.line_texts.borrow_mut().clear();
        imp.words_lit.set(usize::MAX);
        imp.active_index.replace(None);

        let is_synced = match lyrics {
            Lyrics::Synced(lines) => {
                let mut widgets = Vec::with_capacity(lines.len());
                for line in lines {
                    let btn = gtk::Button::new();
                    btn.add_css_class("flat");
                    btn.add_css_class("lyric-line");

                    let label = gtk::Label::new(Some(&line.text));
                    label.set_wrap(true);
                    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                    label.set_halign(gtk::Align::Start);
                    label.set_xalign(0.0);
                    btn.set_child(Some(&line_with_translation(&label)));

                    let seek_ms = line.start_time_ms.min(u64::from(u32::MAX)) as u32;
                    let on_seek_clone = on_seek.clone();
                    let weak_self = self.downgrade();
                    btn.connect_clicked(move |_| {
                        // Jumping to a line means following the song again
                        if let Some(w) = weak_self.upgrade() {
                            w.forget_detach();
                        }
                        on_seek_clone(seek_ms);
                    });

                    imp.lyrics_container.append(&btn);
                    widgets.push(btn.upcast());
                }
                *imp.line_widgets.borrow_mut() = widgets;
                *imp.line_texts.borrow_mut() = lines.iter().map(|l| l.text.clone()).collect();
                true
            }
            Lyrics::Plain(lines) => {
                let mut widgets = Vec::with_capacity(lines.len());
                for text in lines {
                    let label = gtk::Label::new(Some(text));
                    label.set_wrap(true);
                    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                    label.set_halign(gtk::Align::Start);
                    label.set_xalign(0.0);
                    let line = line_with_translation(&label);
                    line.add_css_class("lyric-line");
                    line.add_css_class("plain");

                    imp.lyrics_container.append(&line);
                    widgets.push(line.upcast());
                }
                *imp.line_widgets.borrow_mut() = widgets;
                false
            }
        };

        imp.sync_button.set_visible(is_synced);
        imp.sync_toolbar.set_visible(true);
        imp.lyrics_stack.set_visible_child_name("list");
        imp.lyrics_scroller.vadjustment().set_value(0.0);
    }

    /// Where the shown lyrics came from (`None` while none are shown).
    pub fn source(&self) -> Option<LyricsSource> {
        self.imp().source.borrow().clone()
    }

    /// No lyrics shown: no line saying where they came from.
    fn forget_source(&self) {
        let imp = self.imp();
        imp.source.replace(None);
        imp.lyrics_source.set_visible(false);
        imp.lyrics_source.set_text("");
    }

    /// The line above the lyrics saying where they came from (for tests).
    #[cfg(test)]
    pub fn source_label(&self) -> gtk::Label {
        self.imp().lyrics_source.get()
    }

    /// The label of lyric line `index` (for tests).
    #[cfg(test)]
    pub fn line_label_at(&self, index: usize) -> Option<gtk::Label> {
        self.line_label(index)
    }

    /// The active line and how much of it is sung, in bytes, lighting its
    /// words one by one (unless the whole line lights at once).
    pub fn set_position(&self, active: Option<(usize, usize)>) {
        let index = active.map(|(index, _)| index);
        self.set_active_line(index);
        let Some((index, lit)) = active else {
            return;
        };
        let imp = self.imp();
        if imp.whole_line.get() {
            return;
        }
        let texts = imp.line_texts.borrow();
        let Some(text) = texts.get(index) else {
            return;
        };
        let lit = lit.min(text.len());
        if lit == imp.words_lit.replace(lit) {
            return;
        }
        if let Some(label) = self.line_label(index) {
            label.set_markup(&karaoke_markup(text, lit));
        }
    }

    /// The box holding a line's label and its translation.
    fn line_box(&self, index: usize) -> Option<gtk::Box> {
        let widgets = self.imp().line_widgets.borrow();
        let widget = widgets.get(index)?;
        match widget.downcast_ref::<gtk::Button>() {
            Some(button) => button.child().and_downcast::<gtk::Box>(),
            None => widget.clone().downcast::<gtk::Box>().ok(),
        }
    }

    fn line_label(&self, index: usize) -> Option<gtk::Label> {
        let head = child_with_class(self.line_box(index)?.upcast_ref(), "lyric-line__head")?;
        head.first_child().and_downcast::<gtk::Label>()
    }

    /// Shows `lines[i]` under lyric line `i` (`None` everywhere hides them).
    pub fn set_translations(&self, lines: Option<&[Option<String>]>) {
        let count = self.imp().line_widgets.borrow().len();
        for index in 0..count {
            let Some(translation) = self
                .line_box(index)
                .and_then(|b| child_with_class(b.upcast_ref(), "lyric-translation"))
                .and_downcast::<gtk::Label>()
            else {
                continue;
            };
            let text = lines.and_then(|l| l.get(index)).and_then(|t| t.as_deref());
            translation.set_text(text.unwrap_or(""));
            translation.set_visible(text.is_some());
        }
    }

    /// The text of each shown lyric line, in order.
    pub fn line_texts(&self) -> Vec<String> {
        let count = self.imp().line_widgets.borrow().len();
        (0..count)
            .map(|i| {
                self.line_label(i)
                    .map(|l| l.text().to_string())
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Marks the lines Genius explains: a small button on the line opens the
    /// explanation (`None` everywhere removes them).
    pub fn set_annotations(&self, notes: Option<&[Option<String>]>) {
        let count = self.imp().line_widgets.borrow().len();
        for index in 0..count {
            let Some(line) = self.line_box(index) else {
                continue;
            };
            let Some(head) =
                child_with_class(line.upcast_ref(), "lyric-line__head").and_downcast::<gtk::Box>()
            else {
                continue;
            };
            let Some(text) = child_with_class(line.upcast_ref(), "lyric-annotation__text")
                .and_downcast::<gtk::Label>()
            else {
                continue;
            };
            if let Some(old) = child_with_class(head.upcast_ref(), "lyric-annotation") {
                head.remove(&old);
            }
            text.set_visible(false);
            let Some(note) = notes.and_then(|n| n.get(index)).and_then(|n| n.as_deref()) else {
                continue;
            };
            text.set_text(note);
            // An icon, not a button: the line itself is a button (it seeks),
            // and a button inside it never got the click
            let mark = gtk::Image::from_icon_name("canto-genius-symbolic");
            mark.set_valign(gtk::Align::Center);
            mark.add_css_class("lyric-annotation");
            // translators: Tooltip of the mark on a lyric line that Genius explains.
            mark.set_tooltip_text(Some(&gettext("What This Line Means")));
            mark.set_cursor_from_name(Some("pointer"));
            let click = gtk::GestureClick::new();
            click.connect_pressed(glib::clone!(
                #[weak]
                text,
                move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    text.set_visible(!text.is_visible());
                }
            ));
            mark.add_controller(click);
            head.append(&mark);
        }
    }

    /// Shows the translate button pressed or not.
    pub fn set_translate_active(&self, active: bool) {
        self.imp().translate_button.set_active(active);
    }

    /// A list of languages under the translate button; `f` gets the code
    /// picked ("auto" for the interface's language).
    pub fn connect_language_picked<F: Fn(&str) + 'static>(&self, f: F) {
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("navigation-sidebar");
        let mut codes = vec!["auto"];
        // translators: First entry of the lyrics translation languages: the app's own language.
        let mut names = vec![gettext("App Language")];
        for (code, name) in TRANSLATION_LANGUAGES {
            codes.push(code);
            names.push(name.to_string());
        }
        for name in &names {
            let label = gtk::Label::new(Some(name));
            label.set_xalign(0.0);
            list.append(&label);
        }
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroller.set_propagate_natural_height(true);
        scroller.set_max_content_height(360);
        scroller.set_min_content_width(200);
        scroller.set_child(Some(&list));
        let popover = gtk::Popover::new();
        popover.set_child(Some(&scroller));
        // The arrow points up while the list is open
        let button = self.imp().translate_language_button.downgrade();
        popover.connect_visible_notify(move |popover| {
            let Some(button) = button.upgrade() else {
                return;
            };
            button.set_icon_name(if popover.is_visible() {
                "pan-up-symbolic"
            } else {
                "pan-down-symbolic"
            });
        });
        list.connect_row_activated(glib::clone!(
            #[weak]
            popover,
            move |_, row| {
                if let Some(code) = codes.get(row.index() as usize) {
                    f(code);
                }
                popover.popdown();
            }
        ));
        self.imp()
            .translate_language_button
            .set_popover(Some(&popover));
    }

    pub fn connect_translate_toggled<F: Fn(bool) + 'static>(&self, f: F) {
        self.imp()
            .translate_button
            .connect_toggled(move |button| f(button.is_active()));
    }

    pub fn set_active_line(&self, new_index: Option<usize>) {
        let imp = self.imp();
        let mut active_cell = imp.active_index.borrow_mut();
        if *active_cell == new_index {
            return;
        }
        // The line left behind shows plain again
        if let Some(previous) = *active_cell {
            if let (Some(label), Some(text)) = (
                self.line_label(previous),
                imp.line_texts.borrow().get(previous),
            ) {
                label.set_text(text);
            }
        }
        imp.words_lit.set(usize::MAX);

        let widgets = imp.line_widgets.borrow();

        for (idx, w) in widgets.iter().enumerate() {
            let is_active = Some(idx) == new_index;
            let is_past = new_index.is_some_and(|cur| idx < cur);
            if is_active {
                w.add_css_class("active");
            } else {
                w.remove_css_class("active");
            }
            if is_past {
                w.add_css_class("past");
            } else {
                w.remove_css_class("past");
            }
        }

        *active_cell = new_index;

        if let Some(idx) = new_index {
            if let Some(w) = widgets.get(idx) {
                self.scroll_to_line(w, true);
            }
        }
    }

    /// Word by word (the default) or the whole line at once.
    pub fn set_word_by_word(&self, on: bool) {
        let imp = self.imp();
        imp.whole_line.set(!on);
        let name = if on { "word" } else { "line" };
        if imp.highlight_toggle.active_name().as_deref() != Some(name) {
            imp.highlight_toggle.set_active_name(Some(name));
        }
        // Redraw the active line from scratch on the next tick
        if let Some(index) = *imp.active_index.borrow() {
            if let (Some(label), Some(text)) =
                (self.line_label(index), imp.line_texts.borrow().get(index))
            {
                label.set_text(text);
            }
        }
        imp.words_lit.set(usize::MAX);
    }

    pub fn connect_word_by_word_picked<F: Fn(bool) + 'static>(&self, f: F) {
        self.imp()
            .highlight_toggle
            .connect_active_name_notify(move |group| {
                f(group.active_name().as_deref() != Some("line"))
            });
    }

    /// Whether lyrics scrolled by hand stay there until asked back.
    pub fn set_stay_put(&self, on: bool) {
        let imp = self.imp();
        imp.stay_put.set(on);
        let name = if on { "free" } else { "follow" };
        if imp.scroll_toggle.active_name().as_deref() != Some(name) {
            imp.scroll_toggle.set_active_name(Some(name));
        }
        if imp.detached.get() {
            // Re-arm (or drop) the way back for the new choice
            self.detach();
        }
    }

    pub fn connect_stay_put_picked<F: Fn(bool) + 'static>(&self, f: F) {
        self.imp()
            .scroll_toggle
            .connect_active_name_notify(move |group| {
                f(group.active_name().as_deref() == Some("free"))
            });
    }

    /// Scrolled by hand: stop following the song and offer the way back;
    /// unless it stays put, come back by itself a moment later.
    pub(crate) fn detach(&self) {
        let imp = self.imp();
        if imp.line_texts.borrow().is_empty() {
            return;
        }
        imp.detached.set(true);
        if let Some(anim) = imp.scroll_anim.borrow_mut().take() {
            anim.pause();
        }
        imp.back_to_line_revealer.set_reveal_child(true);
        if let Some(source) = imp.reattach_source.take() {
            source.remove();
        }
        if !imp.stay_put.get() {
            let weak = self.downgrade();
            let source = glib::timeout_add_local_once(Duration::from_secs(3), move || {
                if let Some(w) = weak.upgrade() {
                    // This source is done; forget it without removing it
                    drop(w.imp().reattach_source.take());
                    w.reattach();
                }
            });
            imp.reattach_source.replace(Some(source));
        }
    }

    /// Follow the song again, scrolling back to the line being sung.
    pub fn reattach(&self) {
        self.forget_detach();
        let imp = self.imp();
        let active = *imp.active_index.borrow();
        if let Some(widget) = active.and_then(|i| imp.line_widgets.borrow().get(i).cloned()) {
            self.scroll_to_line(&widget, true);
        }
    }

    fn forget_detach(&self) {
        let imp = self.imp();
        imp.detached.set(false);
        imp.back_to_line_revealer.set_reveal_child(false);
        if let Some(source) = imp.reattach_source.take() {
            source.remove();
        }
    }

    /// Whether the lyrics stopped following the song (for tests).
    #[cfg(test)]
    pub fn is_detached(&self) -> bool {
        self.imp().detached.get()
    }

    /// What line `index` shows, with its markup (for tests).
    #[cfg(test)]
    pub fn line_markup(&self, index: usize) -> Option<String> {
        self.line_label(index)
            .map(|label| label.label().to_string())
    }

    #[allow(deprecated)]
    fn scroll_to_line(&self, widget: &gtk::Widget, animate: bool) {
        let imp = self.imp();
        if imp.detached.get() {
            return;
        }
        let vadjust = imp.lyrics_scroller.vadjustment();
        // From the top of the scrolled column, so the line above the lyrics
        // scrolls away without moving where the sung line sits
        if let Some((_, y)) = widget.translate_coordinates(&*imp.lyrics_column, 0.0, 0.0) {
            let line_h = widget.height() as f64;
            let scroller_h = imp.lyrics_scroller.height() as f64;
            if scroller_h <= 1.0 {
                return;
            }
            let max_val = (vadjust.upper() - vadjust.page_size()).max(0.0);
            let target = (y - scroller_h * 0.38 + line_h * 0.5).clamp(0.0, max_val);
            let current = vadjust.value();
            if (current - target).abs() <= 1.0 {
                return;
            }

            if let Some(prev) = imp.scroll_anim.borrow_mut().take() {
                prev.pause();
            }

            if !animate || !self.is_mapped() {
                vadjust.set_value(target);
                return;
            }

            let target_fn = libadwaita::CallbackAnimationTarget::new(glib::clone!(
                #[weak]
                vadjust,
                move |val| vadjust.set_value(val)
            ));
            let anim = libadwaita::TimedAnimation::new(
                &*imp.lyrics_scroller,
                current,
                target,
                260,
                target_fn,
            );
            anim.set_easing(libadwaita::Easing::EaseOutCubic);
            anim.play();
            *imp.scroll_anim.borrow_mut() = Some(anim);
        }
    }
}

/// The line above the lyrics saying where they came from, and whether they
/// follow the song.
fn source_line(synced: bool, source: &LyricsSource) -> String {
    if synced {
        // translators: Quiet line above the lyrics saying where they came from. {} is the source's name, "LRCLIB" or "Genius" (keep it as is).
        gettext("Lyrics: {}").replace("{}", source.name())
    } else {
        // translators: Quiet line above lyrics that have no timing, so they don't follow the song. {} is the source's name, "LRCLIB" or "Genius" (keep it as is).
        gettext("Lyrics: {} · not synced").replace("{}", source.name())
    }
}

/// The line with its first `lit` bytes bright and the rest dim.
pub fn karaoke_markup(text: &str, lit: usize) -> String {
    let lit = if text.is_char_boundary(lit) {
        lit
    } else {
        text.len()
    };
    let (sung, rest) = text.split_at(lit);
    if rest.trim().is_empty() {
        return glib::markup_escape_text(text).to_string();
    }
    format!(
        "{}<span alpha=\"45%\">{}</span>",
        glib::markup_escape_text(sung),
        glib::markup_escape_text(rest)
    )
}

/// Languages lyrics can be translated into, by their own names.
const TRANSLATION_LANGUAGES: &[(&str, &str)] = &[
    ("pt", "Português"),
    ("en", "English"),
    ("es", "Español"),
    ("fr", "Français"),
    ("it", "Italiano"),
    ("de", "Deutsch"),
    ("nl", "Nederlands"),
    ("sv", "Svenska"),
    ("pl", "Polski"),
    ("tr", "Türkçe"),
    ("el", "Ελληνικά"),
    ("ru", "Русский"),
    ("uk", "Українська"),
    ("ar", "العربية"),
    ("he", "עברית"),
    ("hi", "हिन्दी"),
    ("id", "Bahasa Indonesia"),
    ("vi", "Tiếng Việt"),
    ("th", "ไทย"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("zh-CN", "中文 (简体)"),
    ("zh-TW", "中文 (繁體)"),
    ("yo", "Yorùbá"),
    ("sw", "Kiswahili"),
    ("la", "Latina"),
];

/// A lyric line: its label (with room for a Genius mark on the right), a
/// hidden translation under it and a hidden Genius explanation under that.
fn line_with_translation(label: &gtk::Label) -> gtk::Box {
    let line = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    head.add_css_class("lyric-line__head");
    label.set_hexpand(false);
    head.append(label);
    line.append(&head);
    let translation = gtk::Label::new(None);
    translation.set_wrap(true);
    translation.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    translation.set_halign(gtk::Align::Start);
    translation.set_xalign(0.0);
    translation.set_visible(false);
    translation.add_css_class("lyric-translation");
    line.append(&translation);
    let note = gtk::Label::new(None);
    note.set_wrap(true);
    note.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    note.set_halign(gtk::Align::Start);
    note.set_xalign(0.0);
    note.set_visible(false);
    note.add_css_class("lyric-annotation__text");
    line.append(&note);
    line
}

/// The child of `parent` that has `class`.
fn child_with_class(parent: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
    let mut child = parent.first_child();
    while let Some(widget) = child {
        if widget.has_css_class(class) {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}

pub fn expose_widgets() {
    LyricsPageWidget::static_type();
}
