use crate::app::components::{labels, EventListener};
use crate::app::AppEvent;
use crate::settings::RiffSettings;
use gettextrs::gettext;
use riff_api::scrobbler::{build_auth_url, LastFmClient};
use riff_config::feature_flags::FeatureFlag;
use std::cell::RefCell;

use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib, CompositeTemplate};
use libadwaita::prelude::*;

use super::EqualizerWidget;
use super::PanWidget;
use super::PitchWidget;
use super::SettingsModel;

use riff_config::settings::{
    GENIUS_ACCESS_TOKEN, LASTFM_API_KEY, LASTFM_API_SECRET, LASTFM_SESSION_KEY, LASTFM_USERNAME,
    LISTENBRAINZ_TOKEN, RESTART_REQUIRED_KEYS, SCROBBLE_ENABLED, SCROBBLE_RESPECT_REMOTE, SETTINGS,
    SMART_QUEUE_CONTINUATION, VISUAL_BRIDGE_ENABLED,
};

mod imp {

    use super::*;
    use libadwaita::subclass::prelude::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/settings.ui")]
    pub struct SettingsDialog {
        #[template_child]
        pub general_page: TemplateChild<libadwaita::PreferencesPage>,

        #[template_child]
        pub player_bitrate: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub alsa_device: TemplateChild<gtk::Entry>,

        #[template_child]
        pub alsa_device_row: TemplateChild<libadwaita::ActionRow>,

        #[template_child]
        pub audio_backend: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub gapless_playback: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub ap_port: TemplateChild<gtk::Entry>,

        #[template_child]
        pub language: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub app_theme: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub theme: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub adaptive_cover_colors: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub queue_side_panel_switch: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub lyrics_offset_ms: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub close_behavior: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub inhibit_suspend_switch: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub skip_explicit_switch: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub smart_queue_continuation_switch: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub shuffle_separation: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub volume_curve: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub mono_audio_switch: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub audio_format: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub normalisation_group: TemplateChild<libadwaita::PreferencesGroup>,

        #[template_child]
        pub normalisation_switch: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub normalisation_type: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub normalisation_method: TemplateChild<libadwaita::ComboRow>,

        #[template_child]
        pub normalisation_pregain: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub normalisation_threshold: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub normalisation_attack: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub normalisation_release: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub normalisation_knee: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub equalizer: TemplateChild<EqualizerWidget>,

        #[template_child]
        pub pan: TemplateChild<PanWidget>,

        #[template_child]
        pub pitch: TemplateChild<PitchWidget>,

        #[template_child]
        pub memory_cache_size: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub disk_cache_size: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub audio_cache_size: TemplateChild<libadwaita::SpinRow>,

        #[template_child]
        pub clear_cache_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub genius_guide_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub genius_token_entry: TemplateChild<libadwaita::PasswordEntryRow>,
        #[template_child]
        pub google_translate_key_entry: TemplateChild<libadwaita::PasswordEntryRow>,

        #[template_child]
        pub scrobbling_guide_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub scrobble_enabled_switch: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub scrobble_respect_remote_switch: TemplateChild<libadwaita::SwitchRow>,

        #[template_child]
        pub lastfm_api_key_entry: TemplateChild<libadwaita::PasswordEntryRow>,

        #[template_child]
        pub lastfm_api_secret_entry: TemplateChild<libadwaita::PasswordEntryRow>,

        #[template_child]
        pub lastfm_auth_row: TemplateChild<libadwaita::ActionRow>,

        #[template_child]
        pub lastfm_step1_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub lastfm_step2_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub lastfm_disconnect_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub listenbrainz_token_entry: TemplateChild<libadwaita::PasswordEntryRow>,

        #[template_child]
        pub visual_bridge_switch: TemplateChild<libadwaita::SwitchRow>,

