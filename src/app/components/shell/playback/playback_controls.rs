use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

use crate::app::models::RepeatMode;

mod imp {

    use std::cell::RefCell;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/io/github/gusthanks/Canto/components/playback_controls.ui")]
    pub struct PlaybackControlsWidget {
        #[template_child]
        pub play_pause: TemplateChild<gtk::Button>,

        #[template_child]
        pub next: TemplateChild<gtk::Button>,

        #[template_child]
        pub prev: TemplateChild<gtk::Button>,

        #[template_child]
        pub shuffle: TemplateChild<gtk::ToggleButton>,

        #[template_child]
        pub repeat: TemplateChild<gtk::Button>,

        pub shuffle_signal: RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlaybackControlsWidget {
        const NAME: &'static str = "PlaybackControlsWidget";
        type Type = super::PlaybackControlsWidget;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PlaybackControlsWidget {}
    impl WidgetImpl for PlaybackControlsWidget {}
    impl BoxImpl for PlaybackControlsWidget {}
}

/// The icon of a repeat mode, in the bar and in its More Options menu.
pub fn repeat_icon_name(mode: RepeatMode) -> &'static str {
    match mode {
        RepeatMode::Track => "media-playlist-repeat-song-symbolic",
        RepeatMode::Context => "media-playlist-repeat-symbolic",
        RepeatMode::Off => "media-playlist-consecutive-symbolic",
    }
}

glib::wrapper! {
    pub struct PlaybackControlsWidget(ObjectSubclass<imp::PlaybackControlsWidget>) @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl PlaybackControlsWidget {
    pub fn set_playing(&self, is_playing: bool) {
        let playback_icon = if is_playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        };

        let translated_tooltip = if is_playing {
            gettext("Pause")
        } else {
            gettext("Play")
        };
        let tooltip_text = Some(translated_tooltip.as_str());

        let playback_control = self.imp();
        playback_control.play_pause.set_icon_name(playback_icon);
        playback_control.play_pause.set_tooltip_text(tooltip_text);
    }

    pub fn set_shuffled(&self, shuffled: bool) {
        self.imp().shuffle.set_active(shuffled);
    }

    pub fn set_repeat_mode(&self, mode: RepeatMode) {
        self.imp().repeat.set_icon_name(repeat_icon_name(mode));
    }

    /// Shuffle, previous and repeat: the player bar hides them when it is
    /// narrow (see its tiers in playback_widget.rs).
    pub fn shuffle_button(&self) -> gtk::Widget {
        self.imp().shuffle.get().upcast()
    }

    pub fn prev_button(&self) -> gtk::Widget {
        self.imp().prev.get().upcast()
    }

    pub fn repeat_button(&self) -> gtk::Widget {
        self.imp().repeat.get().upcast()
    }

    pub fn connect_play_pause<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp().play_pause.connect_clicked(move |_| f());
    }

    pub fn connect_prev<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp().prev.connect_clicked(move |_| f());
    }

    pub fn connect_next<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp().next.connect_clicked(move |_| f());
    }

    pub fn connect_shuffle<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp().shuffle.connect_clicked(move |_| f());
    }

    pub fn connect_repeat<F>(&self, f: F)
    where
        F: Fn() + 'static,
    {
        self.imp().repeat.connect_clicked(move |_| f());
    }
}
