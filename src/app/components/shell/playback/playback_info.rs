use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

use crate::app::components::utils::set_missing_art;

mod imp {

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/playback_info.ui")]
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
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlaybackInfoWidget {
        const NAME: &'static str = "PlaybackInfoWidget";
        type Type = super::PlaybackInfoWidget;
        type ParentType = gtk::Button;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PlaybackInfoWidget {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_hover_animations();
        }
    }
    impl WidgetImpl for PlaybackInfoWidget {}
    impl ButtonImpl for PlaybackInfoWidget {}
}

glib::wrapper! {
    pub struct PlaybackInfoWidget(ObjectSubclass<imp::PlaybackInfoWidget>) @extends gtk::Widget, gtk::Button,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl PlaybackInfoWidget {
    fn setup_hover_animations(&self) {}

    /// Clicking the cover art runs `f` instead of the whole button's click.
    pub fn connect_cover_clicked<F: Fn() + 'static>(&self, f: F) {
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gdk::BUTTON_PRIMARY);
        // Claim the press so the surrounding button does not also fire.
        gesture.connect_pressed(|gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        gesture.connect_released(move |_, _, _, _| f());
        let cover = &self.imp().cover_frame;
        cover.set_cursor_from_name(Some("pointer"));
        cover.add_controller(gesture);
    }

    pub fn set_title_and_artist(&self, title: &str, artist: &str) {
        let widget = self.imp();
        widget.song_title.set_text(title);
        widget.song_artist.set_text(artist);
        widget.song_title.remove_css_class("dim-label");
        widget.song_info_box.set_visible(true);
        self.set_sensitive(true);
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
        self.set_sensitive(false);
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
