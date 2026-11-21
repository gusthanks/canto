use gettextrs::gettext;
use gio::prelude::SettingsExt;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};

use crate::app::components::utils::{
    decode_px, format_duration, format_duration_short, Clock, Debouncer,
};
use crate::app::components::{
    display_add_css_provider, DeviceSelectorWidget, SleepTimerMode, SleepTimerSnapshot,
};
use crate::app::models::RepeatMode;

use canto_api::models::is_resource_url;
use canto_api::ApiService;
use canto_config::settings::SETTINGS;

use crate::app::load;
use std::cell::{Cell, RefCell};
use std::sync::Arc;

use super::playback_controls::{repeat_icon_name, PlaybackControlsWidget};
use super::playback_info::PlaybackInfoWidget;
use libadwaita::{BreakpointCondition, BreakpointConditionLengthType, LengthUnit};

/// Matches the artwork size request in playback_info.blp.
const ARTWORK_SIZE: i32 = 48;

/// Under each of these widths the player bar drops more of its parts, so it
/// stays on one line and its title and artist keep about 100 sp
/// (`width-chars` in playback_info.blp). Each tier adds to the ones above:
/// - 860: the volume slider turns into a button, and the device and mini
///   player buttons leave (both stay in Ctrl+K);
/// - 660 (half of a 1200 sp screen): shuffle and repeat move into More
///   Options, and the times beside the seek bar get shorter (a track over
///   an hour reads 1h02);
/// - 560: lyrics and queue leave (the side panel has them as tabs, and the
///   cover opens it);
/// - 486: the like button and the volume button leave;
/// - 400: previous and the times leave.
///
/// The gear and the side panel button never leave: the gear is the way to
/// everything else. Each width is one less than what the tier above needs
/// at text 1.0 (its row: track with a 100 title, controls, buttons), in px,
/// and in sp for larger text (`bar_tier_condition`).
const BAR_TIER_WIDTHS: [f64; 5] = [860.0, 660.0, 560.0, 486.0, 400.0];

