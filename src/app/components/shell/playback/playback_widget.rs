use gio::prelude::SettingsExt;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

use crate::app::components::utils::{decode_px, format_duration, Clock, Debouncer};
use crate::app::components::{
    display_add_css_provider, DeviceSelectorWidget, SleepTimerMode, SleepTimerSnapshot,
};
use crate::app::models::RepeatMode;

use riff_api::models::is_resource_url;
use riff_api::ApiService;
use riff_config::settings::SETTINGS;

use crate::app::load;
use std::cell::{Cell, RefCell};
use std::sync::Arc;

use super::playback_controls::PlaybackControlsWidget;
use super::playback_info::PlaybackInfoWidget;

/// Matches the artwork size request in playback_info.blp.
const ARTWORK_SIZE: i32 = 48;

pub const EQ_PRESETS: [(&str, [f64; 10]); 5] = [
    ("flat", [0.0; 10]),
    ("bass", [5.0, 4.0, 2.5, 1.0, 0.0, 0.0, 0.5, 1.0, 1.5, 1.5]),
    ("vocal", [-2.0, -1.5, 0.0, 2.0, 3.5, 3.5, 2.0, 0.5, -1.0, -1.5]),
    ("lofi", [2.5, 2.0, 1.0, 0.0, -1.5, -2.5, -3.5, -4.5, -5.0, -6.0]),
    ("acoustic", [2.0, 1.5, 1.0, 0.5, 1.5, 1.5, 2.0, 2.5, 2.0, 1.0]),
];

fn apply_eq_preset(gains: [f64; 10]) {
    let settings = gio::Settings::new(SETTINGS);
    for (idx, gain) in gains.iter().enumerate() {
        let key = format!("eq-band-{idx}");
        let _ = settings.set_double(&key, gain.clamp(-12.0, 12.0));
    }
}

mod imp {

    use super::*;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/io/github/gusthanks/Canto/components/playback_widget.ui")]
    pub struct PlaybackWidget {
        #[template_child]
        pub controls: TemplateChild<PlaybackControlsWidget>,

        #[template_child]
        pub now_playing: TemplateChild<PlaybackInfoWidget>,

        #[template_child]
        pub now_playing_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub seek_row: TemplateChild<gtk::Box>,

        #[template_child]
        pub seek_bar: TemplateChild<gtk::Scale>,

        #[template_child]
        pub track_position: TemplateChild<gtk::Label>,

        #[template_child]
        pub track_duration: TemplateChild<gtk::Label>,

        #[template_child]
        pub volume_slider: TemplateChild<gtk::Scale>,

        #[template_child]
        pub mute_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub eq_popover: TemplateChild<gtk::Popover>,

        #[template_child]
        pub eq_preset_flat: TemplateChild<gtk::Button>,

        #[template_child]
        pub eq_preset_bass: TemplateChild<gtk::Button>,

        #[template_child]
        pub eq_preset_vocal: TemplateChild<gtk::Button>,

        #[template_child]
        pub eq_preset_lofi: TemplateChild<gtk::Button>,

        #[template_child]
        pub eq_preset_acoustic: TemplateChild<gtk::Button>,

        #[template_child]
        pub eq_open_settings: TemplateChild<gtk::Button>,

        #[template_child]
        pub start_radio_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub sleep_timer_button: TemplateChild<gtk::MenuButton>,

        #[template_child]
        pub sleep_timer_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub sleep_popover: TemplateChild<gtk::Popover>,

        #[template_child]
        pub sleep_off_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub sleep_track_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub sleep_15_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub sleep_30_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub sleep_45_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub sleep_60_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub device_selector: TemplateChild<DeviceSelectorWidget>,

        #[template_child]
        pub queue_button: TemplateChild<gtk::Button>,

        pub clock: Clock,
        pub volume_changed_handler: RefCell<Option<glib::SignalHandlerId>>,
        pub last_nonzero_volume: Cell<f64>,
    }