        pub lastfm_pending_token: RefCell<Option<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SettingsDialog {
        const NAME: &'static str = "SettingsWindow";
        type Type = super::SettingsDialog;
        type ParentType = libadwaita::PreferencesDialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SettingsDialog {}
    impl WidgetImpl for SettingsDialog {}
    impl AdwDialogImpl for SettingsDialog {}
    impl PreferencesDialogImpl for SettingsDialog {}
}

glib::wrapper! {
    pub struct SettingsDialog(ObjectSubclass<imp::SettingsDialog>) @extends gtk::Widget, libadwaita::Dialog, libadwaita::PreferencesDialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for SettingsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsDialog {
    pub fn new() -> Self {
        let dialog: Self = glib::Object::new();

        dialog.bind_backend_and_device();
        dialog.bind_settings();
        dialog.setup_bitrate_lossless_factory();
        dialog.bind_feature_flags();
        dialog.connect_theme_select();
        dialog.setup_integrations();
        dialog
    }

    fn bind_backend_and_device(&self) {
        let widget = self.imp();

        let audio_backend = widget
            .audio_backend
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        let alsa_device_row = widget
            .alsa_device_row
            .downcast_ref::<libadwaita::ActionRow>()
            .unwrap();

        audio_backend
            .bind_property("selected", alsa_device_row, "visible")
            .transform_to(|_, value: u32| Some(value == 1))
            .build();

        if audio_backend.selected() == 0 {
            alsa_device_row.set_visible(false);
        }
    }

    /// Installs a list factory on the Audio Quality combo row that renders the
    /// "Lossless" entry as a disabled option: its label is dimmed, it cannot be
    /// selected, and a warning icon is shown next to it. Hovering the warning
    /// icon reveals a tooltip explaining why the option is unavailable.
    fn setup_bitrate_lossless_factory(&self) {
        // Index of the "Lossless" entry in the Audio Quality string list.
        const LOSSLESS_INDEX: u32 = 3;

        let player_bitrate = self
            .imp()
            .player_bitrate
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();

        let factory = gtk::SignalListItemFactory::new();

        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);

            let label = gtk::Label::new(None);
            label.set_xalign(0.0);
            label.set_hexpand(true);

            let warning = gtk::Image::from_icon_name("dialog-warning-symbolic");
            warning.set_visible(false);

            row.append(&label);
            row.append(&warning);
            item.set_child(Some(&row));
        });

        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let Some(row) = item.child().and_downcast::<gtk::Box>() else {
                return;
            };
            let Some(label) = row.first_child().and_downcast::<gtk::Label>() else {
                return;
            };
            let Some(warning) = row.last_child().and_downcast::<gtk::Image>() else {
                return;
            };

            let text = item
                .item()
                .and_downcast::<gtk::StringObject>()
                .map(|s| s.string().to_string())
                .unwrap_or_default();
            label.set_label(&text);