pub const EQ_PRESETS: [(&str, [f64; 10]); 5] = [
    ("flat", [0.0; 10]),
    ("bass", [5.0, 4.0, 2.5, 1.0, 0.0, 0.0, 0.5, 1.0, 1.5, 1.5]),
    (
        "vocal",
        [-2.0, -1.5, 0.0, 2.0, 3.5, 3.5, 2.0, 0.5, -1.0, -1.5],
    ),
    (
        "lofi",
        [2.5, 2.0, 1.0, 0.0, -1.5, -2.5, -3.5, -4.5, -5.0, -6.0],
    ),
    (
        "acoustic",
        [2.0, 1.5, 1.0, 0.5, 1.5, 1.5, 2.0, 2.5, 2.0, 1.0],
    ),
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
        pub bar_bin: TemplateChild<libadwaita::BreakpointBin>,

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
        pub volume_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub volume_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub device_selector_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub mini_player_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub lyrics_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub more_shuffle: TemplateChild<gtk::Button>,
        #[template_child]
        pub more_shuffle_check: TemplateChild<gtk::Image>,
        #[template_child]
        pub more_repeat: TemplateChild<gtk::Button>,
        #[template_child]
        pub more_repeat_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub more_repeat_check: TemplateChild<gtk::Image>,
        #[template_child]
        pub more_toggles_separator: TemplateChild<gtk::Separator>,

        #[template_child]
        pub eq_popover: TemplateChild<gtk::Popover>,
        #[template_child]
        pub more_sleep_timer: TemplateChild<gtk::Button>,
        #[template_child]
        pub more_sleep_back: TemplateChild<gtk::Button>,
        #[template_child]
        pub more_sleep_off: TemplateChild<gtk::Button>,
        #[template_child]
        pub more_sleep_track: TemplateChild<gtk::Button>,
        #[template_child]
        pub more_sleep_15: TemplateChild<gtk::Button>,
        #[template_child]
        pub more_sleep_30: TemplateChild<gtk::Button>,
        #[template_child]
        pub more_sleep_45: TemplateChild<gtk::Button>,
        #[template_child]
        pub more_sleep_60: TemplateChild<gtk::Button>,
        #[template_child]
        pub more_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub eq_open_presets: TemplateChild<gtk::Button>,
        #[template_child]
        pub eq_presets_back: TemplateChild<gtk::Button>,
        #[template_child]
        pub customize_bar_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub bar_end: TemplateChild<gtk::Box>,
        #[template_child]
        pub eq_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub lyrics_slot: TemplateChild<gtk::Box>,
        #[template_child]
        pub queue_slot: TemplateChild<gtk::Box>,
        #[template_child]
        pub device_slot: TemplateChild<gtk::Box>,
        #[template_child]
        pub volume_slot: TemplateChild<gtk::Box>,
        #[template_child]
        pub mini_player_slot: TemplateChild<gtk::Box>,
        #[template_child]
        pub side_panel_slot: TemplateChild<gtk::Box>,
        pub customize_popover: RefCell<Option<gtk::Popover>>,

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
        pub sleep_timer_slot: TemplateChild<gtk::Box>,

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
        /// The bar's tiers, by width (see `BAR_TIER_WIDTHS`)
        pub bar_tiers: RefCell<Vec<(f64, libadwaita::Breakpoint)>>,
        /// The room (px) the tiers keep for the sleep timer's countdown
        pub bar_tier_room: Cell<i32>,
        /// Whether a tier under the widest one applies, which keeps the
        /// times beside the seek bar short
        pub short_times: Cell<bool>,
    }

    impl Default for PlaybackWidget {
        fn default() -> Self {
            Self {
                controls: Default::default(),
                now_playing: Default::default(),
                bar_bin: Default::default(),
                seek_row: Default::default(),
                seek_bar: Default::default(),
                track_position: Default::default(),
                track_duration: Default::default(),
                volume_slider: Default::default(),
                mute_button: Default::default(),
                volume_box: Default::default(),
                volume_button: Default::default(),
                device_selector_box: Default::default(),
                mini_player_button: Default::default(),
                lyrics_button: Default::default(),
                more_shuffle: Default::default(),
                more_shuffle_check: Default::default(),
                more_repeat: Default::default(),
                more_repeat_icon: Default::default(),
                more_repeat_check: Default::default(),
                more_toggles_separator: Default::default(),
                eq_popover: Default::default(),
                more_sleep_timer: Default::default(),
                more_sleep_back: Default::default(),
                more_sleep_off: Default::default(),
                more_sleep_track: Default::default(),
                more_sleep_15: Default::default(),
                more_sleep_30: Default::default(),
                more_sleep_45: Default::default(),
                more_sleep_60: Default::default(),
                more_stack: Default::default(),
                eq_open_presets: Default::default(),
                eq_presets_back: Default::default(),
                customize_bar_button: Default::default(),
                bar_end: Default::default(),
                eq_button: Default::default(),
                lyrics_slot: Default::default(),
                queue_slot: Default::default(),
                device_slot: Default::default(),
                volume_slot: Default::default(),
                mini_player_slot: Default::default(),
                side_panel_slot: Default::default(),
                customize_popover: Default::default(),
                eq_preset_flat: Default::default(),
                eq_preset_bass: Default::default(),
                eq_preset_vocal: Default::default(),
                eq_preset_lofi: Default::default(),
                eq_preset_acoustic: Default::default(),
                eq_open_settings: Default::default(),
                start_radio_button: Default::default(),
                sleep_timer_slot: Default::default(),
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
                bar_tiers: Default::default(),
                bar_tier_room: Cell::new(0),
                short_times: Cell::new(false),
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
        fn dispose(&self) {
            if let Some(popover) = self.customize_popover.take() {
                popover.unparent();
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.now_playing.set_info_visible(true);
            self.now_playing.reset_info();
            display_add_css_provider(resource!("/components/playback.css"));

            self.setup_customizable_bar();
            self.setup_bar_tiers();

            // Under the widest tier the times beside the seek bar keep 4
            // characters: a track over an hour reads 1h02 there, not 1∶0…
            use libadwaita::prelude::BreakpointBinExt;
            let weak_self = self.obj().downgrade();
            self.bar_bin.connect_current_breakpoint_notify(move |bin| {
                if let Some(obj) = weak_self.upgrade() {
                    let imp = obj.imp();
                    let short = bin.current_breakpoint().is_some_and(|current| {
                        imp.bar_tiers
                            .borrow()
                            .iter()
                            .skip(1)
                            .any(|(_, tier)| *tier == current)
                    });
                    if imp.short_times.replace(short) != short {
                        let duration = imp.seek_bar.adjustment().upper();
                        obj.update_track_time(imp.seek_bar.value(), duration);
                    }
                }
            });

            // The tiers keep room for the sleep timer's countdown while it
            // shows (measured once it has its style, on screen). The
            // narrowest tier hides its button: its slot follows
            let weak_self = self.obj().downgrade();
            self.sleep_timer_button.connect_map(move |_| {
                if let Some(obj) = weak_self.upgrade() {
                    obj.imp().retune_bar_tiers();
                }
            });
            let weak_self = self.obj().downgrade();
            self.sleep_timer_button.connect_visible_notify(move |_| {
                if let Some(obj) = weak_self.upgrade() {
                    obj.imp().sync_sleep_timer_slot();
                }
            });

            // The presets and the sleep timer's choices live one level down:
            // Equalizer › and Sleep Timer › slide to them, and the menu opens
            // on its first page again next time. The timer's own button in
            // the bar only shows while a timer runs: showing it to open its
            // menu could push the bar into its next tier, which closes any
            // menu open from the bar
            for (open, page) in [
                (&*self.eq_open_presets, "presets"),
                (&*self.more_sleep_timer, "sleep"),
            ] {
                let stack = self.more_stack.clone();
                open.connect_clicked(move |_| stack.set_visible_child_name(page));
            }
            for back in [&*self.eq_presets_back, &*self.more_sleep_back] {
                let stack = self.more_stack.clone();
                back.connect_clicked(move |_| stack.set_visible_child_name("main"));
            }

            // For screen readers, shuffle and repeat's rows are toggles from
            // the start, and repeat's tells which repeat it is
            self.more_shuffle
                .update_state(&[gtk::accessible::State::Pressed(pressed(false))]);
            self.more_repeat
                .update_state(&[gtk::accessible::State::Pressed(pressed(false))]);
            self.more_repeat
                .update_property(
                    &[gtk::accessible::Property::Description(&repeat_description(
                        RepeatMode::Off,
                    ))],
                );
            let stack = self.more_stack.clone();
            self.eq_popover.connect_closed(move |_| {
                stack.set_transition_type(gtk::StackTransitionType::None);
                stack.set_visible_child_name("main");
                stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
            });

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

            // The level and the keys that change it, on the slider and on
            // the narrow bar's volume button
            let volume_button = self.volume_button.clone();
            let sync_volume_tooltip = move |scale: &gtk::Scale| {
                let text = super::volume_tooltip(scale.value());
                scale.set_tooltip_text(Some(&text));
                volume_button.set_tooltip_text(Some(&text));
            };
            sync_volume_tooltip(&self.volume_slider);
            self.volume_slider
                .connect_value_changed(sync_volume_tooltip);

            // The wheel or the touchpad over the speaker and the slider, or
            // over the narrow bar's button: 5% a step, as Ctrl+Up and
            // Ctrl+Down. Captured before the slider's own scrolling, which
            // would move it again
            let targets: [gtk::Widget; 2] = [
                self.volume_box.get().upcast(),
                self.volume_button.get().upcast(),
            ];
            for target in targets {
                let scroll = gtk::EventControllerScroll::new(
                    gtk::EventControllerScrollFlags::VERTICAL
                        | gtk::EventControllerScrollFlags::DISCRETE,
                );
                scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
                let slider = self.volume_slider.clone();
                scroll.connect_scroll(move |controller, _, dy| {
                    // With natural scrolling the fingers
                    // going up scroll down: the volume follows the fingers
                    let event = controller
                        .current_event()
                        .and_then(|event| event.downcast::<gtk::gdk::ScrollEvent>().ok());
                    let natural = event.as_ref().is_some_and(|event| {
                        let hyprland = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")
                            .map(|_| super::hyprland_natural_scroll());
                        let natural = super::is_natural_scroll(
                            event.relative_direction(),
                            event.unit(),
                            hyprland,
                        );
                        log::debug!(
                            "volume scroll: dy={} unit={:?} rel={:?} natural={}",
                            dy,
                            event.unit(),
                            event.relative_direction(),
                            natural
                        );
                        natural
                    });
                    let steps = super::scroll_steps(dy, natural);
                    slider.set_value(super::volume_after_scroll(slider.value(), steps));
                    glib::Propagation::Stop
                });
                target.add_controller(scroll);
            }
        }
    }

    impl WidgetImpl for PlaybackWidget {}

    impl PlaybackWidget {
        /// Each optional button sits in a slot whose visibility follows a
        /// player-bar-* setting (`follow_slot`). The bar's tiers hide the
        /// buttons inside, so the two never fight over the same property.
        fn setup_customizable_bar(&self) {
            use canto_config::settings::{
                player_bar_order, PLAYER_BAR_DEVICE, PLAYER_BAR_LYRICS, PLAYER_BAR_MINI_PLAYER,
                PLAYER_BAR_ORDER, PLAYER_BAR_QUEUE, PLAYER_BAR_SIDE_PANEL, PLAYER_BAR_VOLUME,
            };
            let settings = gio::Settings::new(SETTINGS);
            let items: [(&str, &str, gtk::Widget, String); 7] = [
                (
                    "lyrics",
                    PLAYER_BAR_LYRICS,
                    self.lyrics_slot.get().upcast(),
                    gettext("Lyrics"),
                ),
                (
                    "queue",
                    PLAYER_BAR_QUEUE,
                    self.queue_slot.get().upcast(),
                    gettext("Queue"),
                ),
                // translators: Checkbox in "Customize Bar": the button that picks where the sound plays.
                (
                    "device",
                    PLAYER_BAR_DEVICE,
                    self.device_slot.get().upcast(),
                    gettext("Device"),
                ),
                // translators: Checkbox in "Customize Bar": the volume slider.
                (
                    "volume",
                    PLAYER_BAR_VOLUME,
                    self.volume_slot.get().upcast(),
                    gettext("Volume"),
                ),
                (
                    "mini-player",
                    PLAYER_BAR_MINI_PLAYER,
                    self.mini_player_slot.get().upcast(),
                    gettext("Mini Player"),
                ),
                // The gear has no key: it is always there, as the way back
                (
                    "more",
                    "",
                    self.eq_button.get().upcast(),
                    gettext("More Options"),
                ),
                (
                    "side-panel",
                    PLAYER_BAR_SIDE_PANEL,
                    self.side_panel_slot.get().upcast(),
                    gettext("Side Panel"),
                ),
            ];

            let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
            list.set_margin_top(6);
            list.set_margin_bottom(6);
            list.set_margin_start(6);
            list.set_margin_end(6);
            // translators: Heading of the popover that chooses which buttons the player bar shows.
            let title = gtk::Label::new(Some(&gettext("Show in the Bar")));
            title.set_xalign(0.0);
            title.set_margin_start(6);
            title.set_margin_bottom(4);
            title.add_css_class("heading");
            list.append(&title);
            // Each row: show/hide, then arrows that move the button along
            let mut rows: Vec<(
                &'static str,
                gtk::Widget,
                gtk::Box,
                gtk::Button,
                gtk::Button,
            )> = Vec::new();
            for (id, key, widget, label) in items {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
                if key.is_empty() {
                    let name = gtk::Label::new(Some(&label));
                    name.set_xalign(0.0);
                    name.set_hexpand(true);
                    name.set_margin_start(30);
                    row.append(&name);
                } else {
                    follow_slot(&settings, key, &widget);
                    let check = gtk::CheckButton::with_label(&label);
                    check.set_hexpand(true);
                    settings.bind(key, &check, "active").build();
                    row.append(&check);
                }
                let up = gtk::Button::from_icon_name("pan-up-symbolic");
                // translators: Tooltip of the arrow that moves a button earlier in the player bar.
                up.set_tooltip_text(Some(&gettext("Move Left")));
                let down = gtk::Button::from_icon_name("pan-down-symbolic");
                // translators: Tooltip of the arrow that moves a button later in the player bar.
                down.set_tooltip_text(Some(&gettext("Move Right")));
                for arrow in [&up, &down] {
                    arrow.add_css_class("flat");
                    arrow.add_css_class("circular");
                    arrow.set_valign(gtk::Align::Center);
                    row.append(arrow);
                }
                for (arrow, step) in [(&up, -1i32), (&down, 1i32)] {
                    let settings = settings.clone();
                    arrow.connect_clicked(move |_| {
                        let mut order = player_bar_order(
                            &settings
                                .strv(PLAYER_BAR_ORDER)
                                .iter()
                                .map(|s| s.to_string())
                                .collect::<Vec<_>>(),
                        );
                        let Some(at) = order.iter().position(|o| *o == id) else {
                            return;
                        };
                        let to = at as i32 + step;
                        if to < 0 || to as usize >= order.len() {
                            return;
                        }
                        order.swap(at, to as usize);
                        let _ = settings.set_strv(PLAYER_BAR_ORDER, order);
                    });
                }
                list.append(&row);
                rows.push((id, widget, row, up, down));
            }

            // Lay the bar and the list out in the saved order, now and on change
            let bar_end = self.bar_end.get();
            let sleep_slot = self.sleep_timer_slot.get();
            let apply = {
                let list = list.clone();
                let title = title.clone();
                move |settings: &gio::Settings| {
                    let order = player_bar_order(
                        &settings
                            .strv(PLAYER_BAR_ORDER)
                            .iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>(),
                    );
                    let mut prev_widget: Option<gtk::Widget> = None;
                    let mut prev_row: gtk::Widget = title.clone().upcast();
                    for (index, id) in order.iter().enumerate() {
                        let Some((_, widget, row, up, down)) =
                            rows.iter().find(|(rid, ..)| rid == id)
                        else {
                            continue;
                        };
                        // The sleep timer button, shown while a timer runs,
                        // stays right before the gear
                        if *id == "more" {
                            bar_end.reorder_child_after(&sleep_slot, prev_widget.as_ref());
                            prev_widget = Some(sleep_slot.clone().upcast());
                        }
                        bar_end.reorder_child_after(widget, prev_widget.as_ref());
                        prev_widget = Some(widget.clone());
                        list.reorder_child_after(row, Some(&prev_row));
                        prev_row = row.clone().upcast();
                        up.set_sensitive(index > 0);
                        down.set_sensitive(index + 1 < order.len());
                    }
                }
            };
            apply(&settings);
            settings.connect_changed(Some(PLAYER_BAR_ORDER), move |settings, _| apply(settings));

            let popover = gtk::Popover::new();
            popover.set_child(Some(&list));
            popover.set_position(gtk::PositionType::Top);
            popover.set_parent(&*self.bar_end);

            // From More Options
            let more = self.eq_popover.clone();
            let pop = popover.clone();
            self.customize_bar_button.connect_clicked(move |_| {
                more.popdown();
                pop.set_pointing_to(None::<&gtk::gdk::Rectangle>);
                pop.popup();
            });
            // And by right-clicking the bar's buttons
            let click = gtk::GestureClick::new();
            click.set_button(gtk::gdk::BUTTON_SECONDARY);
            let pop = popover.clone();
            click.connect_pressed(move |gesture, _, x, y| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                pop.popup();
            });
            self.bar_end.add_controller(click);
            self.customize_popover.replace(Some(popover));
        }

        /// The bar's tiers (see `BAR_TIER_WIDTHS`). libadwaita applies only
        /// the last breakpoint that matches, so each one carries the setters
        /// of the tiers above it, the later value winning. They change the
        /// parts inside the slots, never a slot (`follow_slot`).
        fn setup_bar_tiers(&self) {
            use libadwaita::prelude::BreakpointBinExt;
            let set = |widget: gtk::Widget, property: &'static str, value: glib::Value| {
                (widget, property, value)
            };
            let hide = |widget: gtk::Widget| set(widget, "visible", false.to_value());
            let show = |widget: gtk::Widget| set(widget, "visible", true.to_value());
            let times = [
                self.track_position.get().upcast::<gtk::Widget>(),
                self.track_duration.get().upcast(),
            ];
            let tiers = vec![
                vec![
                    hide(self.volume_box.get().upcast()),
                    show(self.volume_button.get().upcast()),
                    hide(self.device_selector_box.get().upcast()),
                    hide(self.mini_player_button.get().upcast()),
                ],
                vec![
                    hide(self.controls.shuffle_button()),
                    hide(self.controls.repeat_button()),
                    show(self.more_shuffle.get().upcast()),
                    show(self.more_repeat.get().upcast()),
                    show(self.more_toggles_separator.get().upcast()),
                    set(times[0].clone(), "width-chars", 4.to_value()),
                    set(times[1].clone(), "width-chars", 4.to_value()),
                ],
                vec![
                    hide(self.lyrics_button.get().upcast()),
                    hide(self.queue_button.get().upcast()),
                ],
                vec![
                    hide(self.now_playing.like_slot()),
                    hide(self.volume_button.get().upcast()),
                ],
                vec![
                    hide(self.controls.prev_button()),
                    hide(times[0].clone()),
                    hide(times[1].clone()),
                    // No tier is left to make room for a timer's countdown:
                    // it leaves (the timer stays in More Options)
                    hide(self.sleep_timer_button.get().upcast()),
                ],
            ];
            let mut setters: Vec<(gtk::Widget, &str, glib::Value)> = Vec::new();
            let mut breakpoints = self.bar_tiers.borrow_mut();
            for (width, tier) in BAR_TIER_WIDTHS.iter().copied().zip(tiers) {
                for (widget, property, value) in tier {
                    setters.retain(|(w, p, _)| !(*w == widget && *p == property));
                    setters.push((widget, property, value));
                }
                let breakpoint = libadwaita::Breakpoint::new(bar_tier_condition(width, 0.0, None));
                for (widget, property, value) in &setters {
                    breakpoint.add_setter(widget, property, Some(value));
                }
                self.bar_bin.add_breakpoint(breakpoint.clone());
                breakpoints.push((width, breakpoint));
            }
        }

        /// The sleep timer's slot shows while a timer counts down and the
        /// bar's tier leaves its button there.
        pub(super) fn sync_sleep_timer_slot(&self) {
            self.sleep_timer_slot.set_visible(
                self.sleep_timer_label.get_visible() && self.sleep_timer_button.get_visible(),
            );
        }

        /// Makes room in every tier for the sleep timer's countdown while a
        /// timer runs: each tier starts that much wider. The room never
        /// shrinks while it counts: its digits narrowing (10:00 to 9:59), or
        /// the narrowest tier hiding it, must not move the tiers back, which
        /// would close any menu open from the bar.
        pub(super) fn retune_bar_tiers(&self) {
            let room = if self.sleep_timer_label.get_visible() {
                // Nothing while the narrowest tier hides its button
                let width = self
                    .sleep_timer_button
                    .measure(gtk::Orientation::Horizontal, -1)
                    .1;
                let measured = if width > 0 {
                    width + self.bar_end.spacing()
                } else {
                    0
                };
                measured.max(self.bar_tier_room.get())
            } else {
                0
            };
            if self.bar_tier_room.replace(room) == room {
                return;
            }
            let settings = self.obj().settings();
            for (width, breakpoint) in self.bar_tiers.borrow().iter() {
                breakpoint.set_condition(Some(&bar_tier_condition(
                    *width,
                    f64::from(room),
                    Some(&settings),
                )));
            }
        }
    }
    impl BoxImpl for PlaybackWidget {}
}

