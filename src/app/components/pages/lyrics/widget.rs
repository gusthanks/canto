use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use libadwaita::subclass::prelude::*;
use std::cell::RefCell;

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

        pub line_widgets: RefCell<Vec<gtk::Widget>>,
        pub active_index: RefCell<Option<usize>>,
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
        let vadjust = widget.imp().lyrics_scroller.vadjustment();
        let weak_widget = widget.downgrade();
        vadjust.connect_page_size_notify(move |_| {
            if let Some(w) = weak_widget.upgrade() {
                let active = *w.imp().active_index.borrow();
                if let Some(idx) = active {
                    let line_widgets = w.imp().line_widgets.borrow();
                    if let Some(lw) = line_widgets.get(idx) {
                        w.scroll_to_line(lw);
                    }
                }
            }
        });
        widget
    }
}

impl LyricsPageWidget {
    pub fn set_loading(&self) {
        self.imp().lyrics_stack.set_visible_child_name("loading");
    }

    pub fn set_empty(&self, title: &str, description: Option<&str>) {
        let imp = self.imp();
        imp.empty_page.set_title(title);
        imp.empty_page.set_description(description);
        imp.lyrics_stack.set_visible_child_name("empty");
    }

    pub fn set_lyrics<F>(&self, lyrics: &Lyrics, on_seek: F)
    where
        F: Fn(u32) + Clone + 'static,
    {
        let imp = self.imp();
        while let Some(child) = imp.lyrics_container.first_child() {
            imp.lyrics_container.remove(&child);
        }
        imp.line_widgets.borrow_mut().clear();
        imp.active_index.replace(None);

        match lyrics {
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

                    let seek_ms = line.start_time_ms as u32;
                    let on_seek_clone = on_seek.clone();
                    btn.connect_clicked(move |_| {
                        on_seek_clone(seek_ms);
                    });

                    imp.lyrics_container.append(&btn);
                    widgets.push(btn.upcast());
                }
                *imp.line_widgets.borrow_mut() = widgets;
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
            }
        }

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

        // Unset previous active line
        if let Some(old_idx) = *active_cell {
            if let Some(w) = widgets.get(old_idx) {
                w.remove_css_class("active");
            }
        }

        *active_cell = new_index;

        // Set new active line
        if let Some(idx) = new_index {
            if let Some(w) = widgets.get(idx) {
                w.add_css_class("active");
                self.scroll_to_line(w);
            }
        }
    }

    #[allow(deprecated)]
    fn scroll_to_line(&self, widget: &gtk::Widget) {
        let imp = self.imp();
        let vadjust = imp.lyrics_scroller.vadjustment();
        if let Some((_, y)) = widget.translate_coordinates(&*imp.lyrics_container, 0.0, 0.0) {
            let line_h = widget.height() as f64;
            let scroller_h = imp.lyrics_scroller.height() as f64;
            let max_val = (vadjust.upper() - vadjust.page_size()).max(0.0);
            let target = (y - (scroller_h - line_h) / 2.0).clamp(0.0, max_val);
            vadjust.set_value(target);
        }
    }
}

pub fn expose_widgets() {
    LyricsPageWidget::static_type();
}
