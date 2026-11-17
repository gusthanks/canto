use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use riff_api::Lyrics;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/lyrics_page.ui")]
    pub struct LyricsPageWidget {
        #[template_child]
        pub lyrics_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub loading_page: TemplateChild<libadwaita::StatusPage>,
        #[template_child]
        pub lyrics_scroller: TemplateChild<gtk::ScrolledWindow>,
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
        pub reload_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub fullscreen_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub translate_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub translate_language_button: TemplateChild<gtk::MenuButton>,

        pub line_widgets: RefCell<Vec<gtk::Widget>>,
        pub active_index: RefCell<Option<usize>>,
        /// The synced lines' text, for the word-by-word highlight.
        pub line_texts: RefCell<Vec<String>>,
        /// How many words of the active line are lit.
        pub words_lit: Cell<usize>,
        pub scroll_anim: RefCell<Option<libadwaita::TimedAnimation>>,
        pub manual_scroll_until: Cell<Option<Instant>>,
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

        let scroll_controller =
            gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let weak_widget = widget.downgrade();
        scroll_controller.connect_scroll(move |_, _, _| {
            if let Some(w) = weak_widget.upgrade() {
                w.imp()
                    .manual_scroll_until
                    .set(Some(Instant::now() + Duration::from_secs(3)));
                if let Some(anim) = w.imp().scroll_anim.borrow_mut().take() {
                    anim.pause();
                }
            }
            glib::Propagation::Proceed
        });
        imp.lyrics_scroller.add_controller(scroll_controller);

        widget
    }
}