/// A tier of the bar applies at `width` px and under, or at `width` sp and
/// under when the text is larger than 1.0: the bar's icons keep their size
/// whatever the text scale, while its title and times grow with it (and
/// with text under 1.0 the px keep the tier as safe as at 1.0). Plus `room`
/// px for the sleep timer's button while it shows.
fn bar_tier_condition(
    width: f64,
    room: f64,
    settings: Option<&gtk::Settings>,
) -> BreakpointCondition {
    let max_width = |value, unit| {
        BreakpointCondition::new_length(BreakpointConditionLengthType::MaxWidth, value, unit)
    };
    BreakpointCondition::new_or(
        max_width(width + room, LengthUnit::Px),
        max_width(
            width + LengthUnit::Sp.from_px(room, settings),
            LengthUnit::Sp,
        ),
    )
}

/// A slot of the bar shows while its `player-bar-*` setting is on and
/// something in it shows: an empty slot would still take the bar's spacing,
/// and the gaps between its buttons would go uneven. The tiers (and the
/// device selector's flag) hide what is inside a slot, never the slot.
fn follow_slot(settings: &gio::Settings, key: &'static str, slot: &gtk::Widget) {
    let sync: std::rc::Rc<dyn Fn()> = {
        // Weak, or the handler below would keep the settings (and itself)
        // alive forever; the check buttons of "Show in the Bar" keep them
        // for as long as the bar
        let settings = settings.downgrade();
        let slot = slot.downgrade();
        std::rc::Rc::new(move || {
            if let (Some(settings), Some(slot)) = (settings.upgrade(), slot.upgrade()) {
                slot.set_visible(settings.boolean(key) && children_show(&slot));
            }
        })
    };
    let mut boxes = vec![slot.clone()];
    while let Some(parent) = boxes.pop() {
        let mut child = parent.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            let sync = sync.clone();
            c.connect_visible_notify(move |_| sync());
            if c.type_() == gtk::Box::static_type() {
                boxes.push(c);
            }
        }
    }
    let on_change = sync.clone();
    settings.connect_changed(Some(key), move |_, _| on_change());
    sync();
}

