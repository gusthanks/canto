use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use riff_api::{GeniusTrackInfo, Lyrics};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/lyrics_page.ui")]
    pub struct LyricsPageWidget {
        #[template_child]
        pub lyrics_top_bar: TemplateChild<gtk::Box>,
        #[template_child]
        pub tab_lyrics_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub tab_about_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub genius_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub content_mode_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub about_scroller: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub about_container: TemplateChild<gtk::Box>,
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

        pub line_widgets: RefCell<Vec<gtk::Widget>>,
        pub active_index: RefCell<Option<usize>>,
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

        let weak_widget = widget.downgrade();
        imp.tab_lyrics_button.connect_toggled(move |btn| {
            if btn.is_active() {
                if let Some(w) = weak_widget.upgrade() {
                    w.imp().content_mode_stack.set_visible_child_name("lyrics");
                }
            }
        });

        let weak_widget = widget.downgrade();
        imp.tab_about_button.connect_toggled(move |btn| {
            if btn.is_active() {
                if let Some(w) = weak_widget.upgrade() {
                    w.imp().content_mode_stack.set_visible_child_name("about");
                }
            }
        });

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
    pub fn set_has_track(&self, has_track: bool) {
        let imp = self.imp();
        imp.lyrics_top_bar.set_visible(has_track);
        if !has_track {
            imp.tab_lyrics_button.set_active(true);
            imp.content_mode_stack.set_visible_child_name("lyrics");
            self.clear_about();
        }
    }

    pub fn is_about_selected(&self) -> bool {
        self.imp().tab_about_button.is_active()
    }

    pub fn set_genius_tooltip(&self, tooltip: &str) {
        self.imp().genius_button.set_tooltip_text(Some(tooltip));
    }

    pub fn connect_genius_clicked<F: Fn() + 'static>(&self, f: F) {
        self.imp().genius_button.connect_clicked(move |_| f());
    }

    pub fn connect_about_tab_activated<F: Fn() + 'static>(&self, f: F) {
        self.imp().tab_about_button.connect_toggled(move |btn| {
            if btn.is_active() {
                f();
            }
        });
    }

    fn clear_about(&self) {
        let container = &self.imp().about_container;
        while let Some(child) = container.first_child() {
            container.remove(&child);
        }
        self.imp().about_scroller.vadjustment().set_value(0.0);
    }

    fn make_about_card(&self) -> gtk::Box {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.add_css_class("about-track-card");
        card
    }

    fn append_header_card(&self, title: &str, artist: &str, subtitle: Option<&str>) {
        let card = self.make_about_card();
        let title_label = gtk::Label::new(Some(title));
        title_label.add_css_class("title-3");
        title_label.set_halign(gtk::Align::Start);
        title_label.set_xalign(0.0);
        title_label.set_wrap(true);
        card.append(&title_label);

        if !artist.is_empty() {
            let artist_label = gtk::Label::new(Some(artist));
            artist_label.add_css_class("dim-label");
            artist_label.set_halign(gtk::Align::Start);
            artist_label.set_xalign(0.0);
            artist_label.set_wrap(true);
            card.append(&artist_label);
        }

        if let Some(sub) = subtitle {
            if !sub.is_empty() {
                let sub_label = gtk::Label::new(Some(sub));
                sub_label.add_css_class("caption");
                sub_label.add_css_class("dim-label");
                sub_label.set_halign(gtk::Align::Start);
                sub_label.set_xalign(0.0);
                sub_label.set_wrap(true);
                card.append(&sub_label);
            }
        }

        self.imp().about_container.append(&card);
    }

    fn append_kv_row(card: &gtk::Box, heading: &str, value: &str) {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let h = gtk::Label::new(Some(&heading.to_uppercase()));
        h.add_css_class("about-track-section-title");
        h.set_halign(gtk::Align::Start);
        h.set_xalign(0.0);
        row.append(&h);

        let v = gtk::Label::new(Some(value));
        v.set_halign(gtk::Align::Start);
        v.set_xalign(0.0);
        v.set_wrap(true);
        v.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        v.set_selectable(true);
        row.append(&v);

        card.append(&row);
    }

    pub fn set_about_loading(&self, title: &str, artist: &str) {
        self.clear_about();
        self.append_header_card(title, artist, None);
        let card = self.make_about_card();
        let spinner_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        spinner_row.set_halign(gtk::Align::Center);
        let spinner = gtk::Spinner::new();
        spinner.start();
        spinner_row.append(&spinner);
        let lbl = gtk::Label::new(Some(&gettext("Loading credits and liner notes...")));
        lbl.add_css_class("dim-label");
        spinner_row.append(&lbl);
        card.append(&spinner_row);
        self.imp().about_container.append(&card);
    }

    pub fn set_about_prompt(
        &self,
        title: &str,
        artist: &str,
        album: Option<&str>,
        search_url: &str,
    ) {
        self.clear_about();
        self.append_header_card(title, artist, album);

        let card = self.make_about_card();
        let heading = gtk::Label::new(Some(&gettext("Credits & Liner Notes")));
        heading.add_css_class("heading");
        heading.set_halign(gtk::Align::Start);
        card.append(&heading);

        let body = gtk::Label::new(Some(&gettext(
            "Add a Genius Client Access Token in Preferences -> Integrations to load producers, writers, samples, and liner notes directly inside Canto.",
        )));
        body.add_css_class("about-track-notes");
        body.set_halign(gtk::Align::Start);
        body.set_xalign(0.0);
        body.set_wrap(true);
        body.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        card.append(&body);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_margin_top(4);

        let prefs_btn = gtk::Button::with_label(&gettext("Open Preferences"));
        prefs_btn.add_css_class("pill");
        prefs_btn.add_css_class("suggested-action");
        prefs_btn.connect_clicked(|btn| {
            let _ = btn.activate_action("win.preferences-integrations", None);
        });
        actions.append(&prefs_btn);

        let search_btn = gtk::Button::with_label(&gettext("Search on Genius"));
        search_btn.add_css_class("pill");
        let url_owned = search_url.to_string();
        search_btn.connect_clicked(move |btn| {
            open_external_url(btn, &url_owned);
        });
        actions.append(&search_btn);

        card.append(&actions);
        self.imp().about_container.append(&card);
    }

    pub fn set_about_error(&self, title: &str, artist: &str, message: &str, search_url: &str) {
        self.clear_about();
        self.append_header_card(title, artist, None);

        let card = self.make_about_card();
        let heading = gtk::Label::new(Some(&gettext("Couldn't load Genius metadata")));
        heading.add_css_class("heading");
        heading.set_halign(gtk::Align::Start);
        card.append(&heading);

        let body = gtk::Label::new(Some(message));
        body.add_css_class("dim-label");
        body.set_halign(gtk::Align::Start);
        body.set_xalign(0.0);
        body.set_wrap(true);
        card.append(&body);

        let btn = gtk::Button::with_label(&gettext("Search on Genius"));
        btn.add_css_class("pill");
        btn.set_halign(gtk::Align::Start);
        let url_owned = search_url.to_string();
        btn.connect_clicked(move |b| {
            open_external_url(b, &url_owned);
        });
        card.append(&btn);

        self.imp().about_container.append(&card);
    }

    pub fn set_about_info(&self, title: &str, artist: &str, info: &GeniusTrackInfo) {
        self.clear_about();

        let display_title = info
            .full_title
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(title);
        let mut sub_parts = Vec::new();
        if let Some(album) = info.album.as_deref().filter(|s| !s.is_empty()) {
            sub_parts.push(album.to_string());
        }
        if let Some(date) = info.release_date.as_deref().filter(|s| !s.is_empty()) {
            sub_parts.push(date.to_string());
        }
        let subtitle = (!sub_parts.is_empty()).then(|| sub_parts.join(" - "));
        self.append_header_card(display_title, artist, subtitle.as_deref());

        let has_credits = !info.producers.is_empty()
            || !info.writers.is_empty()
            || !info.featured_artists.is_empty()
            || info.album.is_some()
            || info.release_date.is_some();

        if has_credits {
            let credits_card = self.make_about_card();
            let heading = gtk::Label::new(Some(&gettext("Credits")));
            heading.add_css_class("heading");
            heading.set_halign(gtk::Align::Start);
            credits_card.append(&heading);

            if !info.producers.is_empty() {
                Self::append_kv_row(&credits_card, &gettext("Producer(s)"), &info.producers.join(", "));
            }
            if !info.writers.is_empty() {
                Self::append_kv_row(&credits_card, &gettext("Writer(s)"), &info.writers.join(", "));
            }
            if !info.featured_artists.is_empty() {
                Self::append_kv_row(
                    &credits_card,
                    &gettext("Featured Artists"),
                    &info.featured_artists.join(", "),
                );
            }
            if let Some(album) = info.album.as_deref().filter(|s| !s.is_empty()) {
                Self::append_kv_row(&credits_card, &gettext("Album"), album);
            }
            if let Some(date) = info.release_date.as_deref().filter(|s| !s.is_empty()) {
                Self::append_kv_row(&credits_card, &gettext("Release Date"), date);
            }
            self.imp().about_container.append(&credits_card);
        }

        if !info.relationships.is_empty() {
            let rel_card = self.make_about_card();
            let heading = gtk::Label::new(Some(&gettext("Samples & Connections")));
            heading.add_css_class("heading");
            heading.set_halign(gtk::Align::Start);
            rel_card.append(&heading);

            for rel in &info.relationships {
                if !rel.tracks.is_empty() {
                    Self::append_kv_row(&rel_card, &rel.label, &rel.tracks.join("\n"));
                }
            }
            self.imp().about_container.append(&rel_card);
        }

        if let Some(desc) = info.description.as_deref().filter(|s| !s.trim().is_empty()) {
            let notes_card = self.make_about_card();
            let heading = gtk::Label::new(Some(&gettext("Liner Notes")));
            heading.add_css_class("heading");
            heading.set_halign(gtk::Align::Start);
            notes_card.append(&heading);

            let notes_label = gtk::Label::new(Some(desc.trim()));
            notes_label.add_css_class("about-track-notes");
            notes_label.set_halign(gtk::Align::Start);
            notes_label.set_xalign(0.0);
            notes_label.set_wrap(true);
            notes_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            notes_label.set_selectable(true);
            notes_card.append(&notes_label);
            self.imp().about_container.append(&notes_card);
        } else if !has_credits && info.relationships.is_empty() {
            let empty_card = self.make_about_card();
            let lbl = gtk::Label::new(Some(&gettext(
                "No detailed credits or liner notes were found on Genius for this track.",
            )));
            lbl.add_css_class("dim-label");
            lbl.set_halign(gtk::Align::Start);
            lbl.set_xalign(0.0);
            lbl.set_wrap(true);
            empty_card.append(&lbl);
            self.imp().about_container.append(&empty_card);
        }

        let open_btn = gtk::Button::with_label(&if info.is_exact_match {
            gettext("Open Verified Genius Page")
        } else {
            gettext("Search on Genius")
        });
        open_btn.add_css_class("pill");
        open_btn.set_halign(gtk::Align::Start);
        let url_owned = info.url.clone();
        open_btn.connect_clicked(move |b| {
            open_external_url(b, &url_owned);
        });
        self.imp().about_container.append(&open_btn);
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
                    btn.set_child(Some(&label));

                    let seek_ms = line.start_time_ms.min(u64::from(u32::MAX)) as u32;
                    let on_seek_clone = on_seek.clone();
                    btn.connect_clicked(move |_| {
                        on_seek_clone(seek_ms);
                    });

                    imp.lyrics_container.append(&btn);
                    widgets.push(btn.upcast());
                }
                *imp.line_widgets.borrow_mut() = widgets;
                true
            }
            Lyrics::Plain(lines) => {
                let mut widgets = Vec::with_capacity(lines.len());
                for text in lines {
                    let label = gtk::Label::new(Some(text));
                    label.add_css_class("lyric-line");
                    label.add_css_class("plain");
                    label.set_wrap(true);
                    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                    label.set_halign(gtk::Align::Start);
                    label.set_xalign(0.0);

                    imp.lyrics_container.append(&label);
                    widgets.push(label.upcast());
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

    pub fn set_active_line(&self, new_index: Option<usize>) {
        let imp = self.imp();
        let mut active_cell = imp.active_index.borrow_mut();
        if *active_cell == new_index {
            return;
        }

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

pub fn expose_widgets() {
    LyricsPageWidget::static_type();
}