    impl Default for PlaybackWidget {
        fn default() -> Self {
            Self {
                controls: Default::default(),
                now_playing: Default::default(),
                now_playing_button: Default::default(),
                seek_row: Default::default(),
                seek_bar: Default::default(),
                track_position: Default::default(),
                track_duration: Default::default(),
                volume_slider: Default::default(),
                mute_button: Default::default(),
                eq_popover: Default::default(),
                eq_preset_flat: Default::default(),
                eq_preset_bass: Default::default(),
                eq_preset_vocal: Default::default(),
                eq_preset_lofi: Default::default(),
                eq_preset_acoustic: Default::default(),
                eq_open_settings: Default::default(),
                start_radio_button: Default::default(),
                sleep_timer_button: Default::default(),
                sleep_timer_label: Default::default(),
                sleep_popover: Default::default(),
                sleep_off_btn: Default::default(),
                sleep_track_btn: Default::default(),
                sleep_15_btn: Default::default(),
                sleep_30_btn: Default::default(),
                sleep_45_btn: Default::default(),
                sleep_60_btn: Default::default(),
                device_selector: Default::default(),
                queue_button: Default::default(),
                clock: Clock::default(),
                volume_changed_handler: Default::default(),
                last_nonzero_volume: Cell::new(0.7),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PlaybackWidget {
        const NAME: &'static str = "PlaybackWidget";
        type Type = super::PlaybackWidget;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PlaybackWidget {
        fn constructed(&self) {
            self.parent_constructed();
            self.now_playing.set_info_visible(true);
            self.now_playing.reset_info();
            display_add_css_provider(resource!("/components/playback.css"));

            let popover = self.eq_popover.clone();
            let preset_buttons = [
                (&*self.eq_preset_flat, EQ_PRESETS[0].1),
                (&*self.eq_preset_bass, EQ_PRESETS[1].1),
                (&*self.eq_preset_vocal, EQ_PRESETS[2].1),
                (&*self.eq_preset_lofi, EQ_PRESETS[3].1),
                (&*self.eq_preset_acoustic, EQ_PRESETS[4].1),
            ];
            for (btn, gains) in preset_buttons {
                let p = popover.clone();
                btn.connect_clicked(move |_| {
                    apply_eq_preset(gains);
                    p.popdown();
                });
            }
            let p_close = popover.clone();
            self.eq_open_settings.connect_clicked(move |_| {
                p_close.popdown();
            });
            self.start_radio_button.connect_clicked(move |_| {
                popover.popdown();
            });

            let volume_slider = self.volume_slider.clone();
            let weak_self = self.obj().downgrade();
            self.mute_button.connect_clicked(move |_| {
                let Some(obj) = weak_self.upgrade() else {
                    return;
                };
                let cur = volume_slider.value();
                if cur > 0.01 {
                    obj.imp().last_nonzero_volume.set(cur);
                    volume_slider.set_value(0.0);
                } else {
                    let restore = obj.imp().last_nonzero_volume.get().max(0.2);
                    volume_slider.set_value(restore);
                }
            });

            // Speaker icon that follows the volume (muted / low / medium / high).
            let mute_button = self.mute_button.clone();
            let sync_volume_icon = move |scale: &gtk::Scale| {
                mute_button.set_icon_name(super::volume_icon_name(scale.value()));
            };
            sync_volume_icon(&self.volume_slider);
            self.volume_slider.connect_value_changed(sync_volume_icon);
        }
    }

    impl WidgetImpl for PlaybackWidget {}
    impl BoxImpl for PlaybackWidget {}
}

/// Standard speaker icon for a volume level in `0.0..=1.0`.
fn volume_icon_name(value: f64) -> &'static str {
    if value <= 0.01 {
        "audio-volume-muted-symbolic"
    } else if value < 0.34 {
        "audio-volume-low-symbolic"
    } else if value < 0.67 {
        "audio-volume-medium-symbolic"
    } else {
        "audio-volume-high-symbolic"
    }
}

glib::wrapper! {
    pub struct PlaybackWidget(ObjectSubclass<imp::PlaybackWidget>) @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl PlaybackWidget {
    pub fn device_selector(&self) -> DeviceSelectorWidget {
        self.imp().device_selector.clone()
    }

    pub fn set_track(&self, title: &str, has_album: bool, artists_markup: &str) {
        let widget = self.imp();
        widget.now_playing.set_visible(true);
        widget
            .now_playing
            .set_track(title, has_album, artists_markup);
        widget.now_playing_button.set_visible(true);
    }

    pub fn set_liked(&self, liked: Option<bool>) {
        self.imp().now_playing.set_liked(liked);
    }

    pub fn reset_info(&self) {
        let widget = self.imp();
        widget.now_playing.set_visible(true);
        widget.now_playing.reset_info();
        widget.now_playing_button.set_visible(false);
        self.set_song_duration(None);
    }

    fn set_artwork(&self, texture: &gdk::Texture) {
        let widget = self.imp();
        widget.now_playing.set_artwork(texture);
    }

    pub fn set_artwork_from_url(&self, url: String, api_service: Arc<ApiService>) {
        if is_resource_url(&url) {
            self.imp().now_playing.set_missing_artwork();
            return;
        }
        let weak_self = self.downgrade();
        // Not part of any page, so it rides the current epoch.
        let tag = load::transport();
        let size = decode_px(ARTWORK_SIZE);
        glib::MainContext::default().spawn_local(async move {
            let result = api_service.load_image(&url, size, size, tag).await;
            if let (Some(ref _self), Some(ref texture)) = (weak_self.upgrade(), result) {
                _self.set_artwork(texture);
            }
        });
    }

    pub fn set_song_duration(&self, duration: Option<f64>) {
        let widget = self.imp();
        let class = "seek-bar--active";
        if let Some(duration) = duration {
            self.add_css_class(class);
            widget.seek_bar.set_sensitive(true);
            widget.seek_bar.set_range(0.0, duration);
            widget.seek_bar.set_value(0.0);
            self.update_track_time(0.0, duration);
        } else {
            self.remove_css_class(class);
            widget.seek_bar.set_sensitive(false);
            widget.seek_bar.set_range(0.0, 0.0);
            self.update_track_time(0.0, 0.0);
        }
    }

    pub fn set_seek_position(&self, pos: f64) {
        let widget = self.imp();
        widget.seek_bar.set_value(pos);
        let duration = widget.seek_bar.adjustment().upper();
        self.update_track_time(pos, duration);
    }

    fn update_track_time(&self, pos: f64, duration: f64) {
        let widget = self.imp();
        widget.track_position.set_text(&format_duration(pos));
        widget.track_duration.set_text(&format_duration(duration));
    }

    pub fn increment_seek_position(&self) {
        let value = self.imp().seek_bar.value() + 1_000.0;
        self.set_seek_position(value);
    }

    /// Start the local clock that advances the seekbar once per second.
    pub fn resume_seek_position(&self) {
        let widget = self.imp();
        widget.clock.start(clone!(
            #[weak(rename_to = _self)]
            self,
            move || _self.increment_seek_position()
        ));
    }

    /// Stop advancing the seekbar locally, freezing it at its current position.
    /// Used when playback is paused, without altering anything else.
    pub fn freeze_seek_position(&self) {
        self.imp().clock.stop();
    }

    pub fn connect_cover_clicked<F: Fn() + 'static>(&self, f: F) {
        self.imp().now_playing.connect_cover_clicked(f);
    }

    pub fn connect_link_activated<F: Fn(&str) + Clone + 'static>(&self, f: F) {
        self.imp().now_playing.connect_link_activated(f);
    }

    pub fn connect_like_clicked<F: Fn() + 'static>(&self, f: F) {
        self.imp().now_playing.connect_like_clicked(f);
    }