            if item.position() == LOSSLESS_INDEX {
                label.add_css_class("dim-label");
                warning.set_visible(true);
                row.set_tooltip_text(Some(&gettextrs::gettext(
                    "Unsupported due to legal restrictions.",
                )));
                // Prevent the entry from being chosen from the dropdown.
                item.set_selectable(false);
                item.set_activatable(false);
            } else {
                // Reset state in case this list item widget is being reused.
                label.remove_css_class("dim-label");
                warning.set_visible(false);
                row.set_tooltip_text(None);
                item.set_selectable(true);
                item.set_activatable(true);
            }
        });

        // Only the popup list needs the custom factory; the selected value shown
        // on the row keeps the combo row's default rendering.
        player_bitrate.set_list_factory(Some(&factory));
    }

    fn bind_settings(&self) {
        let widget = self.imp();
        let settings = gio::Settings::new(SETTINGS);

        // Binds a GtkAdjustment (from a SpinRow or Scale) to a double GSettings key.
        //
        // We deliberately avoid `Settings::bind` here: its bidirectional binding
        // causes a feedback loop with spin/scale widgets where a single step is
        // written to GSettings and then immediately reverted by the `changed`
        // write-back. Instead we load the initial value and write on every change.
        let bind_double_adjustment = |key: &str, adjustment: &gtk::Adjustment| {
            adjustment.set_value(settings.double(key));
            let settings = settings.clone();
            let key = key.to_owned();
            adjustment.connect_value_changed(move |adj| {
                let _ = settings.set_double(&key, adj.value());
            });
        };

        let player_bitrate = widget
            .player_bitrate
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        // "Lossless" is shown as a disabled, non-selectable option (see
        // setup_bitrate_lossless_factory), so it never reaches the mapping below.
        settings
            .bind("player-bitrate", player_bitrate, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "96" => 0,
                        "160" => 1,
                        "320" => 2,
                        _ => unreachable!(),
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().and_then(|u| {
                    match u {
                        0 => Some("96"),
                        1 => Some("160"),
                        2 => Some("320"),
                        // "Lossless" cannot be selected, so ignore any other index.
                        _ => None,
                    }
                    .map(|s| s.to_variant())
                })
            })
            .build();

        let alsa_device = widget.alsa_device.downcast_ref::<gtk::Entry>().unwrap();
        settings.bind("alsa-device", alsa_device, "text").build();

        let audio_backend = widget
            .audio_backend
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind("audio-backend", audio_backend, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "pulseaudio" => 0,
                        "alsa" => 1,
                        _ => unreachable!(),
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "pulseaudio",
                        1 => "alsa",
                        _ => unreachable!(),
                    }
                    .to_variant()
                })
            })
            .build();

        let gapless_playback = widget
            .gapless_playback
            .downcast_ref::<libadwaita::SwitchRow>()
            .unwrap();
        settings
            .bind("gapless-playback", gapless_playback, "active")
            .build();

        let ap_port = widget.ap_port.downcast_ref::<gtk::Entry>().unwrap();
        settings
            .bind("ap-port", ap_port, "text")
            .mapping(|variant, _| variant.get::<u32>().map(|s| s.to_value()))
            .set_mapping(|value, _| value.get::<u32>().ok().map(|u| u.to_variant()))
            .build();

        // Volume curve
        let volume_curve = widget
            .volume_curve
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind("volume-curve", volume_curve, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "log" => 0,
                        "linear" => 1,
                        "cubic" => 2,
                        _ => 0,
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "log",
                        1 => "linear",
                        2 => "cubic",
                        _ => "log",
                    }
                    .to_variant()
                })
            })
            .build();

        // Mono audio
        let mono_audio_switch = widget
            .mono_audio_switch
            .downcast_ref::<libadwaita::SwitchRow>()
            .unwrap();
        settings
            .bind("mono-audio", mono_audio_switch, "active")
            .build();

        // Audio format
        let audio_format = widget
            .audio_format
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind("audio-format", audio_format, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "s16" => 0,
                        "s24" => 1,
                        "s24_3" => 2,
                        "s32" => 3,
                        "f32" => 4,
                        "f64" => 5,
                        _ => 0,
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "s16",
                        1 => "s24",
                        2 => "s24_3",
                        3 => "s32",
                        4 => "f32",
                        5 => "f64",
                        _ => "s16",
                    }
                    .to_variant()
                })
            })
            .build();

        // Normalisation
        let normalisation_switch = widget
            .normalisation_switch
            .downcast_ref::<libadwaita::SwitchRow>()
            .unwrap();
        settings
            .bind("normalisation", normalisation_switch, "active")
            .build();

        let normalisation_type = widget
            .normalisation_type
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind("normalisation-type", normalisation_type, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "auto" => 0,
                        "track" => 1,
                        "album" => 2,
                        _ => 0,
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "auto",
                        1 => "track",
                        2 => "album",
                        _ => "auto",
                    }
                    .to_variant()
                })
            })
            .build();

        let normalisation_method = widget
            .normalisation_method
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind("normalisation-method", normalisation_method, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "dynamic" => 0,
                        "basic" => 1,
                        _ => 0,
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "dynamic",
                        1 => "basic",
                        _ => "dynamic",
                    }
                    .to_variant()
                })
            })
            .build();

        let normalisation_pregain = widget
            .normalisation_pregain
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        bind_double_adjustment(
            "normalisation-pregain-db",
            &normalisation_pregain.adjustment(),
        );

        let normalisation_threshold = widget
            .normalisation_threshold
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        bind_double_adjustment(
            "normalisation-threshold-dbfs",
            &normalisation_threshold.adjustment(),
        );

        let normalisation_attack = widget
            .normalisation_attack
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        bind_double_adjustment(
            "normalisation-attack-ms",
            &normalisation_attack.adjustment(),
        );

        let normalisation_release = widget
            .normalisation_release
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        bind_double_adjustment(
            "normalisation-release-ms",
            &normalisation_release.adjustment(),
        );

        let normalisation_knee = widget
            .normalisation_knee
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        bind_double_adjustment("normalisation-knee-db", &normalisation_knee.adjustment());

        let language = widget.language.downcast_ref::<libadwaita::ComboRow>().unwrap();
        settings
            .bind(riff_config::settings::APP_LANGUAGE, language, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "system" => 0,
                        "en" => 1,
                        "pt_BR" => 2,
                        _ => 0,
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "system",
                        1 => "en",
                        2 => "pt_BR",
                        _ => "system",
                    }
                    .to_variant()
                })
            })
            .build();

        let app_theme = widget
            .app_theme
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind(riff_config::settings::APP_THEME, app_theme, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "omarchy" => 0,
                        "neutral" => 1,
                        "spotify-green" => 2,
                        "oled-dark" => 3,
                        "warm-vinyl" => 4,
                        "purple-night" => 5,
                        _ => 0,
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "omarchy",
                        1 => "neutral",
                        2 => "spotify-green",
                        3 => "oled-dark",
                        4 => "warm-vinyl",
                        5 => "purple-night",
                        _ => "omarchy",
                    }
                    .to_variant()
                })
            })
            .build();

        let theme = widget.theme.downcast_ref::<libadwaita::ComboRow>().unwrap();
        settings
            .bind("theme-preference", theme, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "light" => 0,
                        "dark" => 1,
                        "system" => 2,
                        _ => unreachable!(),
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "light",
                        1 => "dark",
                        2 => "system",
                        _ => unreachable!(),
                    }
                    .to_variant()
                })
            })
            .build();

        settings
            .bind(
                riff_config::settings::ADAPTIVE_COVER_COLORS,
                &*widget.adaptive_cover_colors,
                "active",
            )
            .build();

        settings
            .bind(
                crate::settings::QUEUE_SIDE_PANEL,
                &*widget.queue_side_panel_switch,
                "active",
            )
            .build();

        let lyrics_offset = widget
            .lyrics_offset_ms
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        settings
            .bind(
                riff_config::settings::LYRICS_OFFSET_MS,
                &lyrics_offset.adjustment(),
                "value",
            )
            .mapping(|variant, _| variant.get::<i32>().map(|v| f64::from(v).to_value()))
            .set_mapping(|value, _| {
                value
                    .get::<f64>()
                    .ok()
                    .map(|f| (f.round() as i32).clamp(-10_000, 10_000).to_variant())
            })
            .build();

        let close_behavior = widget
            .close_behavior
            .downcast_ref::<libadwaita::ComboRow>()
            .unwrap();
        settings
            .bind("close-window-behavior", close_behavior, "selected")
            .mapping(|variant, _| {
                variant.str().map(|s| {
                    match s {
                        "ask" => 0,
                        "minimize-to-background" => 1,
                        "stop-and-quit" => 2,
                        _ => unreachable!(),
                    }
                    .to_value()
                })
            })
            .set_mapping(|value, _| {
                value.get::<u32>().ok().map(|u| {
                    match u {
                        0 => "ask",
                        1 => "minimize-to-background",
                        2 => "stop-and-quit",
                        _ => unreachable!(),
                    }
                    .to_variant()
                })
            })
            .build();

        // Keep device awake while playing (inhibit automatic system suspend).
        let inhibit_suspend_switch = widget
            .inhibit_suspend_switch
            .downcast_ref::<libadwaita::SwitchRow>()
            .expect("inhibit_suspend_switch must be a SwitchRow");
        settings
            .bind("inhibit-suspend", inhibit_suspend_switch, "active")
            .build();

        // Skip explicit tracks (local preference). When the account locks the
        // filter, `show_self` overrides this switch to on and disables it.
        let skip_explicit_switch = widget
            .skip_explicit_switch
            .downcast_ref::<libadwaita::SwitchRow>()
            .expect("skip_explicit_switch must be a SwitchRow");
        settings
            .bind("skip-explicit", skip_explicit_switch, "active")
            .build();

        let separation_row = widget.shuffle_separation.get();
        let separation_adjustment = separation_row.adjustment();
        let separation = settings.uint("shuffle-separation");
        separation_adjustment.set_value(separation as f64);
        separation_row.set_subtitle(&labels::shuffle_separation_label(separation));
        {
            let settings = settings.clone();
            let separation_row = separation_row.downgrade();
            separation_adjustment.connect_value_changed(move |adj| {
                let separation = adj.value() as u32;
                if let Some(row) = separation_row.upgrade() {
                    row.set_subtitle(&labels::shuffle_separation_label(separation));
                }
                let _ = settings.set_uint("shuffle-separation", separation);
            });
        }

        // Memory cache size (MB). Bound manually rather than via `Settings::bind`
        // for the same spin-row feedback-loop reason noted above, and because the
        // key is a uint while the adjustment is a double.
        let memory_cache_size = widget
            .memory_cache_size
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        let cache_adjustment = memory_cache_size.adjustment();
        cache_adjustment.set_value(settings.uint("memory-cache-size-mb") as f64);
        {
            let settings = settings.clone();
            cache_adjustment.connect_value_changed(move |adj| {
                let _ = settings.set_uint("memory-cache-size-mb", adj.value() as u32);
            });
        }

        // Disk (image) cache size (MB). Same manual-binding rationale as above.
        let disk_cache_size = widget
            .disk_cache_size
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        let disk_adjustment = disk_cache_size.adjustment();
        disk_adjustment.set_value(settings.uint("disk-cache-size-mb") as f64);
        {
            let settings = settings.clone();
            disk_adjustment.connect_value_changed(move |adj| {
                let _ = settings.set_uint("disk-cache-size-mb", adj.value() as u32);
            });
        }

        // Audio cache on disk size (MB).
        let audio_cache_size = widget
            .audio_cache_size
            .downcast_ref::<libadwaita::SpinRow>()
            .unwrap();
        let audio_adjustment = audio_cache_size.adjustment();
        audio_adjustment.set_value(settings.uint("audio-cache-size-mb") as f64);
        {
            let settings = settings.clone();
            audio_adjustment.connect_value_changed(move |adj| {
                let _ = settings.set_uint("audio-cache-size-mb", adj.value() as u32);
            });
        }

        // Smart Queue Continuation
        settings
            .bind(
                SMART_QUEUE_CONTINUATION,
                &*widget.smart_queue_continuation_switch,
                "active",
            )
            .build();

        // Integrations (Genius, Scrobbling, Visual Bridge)
        settings
            .bind(GENIUS_ACCESS_TOKEN, &*widget.genius_token_entry, "text")
            .build();
        settings
            .bind(
                riff_config::settings::GOOGLE_TRANSLATE_API_KEY,
                &*widget.google_translate_key_entry,
                "text",
            )
            .build();
        settings
            .bind(SCROBBLE_ENABLED, &*widget.scrobble_enabled_switch, "active")
            .build();
        settings
            .bind(
                SCROBBLE_RESPECT_REMOTE,
                &*widget.scrobble_respect_remote_switch,
                "active",
            )
            .build();
        settings
            .bind(LASTFM_API_KEY, &*widget.lastfm_api_key_entry, "text")
            .build();
        settings
            .bind(LASTFM_API_SECRET, &*widget.lastfm_api_secret_entry, "text")
            .build();
        settings
            .bind(LISTENBRAINZ_TOKEN, &*widget.listenbrainz_token_entry, "text")
            .build();
        settings
            .bind(VISUAL_BRIDGE_ENABLED, &*widget.visual_bridge_switch, "active")
            .build();
    }

    fn bind_feature_flags(&self) {
        let settings = gio::Settings::new(SETTINGS);
        let group = libadwaita::PreferencesGroup::new();
        group.set_title(&gettextrs::gettext("Experimental Features"));
        group.set_description(Some(&gettextrs::gettext(
            "These features are still in development and may be unstable or incomplete.",
        )));

        settings
            .bind(
                FeatureFlag::Normalisation.key(),
                &*self.imp().normalisation_group,
                "visible",
            )
            .get()
            .build();

        for flag in FeatureFlag::ALL.iter() {
            let row = libadwaita::SwitchRow::new();
            row.set_title(flag.title());
            row.set_subtitle(flag.description());
            settings.bind(flag.key(), &row, "active").build();
            group.add(&row);
        }

        self.imp().general_page.add(&group);
    }

    fn connect_theme_select(&self) {
        let widget = self.imp();
        let theme = widget.theme.downcast_ref::<libadwaita::ComboRow>().unwrap();
        let settings = gio::Settings::new(SETTINGS);
        settings
            .bind(riff_config::settings::APP_THEME, theme, "sensitive")
            .flags(gio::SettingsBindFlags::GET)
            .mapping(|variant, _| {
                let is_neutral = variant.get::<String>().is_some_and(|s| s == "neutral");
                Some(is_neutral.to_value())
            })
            .build();
        theme.connect_selected_notify(|theme| {
            debug!("Theme switched! --> value: {}", theme.selected());
            let settings = gio::Settings::new(SETTINGS);
            if settings.string(riff_config::settings::APP_THEME) != "neutral" {
                return;
            }
            let manager = libadwaita::StyleManager::default();

            let pref = match theme.selected() {
                0 => libadwaita::ColorScheme::ForceLight,
                1 => libadwaita::ColorScheme::ForceDark,
                _ => libadwaita::ColorScheme::Default,
            };

            manager.set_color_scheme(pref);
        });
    }

    fn connect_closed<F>(&self, on_closed: F)
    where
        F: Fn() + 'static,
    {
        let dialog = self.upcast_ref::<libadwaita::Dialog>();
        dialog.connect_closed(move |_| on_closed());
    }

    fn connect_close<F>(&self, on_close: F)
    where
        F: Fn() + 'static,
    {
        let dialog = self.upcast_ref::<libadwaita::Dialog>();
        dialog.connect_close_attempt(move |_| {
            on_close();
        });
    }

    /// Connect the "Clear Cache" button to the given handler.
    fn connect_clear_cache<F>(&self, on_clear: F)
    where
        F: Fn() + 'static,
    {
        self.imp()
            .clear_cache_button
            .connect_clicked(move |_| on_clear());
    }

    fn setup_integrations(&self) {
        let imp = self.imp();
        let settings = gio::Settings::new(SETTINGS);

        let weak_self = self.downgrade();
        imp.genius_guide_button.connect_clicked(move |_| {
            if let Some(dialog) = weak_self.upgrade() {
                present_genius_guide_dialog(&dialog);
            }
        });

        let weak_self = self.downgrade();
        imp.scrobbling_guide_button.connect_clicked(move |_| {
            if let Some(dialog) = weak_self.upgrade() {
                present_scrobbling_guide_dialog(&dialog);
            }
        });

        let weak_self = self.downgrade();
        let disconnect_settings = settings.clone();
        imp.lastfm_disconnect_btn.connect_clicked(move |_| {
            let _ = disconnect_settings.set_string(LASTFM_SESSION_KEY, "");
            let _ = disconnect_settings.set_string(LASTFM_USERNAME, "");
            if let Some(dialog) = weak_self.upgrade() {
                *dialog.imp().lastfm_pending_token.borrow_mut() = None;
                dialog.refresh_lastfm_auth_row(&disconnect_settings);
            }
        });

        for key in [
            LASTFM_API_KEY,
            LASTFM_API_SECRET,
            LASTFM_SESSION_KEY,
            LASTFM_USERNAME,
        ] {
            let weak_self = self.downgrade();
            let clear_pending = key == LASTFM_API_KEY || key == LASTFM_API_SECRET;
            settings.connect_changed(Some(key), move |s, _| {
                if let Some(dialog) = weak_self.upgrade() {
                    if clear_pending {
                        *dialog.imp().lastfm_pending_token.borrow_mut() = None;
                    }
                    dialog.refresh_lastfm_auth_row(s);
                }
            });
        }

        self.refresh_lastfm_auth_row(&settings);
    }

    fn refresh_lastfm_auth_row(&self, settings: &gio::Settings) {
        let imp = self.imp();
        let api_key = settings.string(LASTFM_API_KEY);
        let api_secret = settings.string(LASTFM_API_SECRET);
        let session_key = settings.string(LASTFM_SESSION_KEY);
        let username = settings.string(LASTFM_USERNAME);
        let has_creds = !api_key.trim().is_empty() && !api_secret.trim().is_empty();
        let has_session = !session_key.trim().is_empty();
        let has_pending = imp.lastfm_pending_token.borrow().is_some();

        if has_session {
            let status = if username.trim().is_empty() {
                gettext("Connected to Last.fm")
            } else {
                format!("{} {}", gettext("Connected as"), username.trim())
            };
            imp.lastfm_auth_row.set_subtitle(&status);
            imp.lastfm_step1_btn.set_sensitive(false);
            imp.lastfm_step2_btn.set_sensitive(false);
            imp.lastfm_disconnect_btn.set_visible(true);
        } else {
            imp.lastfm_disconnect_btn.set_visible(false);
            imp.lastfm_step1_btn.set_sensitive(has_creds);
            imp.lastfm_step2_btn.set_sensitive(has_creds && has_pending);
            let subtitle = if has_pending {
                gettext("Step 1 opened in browser. Approve access, then click 2. Complete.")
            } else if has_creds {
                gettext("Click 1. Open Auth to approve Canto in your browser")
            } else {
                gettext("Enter API Key and Secret above, then connect your account")
            };
            imp.lastfm_auth_row.set_subtitle(&subtitle);
        }
    }

    fn connect_lastfm_auth(&self, client: riff_api::HttpClient, notify: Rc<dyn Fn(String)>) {
        let imp = self.imp();

        let weak_self = self.downgrade();
        let client_step1 = client.clone();
        let notify_step1 = Rc::clone(&notify);
        imp.lastfm_step1_btn.connect_clicked(move |btn| {
            let settings = gio::Settings::new(SETTINGS);
            let api_key = settings.string(LASTFM_API_KEY).trim().to_string();
            let api_secret = settings.string(LASTFM_API_SECRET).trim().to_string();
            if api_key.is_empty() || api_secret.is_empty() {
                notify_step1(gettext("Enter your Last.fm API Key and Secret first"));
                return;
            }

            btn.set_sensitive(false);
            let client = client_step1.clone();
            let weak_dialog = weak_self.clone();
            let notify_err = Rc::clone(&notify_step1);

            glib::spawn_future_local(async move {
                let res = LastFmClient::get_token(&client, &api_key, &api_secret).await;
                let Some(dialog) = weak_dialog.upgrade() else {
                    return;
                };
                match res {
                    Ok(token) => {
                        let url = build_auth_url(&api_key, &token);
                        *dialog.imp().lastfm_pending_token.borrow_mut() = Some(token);
                        open_external_url(&dialog, &url);
                        let s = gio::Settings::new(SETTINGS);
                        dialog.refresh_lastfm_auth_row(&s);
                    }
                    Err(_) => {
                        let s = gio::Settings::new(SETTINGS);
                        dialog.refresh_lastfm_auth_row(&s);
                        notify_err(gettext(
                            "Could not start Last.fm authorization. Check API credentials and network.",
                        ));
                    }
                }
            });
        });

        let weak_self = self.downgrade();
        imp.lastfm_step2_btn.connect_clicked(move |btn| {
            let Some(dialog) = weak_self.upgrade() else {
                return;
            };
            let Some(token) = dialog.imp().lastfm_pending_token.borrow().clone() else {
                notify(gettext("Run Step 1 first to open browser authorization"));
                return;
            };

            let settings = gio::Settings::new(SETTINGS);
            let api_key = settings.string(LASTFM_API_KEY).trim().to_string();
            let api_secret = settings.string(LASTFM_API_SECRET).trim().to_string();
            if api_key.is_empty() || api_secret.is_empty() {
                notify(gettext("Enter your Last.fm API Key and Secret first"));
                return;
            }

            btn.set_sensitive(false);
            let client = client.clone();
            let weak_dialog = weak_self.clone();
            let notify_cb = Rc::clone(&notify);

            glib::spawn_future_local(async move {
                let res = LastFmClient::get_session(&client, &api_key, &api_secret, &token).await;
                let Some(dialog) = weak_dialog.upgrade() else {
                    return;
                };
                let s = gio::Settings::new(SETTINGS);
                match res {
                    Ok(session) => {
                        let _ = s.set_string(LASTFM_SESSION_KEY, &session.key);
                        let _ = s.set_string(LASTFM_USERNAME, &session.username);
                        *dialog.imp().lastfm_pending_token.borrow_mut() = None;
                        dialog.refresh_lastfm_auth_row(&s);
                        let msg = if session.username.is_empty() {
                            gettext("Connected to Last.fm")
                        } else {
                            format!("{} {}", gettext("Connected to Last.fm as"), session.username)
                        };
                        notify_cb(msg);
                    }
                    Err(_) => {
                        dialog.refresh_lastfm_auth_row(&s);
                        notify_cb(gettext(
                            "Last.fm authorization is not approved yet. Approve in browser, then try Step 2 again.",
                        ));
                    }
                }
            });
        });
    }

    /// Re-lock the equalizer and pan controls. The locks are UI-only and never
    /// persisted, so they are reset every time the dialog is opened.
    fn reset_locks(&self) {
        self.imp().equalizer.lock();
        self.imp().pan.lock();
        self.imp().pitch.lock();
    }

    /// Apply the Spotify account's explicit content filter lock to the toggle.
    ///
    /// When the account has the filter locked (e.g. a family plan parental
    /// control), the switch is forced on, made insensitive so it cannot be
    /// changed, and given a tooltip explaining why. Otherwise the switch is
    /// interactive and reflects the local preference.
    fn set_explicit_filter_locked(&self, locked: bool) {
        let switch = &self.imp().skip_explicit_switch;
        if locked {
            switch.set_active(true);
            switch.set_sensitive(false);
            switch.set_tooltip_text(Some(&gettextrs::gettext(
                "Locked by a Family plan parental control and cannot be changed here.",
            )));
        } else {
            switch.set_sensitive(true);
            switch.set_tooltip_text(None);
        }
    }
}

