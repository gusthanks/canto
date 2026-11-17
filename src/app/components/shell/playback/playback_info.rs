use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

use crate::app::components::utils::set_missing_art;
use crate::app::components::now_panel::{show_like_state, title_markup};

mod imp {

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/io/github/gusthanks/Canto/components/playback_info.ui")]
    pub struct PlaybackInfoWidget {
        #[template_child]
        pub cover_frame: TemplateChild<gtk::Overlay>,

        #[template_child]
        pub placeholder_art: TemplateChild<gtk::Image>,

        #[template_child]
        pub playing_image: TemplateChild<gtk::Image>,

        #[template_child]
        pub song_info_box: TemplateChild<gtk::Box>,

        #[template_child]
        pub song_title: TemplateChild<gtk::Label>,

        #[template_child]
        pub song_artist: TemplateChild<gtk::Label>,

        #[template_child]
        pub like_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlaybackInfoWidget {
        const NAME: &'static str = "PlaybackInfoWidget";
        type Type = super::PlaybackInfoWidget;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PlaybackInfoWidget {}
    impl WidgetImpl for PlaybackInfoWidget {}
    impl BoxImpl for PlaybackInfoWidget {}
}

glib::wrapper! {
    pub struct PlaybackInfoWidget(ObjectSubclass<imp::PlaybackInfoWidget>) @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl PlaybackInfoWidget {
    /// Clicking the cover art runs `f`.
    pub fn connect_cover_clicked<F: Fn() + 'static>(&self, f: F) {
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        gesture.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            f();
        });
        let cover = &self.imp().cover_frame;
        cover.set_cursor_from_name(Some("pointer"));
        cover.add_controller(gesture);
    }

    /// A link in the title or the artists was clicked (see `now_panel::TrackLink`).
    pub fn connect_link_activated<F: Fn(&str) + Clone + 'static>(&self, f: F) {
        let imp = self.imp();
        for label in [&*imp.song_title, &*imp.song_artist] {
            let f = f.clone();
            label.connect_activate_link(move |_, uri| {
                f(uri);
                glib::Propagation::Stop
            });
        }
    }

    pub fn connect_like_clicked<F: Fn() + 'static>(&self, f: F) {
        self.imp().like_button.connect_clicked(move |_| f());
    }

    pub fn set_liked(&self, liked: Option<bool>) {
        show_like_state(&self.imp().like_button, liked);
    }

    /// `artists_markup` comes from `now_panel::artists_markup`.
    pub fn set_track(&self, title: &str, has_album: bool, artists_markup: &str) {
        let widget = self.imp();
        widget.song_title.set_markup(&title_markup(title, has_album));
        widget.song_artist.set_markup(artists_markup);
        widget.song_title.remove_css_class("dim-label");
        widget.song_info_box.set_visible(true);
        widget.cover_frame.set_sensitive(true);
    }

    pub fn reset_info(&self) {
        let widget = self.imp();
        widget
            .song_title
            // translators: Short text displayed instead of a track title when nothing plays
            .set_text(&gettext("No Track Playing"));
        widget
            .song_artist
            // translators: Subtle hint displayed below "No Track Playing" in the bottom player bar
            .set_text(&gettext("Select a track to play"));
        widget.song_title.add_css_class("dim-label");
        widget.song_info_box.set_visible(true);
        widget
            .playing_image
            .set_paintable(None::<gdk::Paintable>.as_ref());
        widget.placeholder_art.set_visible(true);
        set_missing_art(&*widget.playing_image, false);
        widget.cover_frame.set_sensitive(false);
        self.set_liked(None);
    }

    pub fn set_info_visible(&self, visible: bool) {
        self.imp().song_info_box.set_visible(visible);
    }

    pub fn set_artwork(&self, texture: &gdk::Texture) {
        let widget = self.imp();
        widget.playing_image.set_paintable(Some(texture));
        widget.placeholder_art.set_visible(false);
        set_missing_art(&*widget.playing_image, false);
    }

    pub fn set_missing_artwork(&self) {
        let widget = self.imp();
        widget
            .playing_image
            .set_paintable(None::<gdk::Paintable>.as_ref());
        widget.placeholder_art.set_visible(true);
        set_missing_art(&*widget.playing_image, true);
    }
}
