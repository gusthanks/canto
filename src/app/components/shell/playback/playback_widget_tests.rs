//! Unit tests for `PlaybackWidget` volume scrolling, icons, tooltips, and EQ presets.

use super::super::volume_scroll::{
    is_natural_scroll, parse_hyprctl_bool_option, scroll_steps, volume_icon_name,
};
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
            r#"{"option": "input:touchpad:natural_scroll", "bool": true, "set": true }"#
        ),
        Some(true)
    );
    assert_eq!(
        parse_hyprctl_bool_option(
            r#"{"option": "input:natural_scroll", "bool": false, "set": false }"#
        ),
        Some(false)
    );
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
    let mouse_natural = is_natural_scroll(Rel::Identical, ScrollUnit::Wheel, Some((true, false)));
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