pub struct Settings {
    parent: gtk::Window,
    settings_dialog: SettingsDialog,
    model: Rc<SettingsModel>,
}

impl Settings {
    pub fn new(parent: gtk::Window, model: SettingsModel) -> Self {
        let settings_dialog = SettingsDialog::new();
        let model = Rc::new(model);

        let close_model = model.clone();
        settings_dialog.connect_close(move || {
            let new_settings = RiffSettings::new_from_gsettings().unwrap_or_default();
            // Only stop the player for changes that require a full reload.
            // Equalizer changes are applied live and must not interrupt playback.
            if close_model
                .settings()
                .player_settings
                .requires_reload(&new_settings.player_settings)
            {
                close_model.stop_player();
            }
            close_model.set_settings();
        });

        let gsettings = gio::Settings::new(SETTINGS);
        let startup_values: Vec<_> = RESTART_REQUIRED_KEYS
            .iter()
            .map(|key| gsettings.value(key))
            .collect();
        let restart_model = model.clone();
        settings_dialog.connect_closed(move || {
            let changed = RESTART_REQUIRED_KEYS
                .iter()
                .zip(&startup_values)
                .any(|(key, startup)| gsettings.value(key) != *startup);
            if changed {
                restart_model.show_restart_required();
            }
        });

        let clear_model = model.clone();
        settings_dialog.connect_clear_cache(move || {
            clear_model.clear_cache();
        });

        let notify_model = model.clone();
        settings_dialog.connect_lastfm_auth(
            model.http_client(),
            Rc::new(move |msg| notify_model.notify(msg)),
        );

        Self {
            parent,
            settings_dialog,
            model,
        }
    }