/// Whether anything inside `widget` shows: a plain box through its
/// children, anything else by its own `visible` (not `is_visible`, which
/// is false in a hidden slot whatever the child's own state).
fn children_show(widget: &gtk::Widget) -> bool {
    let mut child = widget.first_child();
    while let Some(c) = child {
        if c.get_visible() && (c.type_() != gtk::Box::static_type() || children_show(&c)) {
            return true;
        }
        child = c.next_sibling();
    }
    false
}

/// Which repeat the More Options row stands for, for screen readers (its
/// check mark shows for both repeats, its icon tells them apart).
fn repeat_description(mode: RepeatMode) -> String {
    match mode {
        // translators: What a screen reader says of the Repeat row in the player bar's More Options menu while the whole album or playlist repeats.
        RepeatMode::Context => gettext("Repeat All"),
        // translators: What a screen reader says of the Repeat row in the player bar's More Options menu while the song playing repeats.
        RepeatMode::Track => gettext("Repeat This Song"),
        RepeatMode::Off => gettext("Off"),
    }
}

/// A More Options row's state, for screen readers.
fn pressed(on: bool) -> gtk::AccessibleTristate {
    if on {
        gtk::AccessibleTristate::True
    } else {
        gtk::AccessibleTristate::False
    }
}

/// Parses the integer boolean field from `hyprctl -j getoption <name>`.
fn parse_hyprctl_bool_option(stdout: &str) -> Option<bool> {
    serde_json::from_str::<serde_json::Value>(stdout)
        .ok()?
        .get("int")
        .and_then(|v| v.as_i64())
        .map(|n| n != 0)
}

