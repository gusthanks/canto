//! Volume wheel/touchpad scroll direction handling and speaker icon helpers.

use gtk::prelude::*;

/// Parses the boolean field from `hyprctl -j getoption <name>`
/// (`"bool"` on Hyprland 0.56+, `"int"` on older releases).
pub(super) fn parse_hyprctl_bool_option(stdout: &str) -> Option<bool> {
    let value = serde_json::from_str::<serde_json::Value>(stdout).ok()?;
    if let Some(flag) = value.get("bool").and_then(|v| v.as_bool()) {
        return Some(flag);
    }
    value.get("int").and_then(|v| v.as_i64()).map(|n| n != 0)
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
pub(super) fn is_natural_scroll(
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
pub(super) fn scroll_steps(dy: f64, natural: bool) -> f64 {
    if natural {
        -dy
    } else {
        dy
    }
}

/// Resolves physical scroll steps from a GTK scroll controller's current event,
/// accounting for Wayland `relative_direction` and Hyprland's `natural_scroll`
/// fallback so volume always follows physical hand motion.
pub(crate) fn scroll_steps_from_event(controller: &gtk::EventControllerScroll, dy: f64) -> f64 {
    let event = controller
        .current_event()
        .and_then(|event| event.downcast::<gtk::gdk::ScrollEvent>().ok());
    let natural = event.as_ref().is_some_and(|event| {
        let hyprland =
            std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").map(|_| hyprland_natural_scroll());
        let natural = is_natural_scroll(event.relative_direction(), event.unit(), hyprland);
        log::debug!(
            "volume scroll: dy={} unit={:?} rel={:?} natural={}",
            dy,
            event.unit(),
            event.relative_direction(),
            natural
        );
        natural
    });
    scroll_steps(dy, natural)
}

/// The volume after `steps` of the wheel or the touchpad (negative: up),
/// each the step of Ctrl+Up and Ctrl+Down.
pub(crate) fn volume_after_scroll(value: f64, steps: f64) -> f64 {
    (value - steps * crate::VOLUME_STEP).clamp(0.0, 1.0)
}

/// Standard speaker icon for a volume level in `0.0..=1.0`.
pub(super) fn volume_icon_name(value: f64) -> &'static str {
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