    fn dialog(&self) -> &libadwaita::Dialog {
        self.settings_dialog.upcast_ref::<libadwaita::Dialog>()
    }

    pub fn show_self(&self) {
        // Locks are UI-only and must not be stateful between openings.
        self.settings_dialog.reset_locks();
        // Reflect the account's explicit content filter lock each time the
        // dialog opens, since it becomes known only after login.
        self.settings_dialog
            .set_explicit_filter_locked(self.model.explicit_filter_locked());
        self.dialog().present(Some(&self.parent));
    }

    pub fn show_integrations(&self) {
        self.settings_dialog.set_visible_page_name("integrations");
        self.show_self();
    }
}

impl EventListener for Settings {
    fn on_event(&mut self, _: &AppEvent) {}
}

fn open_external_url(widget: &impl IsA<gtk::Widget>, url: &str) {
    if open::that_detached(url).is_err() {
        let parent = widget
            .root()
            .and_then(|r| r.downcast::<gtk::Window>().ok());
        let launcher = gtk::UriLauncher::new(url);
        launcher.launch(parent.as_ref(), None::<&gio::Cancellable>, |_| {});
    }
}

fn make_guide_step_label(text: &str) -> gtk::Label {
    let lbl = gtk::Label::new(Some(text));
    lbl.set_halign(gtk::Align::Start);
    lbl.set_xalign(0.0);
    lbl.set_wrap(true);
    lbl.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    lbl
}