    pub fn connect_seek<Seek>(&self, seek: Seek)
    where
        Seek: Fn(u32) + Clone + 'static,
    {
        let debouncer = Debouncer::new();
        let widget = self.imp();
        widget.seek_bar.set_increments(5_000.0, 10_000.0);
        widget.seek_bar.connect_change_value(clone!(
            #[weak(rename_to = _self)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, _, requested| {
                let duration = _self.imp().seek_bar.adjustment().upper();
                _self.update_track_time(requested, duration);
                let seek = seek.clone();
                debouncer.debounce(200, move || seek(requested as u32));
                glib::Propagation::Proceed
            }
        ));
    }

    pub fn set_playing(&self, is_playing: bool) {
        let widget = self.imp();
        widget.controls.set_playing(is_playing);
        if is_playing {
            self.resume_seek_position();
        } else {
            self.freeze_seek_position();
        }
    }

    pub fn set_repeat_mode(&self, mode: RepeatMode) {
        let widget = self.imp();
        widget.controls.set_repeat_mode(mode);
    }

    pub fn set_shuffled(&self, shuffled: bool) {
        let widget = self.imp();
        widget.controls.set_shuffled(shuffled);
    }

    pub fn set_seekbar_visible(&self, visible: bool) {
        let widget = self.imp();
        widget.seek_row.set_visible(visible);
    }

    pub fn set_volume(&self, value: f64) {
        let widget = self.imp();
        if value > 0.01 {
            widget.last_nonzero_volume.set(value);
        }
        let handler = widget.volume_changed_handler.borrow();
        if let Some(id) = handler.as_ref() {
            widget.volume_slider.block_signal(id);
        }
        widget.volume_slider.set_value(value);
        if let Some(id) = handler.as_ref() {
            widget.volume_slider.unblock_signal(id);
        }
    }