pub fn open_external_url(widget: &impl IsA<gtk::Widget>, url: &str) {
    if open::that_detached(url).is_err() {
        let parent = widget
            .root()
            .and_then(|r| r.downcast::<gtk::Window>().ok());
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
        imp.reload_empty_button.set_visible(false);
        imp.sync_toolbar.set_visible(false);
        imp.lyrics_stack.set_visible_child_name("loading");
    }

    pub fn set_empty(&self, title: &str, description: Option<&str>, allow_reload: bool) {
        let imp = self.imp();
        imp.empty_page.set_title(title);
        imp.empty_page.set_description(description);
        imp.reload_empty_button.set_visible(allow_reload);
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

    pub fn connect_reload<F: Fn() + Clone + 'static>(&self, f: F) {
        let f2 = f.clone();
        self.imp().reload_button.connect_clicked(move |_| f());
        self.imp()
            .reload_empty_button
            .connect_clicked(move |_| f2());
    }

    pub fn set_lyrics<F>(&self, lyrics: &Lyrics, on_seek: F)
    where
        F: Fn(u32) + Clone + 'static,
    {
        let imp = self.imp();
        if let Some(anim) = imp.scroll_anim.borrow_mut().take() {
            anim.pause();
        }
        imp.manual_scroll_until.set(None);
        while let Some(child) = imp.lyrics_container.first_child() {
            imp.lyrics_container.remove(&child);
        }
        imp.line_widgets.borrow_mut().clear();
        imp.line_texts.borrow_mut().clear();
        imp.words_lit.set(0);
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
                    btn.connect_clicked(move |_| {
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

        imp.offset_minus_button.set_visible(is_synced);
        imp.offset_reset_button.set_visible(is_synced);
        imp.offset_plus_button.set_visible(is_synced);
        imp.sync_toolbar.set_visible(true);
        imp.lyrics_stack.set_visible_child_name("list");
        imp.lyrics_scroller.vadjustment().set_value(0.0);
    }

    /// The active line and how far into it the song is (0 to 1), lighting
    /// its words one by one (an estimate: LRC lyrics time lines, not words).
    pub fn set_position(&self, active: Option<(usize, f64)>) {
        let index = active.map(|(index, _)| index);
        self.set_active_line(index);
        let Some((index, progress)) = active else {
            return;
        };
        let imp = self.imp();
        let texts = imp.line_texts.borrow();
        let Some(text) = texts.get(index) else {
            return;
        };
        let lit = riff_api::lyrics::model::words_reached(text, progress);
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
        self.line_box(index)?.first_child().and_downcast::<gtk::Label>()
    }

    /// Shows `lines[i]` under lyric line `i` (`None` everywhere hides them).
    pub fn set_translations(&self, lines: Option<&[Option<String>]>) {
        let count = self.imp().line_widgets.borrow().len();
        for index in 0..count {
            let Some(translation) = self
                .line_box(index)
                .and_then(|b| b.last_child())
                .and_downcast::<gtk::Label>()
                .filter(|l| l.has_css_class("lyric-translation"))
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
            .map(|i| self.line_label(i).map(|l| l.text().to_string()).unwrap_or_default())
            .collect()
    }

    /// Marks the lines Genius explains: a small button on the line opens the
    /// explanation (`None` everywhere removes them).
    pub fn set_annotations(&self, notes: Option<&[Option<String>]>) {
        let count = self.imp().line_widgets.borrow().len();
        for index in 0..count {
            let Some(line) = self.line_box(index) else { continue };
            // The line box: label, translation, then the annotation button
            let existing = line
                .last_child()
                .and_downcast::<gtk::MenuButton>()
                .filter(|b| b.has_css_class("lyric-annotation"));
            let note = notes.and_then(|n| n.get(index)).and_then(|n| n.as_deref());
            match (note, existing) {
                (None, Some(button)) => line.remove(&button),
                (Some(note), existing) => {
                    let button = existing.unwrap_or_else(|| {
                        let button = gtk::MenuButton::new();
                        button.set_icon_name("canto-genius-symbolic");
                        button.set_halign(gtk::Align::Start);
                        button.add_css_class("flat");
                        button.add_css_class("circular");
                        button.add_css_class("lyric-annotation");
                        // translators: Tooltip of the button on a lyric line that Genius explains.
                        button.set_tooltip_text(Some(&gettextrs::gettext("What This Line Means")));
                        line.append(&button);
                        button
                    });
                    let text = gtk::Label::new(Some(note));
                    text.set_wrap(true);
                    text.set_max_width_chars(48);
                    text.set_xalign(0.0);
                    text.set_selectable(true);
                    text.add_css_class("lyric-annotation__text");
                    let popover = gtk::Popover::new();
                    popover.set_child(Some(&text));
                    button.set_popover(Some(&popover));
                }
                (None, None) => {}
            }
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
        let mut names = vec![gettextrs::gettext("App Language")];
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
        self.imp().translate_language_button.set_popover(Some(&popover));
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
            if let (Some(label), Some(text)) =
                (self.line_label(previous), imp.line_texts.borrow().get(previous))
            {
                label.set_text(text);
            }
        }
        imp.words_lit.set(0);

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

    #[allow(deprecated)]
    fn scroll_to_line(&self, widget: &gtk::Widget, animate: bool) {
        let imp = self.imp();
        if imp
            .manual_scroll_until
            .get()
            .is_some_and(|until| Instant::now() < until)
        {
            return;
        }
        let vadjust = imp.lyrics_scroller.vadjustment();
        if let Some((_, y)) = widget.translate_coordinates(&*imp.lyrics_container, 0.0, 0.0) {
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
            let anim =
                libadwaita::TimedAnimation::new(&*imp.lyrics_scroller, current, target, 260, target_fn);
            anim.set_easing(libadwaita::Easing::EaseOutCubic);
            anim.play();
            *imp.scroll_anim.borrow_mut() = Some(anim);
        }
    }
}

/// The line with its first `lit` words bright and the rest dim.
pub fn karaoke_markup(text: &str, lit: usize) -> String {
    let mut words = text.split_whitespace();
    let sung: Vec<&str> = words.by_ref().take(lit).collect();
    let rest: Vec<&str> = words.collect();
    let sung = glib::markup_escape_text(&sung.join(" "));
    if rest.is_empty() {
        return sung.to_string();
    }
    let rest = glib::markup_escape_text(&rest.join(" "));
    format!("{sung} <span alpha=\"45%\">{rest}</span>")
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

/// A lyric line's label with a hidden translation label under it.
fn line_with_translation(label: &gtk::Label) -> gtk::Box {
    let line = gtk::Box::new(gtk::Orientation::Vertical, 2);
    line.append(label);
    let translation = gtk::Label::new(None);
    translation.set_wrap(true);
    translation.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    translation.set_halign(gtk::Align::Start);
    translation.set_xalign(0.0);
    translation.set_visible(false);
    translation.add_css_class("lyric-translation");
    line.append(&translation);
    line
}

pub fn expose_widgets() {
    LyricsPageWidget::static_type();
}