/// What to type in each field of Genius's "New API Client" form.
/// `None` means "keep what Genius pre-fills".
const GENIUS_FORM_FIELDS: [(&str, Option<&str>); 4] = [
    ("App Name", Some("Canto")),
    ("Icon URL", None),
    ("App Website URL", Some("http://localhost")),
    ("Redirect URI", Some("http://localhost")),
];

/// A row showing a form field and the value to type, with a copy button.
fn make_copy_row(
    field: &str,
    value: Option<&str>,
    toasts: &libadwaita::ToastOverlay,
) -> gtk::Widget {
    let row = libadwaita::ActionRow::new();
    row.set_title(field);
    match value {
        None => {
            // translators: Genius form field that should be left untouched.
            row.set_subtitle(&gettext("Keep the default"));
        }
        Some(value) => {
            row.set_subtitle(value);
            row.set_subtitle_selectable(true);
            let copy = gtk::Button::from_icon_name("edit-copy-symbolic");
            copy.set_valign(gtk::Align::Center);
            copy.add_css_class("flat");
            // translators: Tooltip of the button copying a value for a form field.
            copy.set_tooltip_text(Some(&gettext("Copy")));
            let value = value.to_string();
            let toasts = toasts.downgrade();
            copy.connect_clicked(move |button| {
                button.clipboard().set_text(&value);
                if let Some(toasts) = toasts.upgrade() {
                    // translators: Toast after copying a value; {} is the value.
                    let text = gettext("Copied {}").replace("{}", &value);
                    toasts.add_toast(libadwaita::Toast::new(&text));
                }
            });
            row.add_suffix(&copy);
            row.set_activatable_widget(Some(&copy));
        }
    }
    row.upcast()
}