    pub fn set_sleep_timer_snapshot(&self, snap: &SleepTimerSnapshot) {
        let imp = self.imp();
        imp.sleep_timer_button
            .set_tooltip_text(Some(&snap.tooltip()));
        if let Some(text) = snap.compact_label() {
            imp.sleep_timer_label.set_label(&text);
            imp.sleep_timer_label.set_visible(true);
            imp.sleep_timer_button.set_width_request(-1);
            imp.sleep_timer_button.remove_css_class("circular");
            imp.sleep_timer_button.add_css_class("pill");
        } else {
            imp.sleep_timer_label.set_visible(false);
            imp.sleep_timer_button.remove_css_class("pill");
            imp.sleep_timer_button.add_css_class("circular");
            imp.sleep_timer_button.set_width_request(36);
        }

        let active_btn: Option<&gtk::Button> = match snap.mode {
            SleepTimerMode::Off => Some(&*imp.sleep_off_btn),
            SleepTimerMode::EndOfTrack => Some(&*imp.sleep_track_btn),
            SleepTimerMode::Minutes(15) => Some(&*imp.sleep_15_btn),
            SleepTimerMode::Minutes(30) => Some(&*imp.sleep_30_btn),
            SleepTimerMode::Minutes(45) => Some(&*imp.sleep_45_btn),
            SleepTimerMode::Minutes(60) => Some(&*imp.sleep_60_btn),
            SleepTimerMode::Minutes(_) => None,
        };
        for btn in [
            &*imp.sleep_off_btn,
            &*imp.sleep_track_btn,
            &*imp.sleep_15_btn,
            &*imp.sleep_30_btn,
            &*imp.sleep_45_btn,
            &*imp.sleep_60_btn,
        ] {
            if active_btn == Some(btn) {
                btn.add_css_class("suggested-action");
            } else {
                btn.remove_css_class("suggested-action");
            }
        }
    }

    pub fn connect_sleep_timer<F: Fn(SleepTimerMode) + Clone + 'static>(&self, on_select: F) {
        let imp = self.imp();
        let presets = [
            (&*imp.sleep_off_btn, SleepTimerMode::Off),
            (&*imp.sleep_track_btn, SleepTimerMode::EndOfTrack),
            (&*imp.sleep_15_btn, SleepTimerMode::Minutes(15)),
            (&*imp.sleep_30_btn, SleepTimerMode::Minutes(30)),
            (&*imp.sleep_45_btn, SleepTimerMode::Minutes(45)),
            (&*imp.sleep_60_btn, SleepTimerMode::Minutes(60)),
        ];
        for (btn, mode) in presets {
            let pop = imp.sleep_popover.clone();
            let cb = on_select.clone();
            btn.connect_clicked(move |_| {
                cb(mode);
                pop.popdown();
            });
        }
    }

    pub fn connect_play_pause<F>(&self, f: F)
    where
        F: Fn() + Clone + 'static,
    {
        let widget = self.imp();
        widget.controls.connect_play_pause(f);
    }

    pub fn connect_prev<F>(&self, f: F)
    where
        F: Fn() + Clone + 'static,
    {
        let widget = self.imp();
        widget.controls.connect_prev(f);
    }

    pub fn connect_next<F>(&self, f: F)
    where
        F: Fn() + Clone + 'static,
    {
        let widget = self.imp();
        widget.controls.connect_next(f);
    }

    pub fn connect_shuffle<F>(&self, f: F)
    where
        F: Fn() + Clone + 'static,
    {
        let widget = self.imp();
        widget.controls.connect_shuffle(f);
    }

    pub fn connect_repeat<F>(&self, f: F)
    where
        F: Fn() + Clone + 'static,
    {
        let widget = self.imp();
        widget.controls.connect_repeat(f);
    }

    pub fn connect_volume_changed<F>(&self, f: F)
    where
        F: Fn(f64) + Clone + 'static,
    {
        let debouncer = Debouncer::new();
        let widget = self.imp();
        let id = widget.volume_slider.connect_value_changed(move |scale| {
            // Debounce dispatch: a single mouse-wheel flick emits a burst of
            // high-resolution scroll deltas, each firing `value_changed`.
            // Without this, every delta would fan out to the mixer, the Web
            // API, dconf and an MPRIS `PropertiesChanged` D-Bus signal, which
            // can flood GNOME Shell's media controls and hang the session.
            let value = scale.value();
            let f = f.clone();
            debouncer.debounce(100, move || f(value));
        });
        *widget.volume_changed_handler.borrow_mut() = Some(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_icon_follows_level() {
        assert_eq!(volume_icon_name(0.0), "audio-volume-muted-symbolic");
        assert_eq!(volume_icon_name(0.2), "audio-volume-low-symbolic");
        assert_eq!(volume_icon_name(0.5), "audio-volume-medium-symbolic");
        assert_eq!(volume_icon_name(0.9), "audio-volume-high-symbolic");
    }

    #[test]
    fn eq_presets_have_valid_10_band_ranges() {
        for (name, gains) in EQ_PRESETS {
            assert_eq!(gains.len(), 10, "preset {} should define 10 bands", name);
            for g in gains {
                assert!(
                    (-12.0..=12.0).contains(&g),
                    "preset {} gain {} out of gschema range",
                    name,
                    g
                );
            }
        }
    }
}