fn query_hyprctl_bool_option(option: &str) -> bool {
    std::process::Command::new("hyprctl")
        .args(["-j", "getoption", option])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|stdout| parse_hyprctl_bool_option(&stdout))
        .unwrap_or(false)
}

/// Reads Hyprland's `(touchpad, pointer)` `natural_scroll` settings, cached
/// for a few seconds so rapid scroll events never spawn repeated commands.
fn hyprland_natural_scroll() -> (bool, bool) {
    static CACHE: std::sync::Mutex<Option<(std::time::Instant, (bool, bool))>> =
        std::sync::Mutex::new(None);
    let now = std::time::Instant::now();
    if let Ok(cache) = CACHE.lock() {
        if let Some((at, pair)) = *cache {
            if now.duration_since(at) < std::time::Duration::from_secs(10) {
                return pair;
            }
        }
    }
    let pair = (
        query_hyprctl_bool_option("input:touchpad:natural_scroll"),
        query_hyprctl_bool_option("input:natural_scroll"),
    );
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some((now, pair));
    }
    pair
}

/// Whether `event`'s scroll delta is inverted relative to physical motion.
/// Wayland compositors should report this through `relative_direction`, but some
/// (such as Hyprland) always report `Identical`, so we fall back to checking the
/// compositor's `natural_scroll` setting for touchpad (`Surface`) vs mouse (`Wheel`).
fn is_natural_scroll(
    relative: gtk::gdk::ScrollRelativeDirection,
    unit: gtk::gdk::ScrollUnit,
    hyprland_fallback: Option<(bool, bool)>,
) -> bool {
    if relative == gtk::gdk::ScrollRelativeDirection::Inverted {
        true
    } else if let Some((touchpad, pointer)) = hyprland_fallback {
        match unit {
            gtk::gdk::ScrollUnit::Surface => touchpad,
            _ => pointer,
        }
    } else {
        false
    }
}