/// The body of the Genius guide (kept apart from the dialog so tests can build it).
fn genius_guide_content(on_close: impl Fn() + 'static) -> libadwaita::ToastOverlay {
    let toasts = libadwaita::ToastOverlay::new();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(16);
    content.set_margin_bottom(20);
    content.set_margin_start(20);
    content.set_margin_end(20);

    content.append(&make_guide_step_label(&gettext(
        "1. Click the button below to open the New API Client form on Genius (sign in if asked).",
    )));
    content.append(&make_guide_step_label(&gettext(
        "2. Fill in the form like this, then click 'Save':",
    )));

    let fields = gtk::ListBox::new();
    fields.set_selection_mode(gtk::SelectionMode::None);
    fields.add_css_class("boxed-list");
    for (field, value) in GENIUS_FORM_FIELDS {
        fields.append(&make_copy_row(field, value, &toasts));
    }
    content.append(&fields);

    content.append(&make_guide_step_label(&gettext(
        "3. On the next page, click 'Generate Access Token' and copy the Client Access Token. Canto doesn't need the Client ID or Client Secret.",
    )));
    content.append(&make_guide_step_label(&gettext(
        "4. Back in Canto, paste it into Client Access Token, under Genius.",
    )));

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(8);

    let open_btn = gtk::Button::with_label(&gettext("Open Genius"));
    open_btn.add_css_class("pill");
    open_btn.add_css_class("suggested-action");
    open_btn.connect_clicked(|b| {
        open_external_url(b, "https://genius.com/api-clients/new");
    });
    buttons.append(&open_btn);

    let close_btn = gtk::Button::with_label(&gettext("Close"));
    close_btn.add_css_class("pill");
    close_btn.connect_clicked(move |_| on_close());
    buttons.append(&close_btn);

    content.append(&buttons);
    toasts.set_child(Some(&content));
    toasts
}