/// Steps of the wheel or the touchpad as the hand moved them (negative:
/// up). With natural scrolling the fingers going up send a scroll down.
fn scroll_steps(dy: f64, natural: bool) -> f64 {
    if natural {
        -dy
    } else {
        dy
    }
}

/// The volume after `steps` of the wheel or the touchpad (negative: up),
/// each the step of Ctrl+Up and Ctrl+Down.
fn volume_after_scroll(value: f64, steps: f64) -> f64 {
    (value - steps * crate::VOLUME_STEP).clamp(0.0, 1.0)
}

/// The volume's tooltip: its level, and the keys that change it.
fn volume_tooltip(value: f64) -> String {
    // translators: Tooltip of the volume slider. {} is the volume in percent (the % follows it); Ctrl+↑ and Ctrl+↓ are the keys that change it.
    gettext("Volume {}% · Ctrl+↑ / Ctrl+↓")
        .replace("{}", &((value * 100.0).round() as i32).to_string())
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
    /// The volume slider, the box with the speaker and the slider, and the
    /// narrow bar's volume button, for the GTK checks.
    #[cfg(test)]
    pub fn volume_parts_for_tests(&self) -> (gtk::Scale, gtk::Widget, gtk::Widget) {
        let imp = self.imp();
        (
            imp.volume_slider.get(),
            imp.volume_box.get().upcast(),
            imp.volume_button.get().upcast(),
        )
    }

    pub fn device_selector(&self) -> DeviceSelectorWidget {
        self.imp().device_selector.clone()
    }

    /// Markups from `now_panel::title_markup` and `now_panel::artists_markup`.
    pub fn set_track(&self, title_markup: &str, artists_markup: &str) {
        let widget = self.imp();
        widget.now_playing.set_visible(true);
        widget.now_playing.set_track(title_markup, artists_markup);
    }

    pub fn set_liked(&self, liked: Option<bool>) {
        self.imp().now_playing.set_liked(liked);
    }

    pub fn reset_info(&self) {
        let widget = self.imp();
        widget.now_playing.set_visible(true);
        widget.now_playing.reset_info();
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
        let format = if widget.short_times.get() {
            format_duration_short
        } else {
            format_duration
        };
        widget.track_position.set_text(&format(pos));
        widget.track_duration.set_text(&format(duration));
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
        // And its row in More Options, for a bar too narrow for the button
        let on = mode != RepeatMode::Off;
        widget
            .more_repeat_icon
            .set_icon_name(Some(repeat_icon_name(mode)));
        widget.more_repeat_check.set_visible(on);
        widget
            .more_repeat
            .update_state(&[gtk::accessible::State::Pressed(pressed(on))]);
        widget
            .more_repeat
            .update_property(
                &[gtk::accessible::Property::Description(&repeat_description(
                    mode,
                ))],
            );
    }

    pub fn set_shuffled(&self, shuffled: bool) {
        let widget = self.imp();
        widget.controls.set_shuffled(shuffled);
        widget.more_shuffle_check.set_visible(shuffled);
        widget
            .more_shuffle
            .update_state(&[gtk::accessible::State::Pressed(pressed(shuffled))]);
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
            // Its icon and time, without the pill's wide padding: a window
            // on half of the screen keeps its like and volume buttons
            imp.sleep_timer_button.remove_css_class("circular");
            imp.sleep_timer_button.add_css_class("sleep-timer-counting");
        } else {
            imp.sleep_timer_label.set_visible(false);
            imp.sleep_timer_button
                .remove_css_class("sleep-timer-counting");
            imp.sleep_timer_button.add_css_class("circular");
            imp.sleep_timer_button.set_width_request(36);
        }
        imp.sync_sleep_timer_slot();

        // The choice that runs, in the countdown's menu and in More Options
        for (mode, choices) in self.sleep_timer_choices() {
            for btn in choices {
                if mode == snap.mode {
                    btn.add_css_class("suggested-action");
                } else {
                    btn.remove_css_class("suggested-action");
                }
            }
        }
        // The countdown's width changes with its digits and its style
        imp.retune_bar_tiers();
    }

    /// Each sleep timer choice, with its buttons in the countdown's menu and
    /// in More Options.
    fn sleep_timer_choices(&self) -> [(SleepTimerMode, [gtk::Button; 2]); 6] {
        let imp = self.imp();
        [
            (
                SleepTimerMode::Off,
                [imp.sleep_off_btn.get(), imp.more_sleep_off.get()],
            ),
            (
                SleepTimerMode::EndOfTrack,
                [imp.sleep_track_btn.get(), imp.more_sleep_track.get()],
            ),
            (
                SleepTimerMode::Minutes(15),
                [imp.sleep_15_btn.get(), imp.more_sleep_15.get()],
            ),
            (
                SleepTimerMode::Minutes(30),
                [imp.sleep_30_btn.get(), imp.more_sleep_30.get()],
            ),
            (
                SleepTimerMode::Minutes(45),
                [imp.sleep_45_btn.get(), imp.more_sleep_45.get()],
            ),
            (
                SleepTimerMode::Minutes(60),
                [imp.sleep_60_btn.get(), imp.more_sleep_60.get()],
            ),
        ]
    }

    pub fn connect_sleep_timer<F: Fn(SleepTimerMode) + Clone + 'static>(&self, on_select: F) {
        let imp = self.imp();
        let menus = [imp.sleep_popover.get(), imp.eq_popover.get()];
        for (mode, choices) in self.sleep_timer_choices() {
            for (btn, menu) in choices.iter().zip(menus.iter()) {
                let menu = menu.clone();
                let cb = on_select.clone();
                btn.connect_clicked(move |_| {
                    cb(mode);
                    menu.popdown();
                });
            }
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
    fn the_wheel_moves_the_volume_a_key_step_at_a_time() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(close(volume_after_scroll(0.40, -1.0), 0.45));
        assert!(close(volume_after_scroll(0.40, 2.0), 0.30));
        assert!(close(volume_after_scroll(0.98, -1.0), 1.0));
        assert!(close(volume_after_scroll(0.02, 1.0), 0.0));
        assert_eq!(volume_tooltip(0.45), "Volume 45% · Ctrl+↑ / Ctrl+↓");
        // A wheel up, and fingers up with natural scrolling: louder
        assert!(close(
            volume_after_scroll(0.40, scroll_steps(-1.0, false)),
            0.45
        ));
        assert!(close(
            volume_after_scroll(0.40, scroll_steps(1.0, true)),
            0.45
        ));
        assert_eq!(volume_tooltip(0.0), "Volume 0% · Ctrl+↑ / Ctrl+↓");
    }

    #[test]
    fn hyprland_fallback_detects_touchpad_natural_scroll_when_relative_stays_identical() {
        use gtk::gdk::{ScrollRelativeDirection as Rel, ScrollUnit};
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;

        assert_eq!(
            parse_hyprctl_bool_option(
                r#"{"option":"input:touchpad:natural_scroll","int":1,"set":true}"#
            ),
            Some(true)
        );
        assert_eq!(
            parse_hyprctl_bool_option(r#"{"option":"input:natural_scroll","int":0,"set":false}"#),
            Some(false)
        );
        assert_eq!(parse_hyprctl_bool_option("{}"), None);
        assert_eq!(parse_hyprctl_bool_option("not json"), None);

        assert!(is_natural_scroll(Rel::Inverted, ScrollUnit::Wheel, None));
        assert!(is_natural_scroll(
            Rel::Inverted,
            ScrollUnit::Surface,
            Some((false, false))
        ));
        assert!(is_natural_scroll(
            Rel::Identical,
            ScrollUnit::Surface,
            Some((true, false))
        ));
        assert!(!is_natural_scroll(
            Rel::Identical,
            ScrollUnit::Wheel,
            Some((true, false))
        ));
        assert!(!is_natural_scroll(
            Rel::Identical,
            ScrollUnit::Surface,
            None
        ));

        // Fingers up on a touchpad with natural scroll (dy=+1, Identical on Hyprland): louder
        let natural = is_natural_scroll(Rel::Identical, ScrollUnit::Surface, Some((true, false)));
        assert!(close(
            volume_after_scroll(0.40, scroll_steps(1.0, natural)),
            0.45
        ));
        // Mouse wheel up without natural scroll (dy=-1): louder
        let mouse_natural =
            is_natural_scroll(Rel::Identical, ScrollUnit::Wheel, Some((true, false)));
        assert!(close(
            volume_after_scroll(0.40, scroll_steps(-1.0, mouse_natural)),
            0.45
        ));
    }

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