fn present_genius_guide_dialog(parent: &SettingsDialog) {
    let dialog = libadwaita::Dialog::builder()
        .title(gettext("How to Get a Genius Token"))
        .content_width(460)
        .build();

    let toolbar = libadwaita::ToolbarView::new();
    toolbar.add_top_bar(&libadwaita::HeaderBar::new());

    let weak_dialog = dialog.downgrade();
    let content = genius_guide_content(move || {
        if let Some(d) = weak_dialog.upgrade() {
            d.close();
        }
    });
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(parent));
}

#[cfg(test)]
pub(crate) fn genius_guide_for_tests() -> gtk::Widget {
    genius_guide_content(|| {}).upcast()
}

fn present_scrobbling_guide_dialog(parent: &SettingsDialog) {
    let dialog = libadwaita::Dialog::builder()
        .title(gettext("How to Set Up Scrobbling"))
        .content_width(480)
        .build();

    let toolbar = libadwaita::ToolbarView::new();
    toolbar.add_top_bar(&libadwaita::HeaderBar::new());

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(16);
    content.set_margin_bottom(20);
    content.set_margin_start(20);
    content.set_margin_end(20);

    let lfm_header = gtk::Label::new(Some(&gettext("Last.fm (API Key + Browser Auth)")));
    lfm_header.add_css_class("heading");
    lfm_header.set_halign(gtk::Align::Start);
    content.append(&lfm_header);

    content.append(&make_guide_step_label(&gettext(
        "1. Open last.fm/api/account/create, create an API application, and copy the API Key and Shared Secret into Canto.",
    )));
    content.append(&make_guide_step_label(&gettext(
        "2. Click '1. Open Auth' to open Last.fm in your browser and approve access for Canto.",
    )));
    content.append(&make_guide_step_label(&gettext(
        "3. Return to Canto and click '2. Complete' to store your persistent session key.",
    )));

    let lb_header = gtk::Label::new(Some(&gettext("ListenBrainz (User Token)")));
    lb_header.add_css_class("heading");
    lb_header.set_halign(gtk::Align::Start);
    lb_header.set_margin_top(6);
    content.append(&lb_header);

    content.append(&make_guide_step_label(&gettext(
        "Open listenbrainz.org/settings, copy your User Token, and paste it into ListenBrainz User Token.",
    )));

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(8);

    let lfm_btn = gtk::Button::with_label(&gettext("Open Last.fm API"));
    lfm_btn.add_css_class("pill");
    lfm_btn.connect_clicked(|b| {
        open_external_url(b, "https://www.last.fm/api/account/create");
    });
    buttons.append(&lfm_btn);

    let lb_btn = gtk::Button::with_label(&gettext("Open ListenBrainz"));
    lb_btn.add_css_class("pill");
    lb_btn.connect_clicked(|b| {
        open_external_url(b, "https://listenbrainz.org/settings/");
    });
    buttons.append(&lb_btn);

    let close_btn = gtk::Button::with_label(&gettext("Close"));
    close_btn.add_css_class("pill");
    let weak_dialog = dialog.downgrade();
    close_btn.connect_clicked(move |_| {
        if let Some(d) = weak_dialog.upgrade() {
            d.close();
        }
    });
    buttons.append(&close_btn);

    content.append(&buttons);
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(parent));
}
