use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use canto_api::Load;
use canto_config::settings::{ADAPTIVE_COVER_ACCENT, ADAPTIVE_COVER_COLORS, APP_THEME, SETTINGS};
use gio::prelude::{FileExt, FileMonitorExt, SettingsExt};
use gtk::gdk;
use libadwaita as adw;

use crate::app::components::EventListener;
use crate::app::state::PlaybackEvent;
use crate::app::{AppEvent, AppModel};

const THEME_PROVIDER_PRIORITY: u32 = gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1;
const WCAG_AA_TEXT_CONTRAST: f64 = 4.5;
const WCAG_UI_COMPONENT_CONTRAST: f64 = 3.0;
const ADWAITA_DARK_VIEW_BG: &str = "#1e1e1e";
const ADWAITA_DARK_CARD_BG: &str = "#2b2b2b";
const ADWAITA_LIGHT_VIEW_BG: &str = "#ffffff";
const ADWAITA_LIGHT_CARD_BG: &str = "#ffffff";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKind {
    Omarchy,
    Neutral,
    SpotifyGreen,
    OledDark,
    WarmVinyl,
    PurpleNight,
}

impl ThemeKind {
    pub fn from_setting(value: &str) -> Self {
        match value.trim() {
            "omarchy" => Self::Omarchy,
            "neutral" => Self::Neutral,
            "spotify-green" => Self::SpotifyGreen,
            "oled-dark" => Self::OledDark,
            "warm-vinyl" => Self::WarmVinyl,
            "purple-night" => Self::PurpleNight,
            _ => Self::Omarchy,
        }
    }

    pub fn setting_key(self) -> &'static str {
        match self {
            Self::Omarchy => "omarchy",
            Self::Neutral => "neutral",
            Self::SpotifyGreen => "spotify-green",
            Self::OledDark => "oled-dark",
            Self::WarmVinyl => "warm-vinyl",
            Self::PurpleNight => "purple-night",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemThemeSource {
    Omarchy,
    Matugen,
    Pywal,
    Wallust,
}

impl SystemThemeSource {
    pub fn setting_key(self) -> &'static str {
        match self {
            Self::Omarchy => "omarchy",
            Self::Matugen => "matugen",
            Self::Pywal => "pywal",
            Self::Wallust => "wallust",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedSystemTheme {
    pub source: SystemThemeSource,
    pub palette: ThemePalette,
    /// True when the palette file carries both dark and light variants with no
    /// explicit mode lock, so `AdwStyleManager` should stay on `Default` and
    /// track OS dark/light switches live.
    pub follows_system_scheme: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemThemePaths {
    pub omarchy: PathBuf,
    pub matugen: Vec<PathBuf>,
    pub pywal: Vec<PathBuf>,
    pub wallust: Vec<PathBuf>,
}

impl SystemThemePaths {
    pub fn default_system() -> Self {
        let cache = xdg_cache_home();
        let config = xdg_config_home();
        Self {
            omarchy: omarchy_colors_path(),
            matugen: vec![
                cache.join("matugen").join("colors.json"),
                config.join("matugen").join("colors.json"),
            ],
            pywal: vec![cache.join("wal").join("colors.json")],
            wallust: vec![
                cache.join("wallust").join("colors.json"),
                config.join("wallust").join("colors.json"),
            ],
        }
    }

    fn optional_palette_files(&self) -> impl Iterator<Item = &Path> {
        self.matugen
            .iter()
            .chain(self.pywal.iter())
            .chain(self.wallust.iter())
            .map(PathBuf::as_path)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeSnapshot {
    pub kind: &'static str,
    pub is_dark: bool,
    pub background: String,
    pub surface: String,
    pub foreground: String,
    pub accent: String,
    pub highlight: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemePalette {
    pub is_dark: bool,
    pub window_bg: String,
    pub window_fg: String,
    pub view_bg: String,
    pub view_fg: String,
    pub headerbar_bg: String,
    pub sidebar_bg: String,
    pub card_bg: String,
    pub popover_bg: String,
    pub accent_bg: String,
    pub accent: String,
    pub accent_fg: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputedAccent {
    pub accent_bg: String,
    pub accent: String,
    pub accent_fg: String,
}

impl ThemePalette {
    fn fixed_preset(kind: ThemeKind) -> Option<Self> {
        match kind {
            ThemeKind::Omarchy | ThemeKind::Neutral => None,
            ThemeKind::SpotifyGreen => Some(Self::spotify_green()),
            ThemeKind::OledDark => Some(Self::oled_dark()),
            ThemeKind::WarmVinyl => Some(Self::warm_vinyl()),
            ThemeKind::PurpleNight => Some(Self::purple_night()),
        }
    }

    fn spotify_green() -> Self {
        Self {
            is_dark: true,
            window_bg: "#121212".into(),
            window_fg: "#f5f5f5".into(),
            view_bg: "#121212".into(),
            view_fg: "#f5f5f5".into(),
            headerbar_bg: "#101010".into(),
            sidebar_bg: "#090909".into(),
            card_bg: "#1b1b1b".into(),
            popover_bg: "#212121".into(),
            accent_bg: "#1db954".into(),
            accent: "#1ed760".into(),
            accent_fg: "#000000".into(),
        }
    }

    fn oled_dark() -> Self {
        Self {
            is_dark: true,
            window_bg: "#000000".into(),
            window_fg: "#f4f4f5".into(),
            view_bg: "#000000".into(),
            view_fg: "#f4f4f5".into(),
            headerbar_bg: "#050505".into(),
            sidebar_bg: "#040404".into(),
            card_bg: "#0f0f10".into(),
            popover_bg: "#141416".into(),
            accent_bg: "#1db954".into(),
            accent: "#1ed760".into(),
            accent_fg: "#000000".into(),
        }
    }

    fn warm_vinyl() -> Self {
        Self {
            is_dark: true,
            window_bg: "#16120f".into(),
            window_fg: "#ede2d4".into(),
            view_bg: "#181310".into(),
            view_fg: "#ede2d4".into(),
            headerbar_bg: "#120e0c".into(),
            sidebar_bg: "#0e0b09".into(),
            card_bg: "#211b17".into(),
            popover_bg: "#28211c".into(),
            accent_bg: "#d4884a".into(),
            accent: "#e39b5e".into(),
            accent_fg: "#110d0a".into(),
        }
    }

    fn purple_night() -> Self {
        Self {
            is_dark: true,
            window_bg: "#110f1a".into(),
            window_fg: "#ece8f8".into(),
            view_bg: "#13101d".into(),
            view_fg: "#ece8f8".into(),
            headerbar_bg: "#0d0b14".into(),
            sidebar_bg: "#09080f".into(),
            card_bg: "#1a1728".into(),
            popover_bg: "#211d32".into(),
            accent_bg: "#9f7aea".into(),
            accent: "#b794f4".into(),
            accent_fg: "#0d0917".into(),
        }
    }
}

fn xdg_state_home() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| glib::home_dir().join(".local").join("state"))
}

fn xdg_cache_home() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| glib::home_dir().join(".cache"))
}

fn xdg_config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| glib::home_dir().join(".config"))
}

pub fn omarchy_colors_path() -> PathBuf {
    xdg_state_home()
        .join("omarchy")
        .join("current")
        .join("theme")
        .join("colors.toml")
}

/// Resolves "Follow System" in priority order:
/// 1. Omarchy (`colors.toml`)
/// 2. Matugen (`colors.json`)
/// 3. Pywal (`~/.cache/wal/colors.json`)
/// 4. Wallust (`colors.json`)
/// 5. Native desktop fallback (`None`, handled by `AdwStyleManager`)
pub fn resolve_system_palette(
    paths: &SystemThemePaths,
    prefer_dark: bool,
) -> Option<ResolvedSystemTheme> {
    if let Ok(content) = std::fs::read_to_string(&paths.omarchy) {
        if let Some(palette) = parse_omarchy_colors_toml(&content) {
            return Some(ResolvedSystemTheme {
                source: SystemThemeSource::Omarchy,
                palette,
                follows_system_scheme: false,
            });
        }
    }

    for path in &paths.matugen {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Some((palette, follows_system_scheme)) =
                parse_matugen_colors_json(&content, prefer_dark)
            {
                return Some(ResolvedSystemTheme {
                    source: SystemThemeSource::Matugen,
                    palette,
                    follows_system_scheme,
                });
            }
        }
    }

    for path in &paths.pywal {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Some(palette) = parse_pywal_colors_json(&content) {
                return Some(ResolvedSystemTheme {
                    source: SystemThemeSource::Pywal,
                    palette,
                    follows_system_scheme: false,
                });
            }
        }
    }

    for path in &paths.wallust {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Some(palette) = parse_wallust_colors_json(&content) {
                return Some(ResolvedSystemTheme {
                    source: SystemThemeSource::Wallust,
                    palette,
                    follows_system_scheme: false,
                });
            }
        }
    }

    None
}

pub fn parse_omarchy_colors_toml(content: &str) -> Option<ThemePalette> {
    let mut map = HashMap::<String, String>::new();
    for raw_line in content.lines() {
        let line = strip_toml_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let val = trim_toml_quotes(val.trim());
        if !val.is_empty() {
            map.insert(key, val.to_string());
        }
    }

    let bg = valid_hex_color(map.get("background").map(String::as_str)?)?;
    let fg = valid_hex_color(map.get("foreground").map(String::as_str)?)?;
    let is_dark = !map
        .get("mode")
        .is_some_and(|m| m.eq_ignore_ascii_case("light"));

    let dark_bg = map
        .get("dark_background")
        .and_then(|s| valid_hex_color(s))
        .unwrap_or_else(|| bg.clone());
    let darker_bg = map
        .get("darker_background")
        .and_then(|s| valid_hex_color(s))
        .unwrap_or_else(|| dark_bg.clone());
    let lighter_bg = map
        .get("lighter_background")
        .and_then(|s| valid_hex_color(s))
        .unwrap_or_else(|| bg.clone());
    let accent_bg = map
        .get("accent")
        .and_then(|s| valid_hex_color(s))
        .or_else(|| map.get("green").and_then(|s| valid_hex_color(s)))
        .unwrap_or_else(|| "#1db954".to_string());

    let (headerbar_bg, sidebar_bg, card_bg, popover_bg) = if is_dark {
        (dark_bg.clone(), darker_bg, lighter_bg.clone(), lighter_bg)
    } else {
        (darker_bg, dark_bg, lighter_bg.clone(), lighter_bg)
    };

    let accent_fg = contrasting_fg_for_hex(&accent_bg);
    Some(ThemePalette {
        is_dark,
        window_bg: bg.clone(),
        window_fg: fg.clone(),
        view_bg: bg,
        view_fg: fg,
        headerbar_bg,
        sidebar_bg,
        card_bg,
        popover_bg,
        accent_bg: accent_bg.clone(),
        accent: accent_bg,
        accent_fg,
    })
}

fn json_hex_value(val: &serde_json::Value) -> Option<String> {
    val.as_str()
        .and_then(valid_hex_color)
        .or_else(|| {
            val.get("color")
                .and_then(serde_json::Value::as_str)
                .and_then(valid_hex_color)
        })
        .or_else(|| {
            val.get("hex")
                .and_then(serde_json::Value::as_str)
                .and_then(valid_hex_color)
        })
}

fn extract_flat_hex_map(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (key, value) in obj {
        if let Some(hex) = json_hex_value(value) {
            map.insert(key.to_ascii_lowercase(), hex);
        }
    }
    map
}

fn extract_matugen_tokens(
    root: &serde_json::Value,
    prefer_dark: bool,
) -> Option<(HashMap<String, String>, Option<bool>, bool)> {
    let root_obj = root.as_object()?;
    let explicit_mode = root_obj
        .get("mode")
        .and_then(serde_json::Value::as_str)
        .and_then(|m| match m.trim().to_ascii_lowercase().as_str() {
            "dark" => Some(true),
            "light" => Some(false),
            _ => None,
        });

    let colors_obj = root_obj
        .get("colors")
        .and_then(serde_json::Value::as_object)
        .unwrap_or(root_obj);

    // Format 1: mode-first (`colors.dark` / `colors.light`).
    let dark_section = colors_obj
        .get("dark")
        .and_then(serde_json::Value::as_object);
    let light_section = colors_obj
        .get("light")
        .and_then(serde_json::Value::as_object);
    if dark_section.is_some() || light_section.is_some() {
        let has_both = dark_section.is_some() && light_section.is_some();
        let want_dark = explicit_mode.unwrap_or(prefer_dark);
        let (picked, picked_dark) = match (want_dark, dark_section, light_section) {
            (true, Some(d), _) => (d, true),
            (false, _, Some(l)) => (l, false),
            (_, Some(d), None) => (d, true),
            (_, None, Some(l)) => (l, false),
            (_, None, None) => return None,
        };
        let map = extract_flat_hex_map(picked);
        let follows_system_scheme = explicit_mode.is_none() && has_both;
        return Some((map, Some(picked_dark), follows_system_scheme));
    }

    // Format 2: color-first (`colors.primary.dark` / `colors.primary.light` / `colors.primary.default`).
    let has_nested_modes = colors_obj.values().any(|v| {
        v.as_object().is_some_and(|o| {
            o.contains_key("dark") || o.contains_key("light") || o.contains_key("default")
        })
    });
    if has_nested_modes {
        let has_dark = colors_obj
            .values()
            .any(|v| v.get("dark").and_then(json_hex_value).is_some());
        let has_light = colors_obj
            .values()
            .any(|v| v.get("light").and_then(json_hex_value).is_some());
        let has_both = has_dark && has_light;
        let want_dark = explicit_mode.unwrap_or(prefer_dark);
        let order: [&str; 3] = if want_dark {
            ["dark", "default", "light"]
        } else {
            ["light", "default", "dark"]
        };
        let mut map = HashMap::new();
        for (key, value) in colors_obj {
            if let Some(entry) = value.as_object() {
                if let Some(hex) = order
                    .iter()
                    .find_map(|mode_key| entry.get(*mode_key).and_then(json_hex_value))
                {
                    map.insert(key.to_ascii_lowercase(), hex);
                }
            }
        }
        let chosen_mode = explicit_mode.or_else(|| {
            if has_both {
                Some(want_dark)
            } else if has_dark {
                Some(true)
            } else if has_light {
                Some(false)
            } else {
                None
            }
        });
        let follows_system_scheme = explicit_mode.is_none() && has_both;
        return Some((map, chosen_mode, follows_system_scheme));
    }

    // Format 3: flat token object (either under `colors` or at top level).
    let map = extract_flat_hex_map(colors_obj);
    Some((map, explicit_mode, false))
}

/// Parses Matugen `colors.json` (mode-first, color-first, or flat JSON).
/// Returns `(palette, follows_system_scheme)`.
pub fn parse_matugen_colors_json(content: &str, prefer_dark: bool) -> Option<(ThemePalette, bool)> {
    let root: serde_json::Value = serde_json::from_str(content).ok()?;
    let (map, mode_hint, follows_system_scheme) = extract_matugen_tokens(&root, prefer_dark)?;

    let bg = map
        .get("background")
        .or_else(|| map.get("surface"))
        .cloned()?;
    let fg = map
        .get("on_background")
        .or_else(|| map.get("on_surface"))
        .or_else(|| map.get("foreground"))
        .cloned()?;
    let accent_raw = map.get("primary").or_else(|| map.get("accent")).cloned()?;

    let is_dark = mode_hint.unwrap_or_else(|| is_dark_hex(&bg));
    let (default_header, default_sidebar, default_card, default_popover) =
        derive_surfaces(&bg, is_dark);

    let headerbar_bg = map
        .get("surface_container_low")
        .or_else(|| map.get("surface_dim"))
        .cloned()
        .unwrap_or(default_header);
    let sidebar_bg = map
        .get("surface_container_lowest")
        .or_else(|| map.get("surface_container_low"))
        .cloned()
        .unwrap_or(default_sidebar);
    let card_bg = map
        .get("surface_container")
        .or_else(|| map.get("surface_container_high"))
        .cloned()
        .unwrap_or(default_card);
    let popover_bg = map
        .get("surface_container_high")
        .or_else(|| map.get("surface_container_highest"))
        .cloned()
        .unwrap_or(default_popover);

    let computed = compute_accessible_cover_accent(&accent_raw, is_dark, &bg, &card_bg)?;
    let accent_fg = map
        .get("on_primary")
        .filter(|on_primary| {
            wcag_contrast_ratio_hex(on_primary, &computed.accent_bg)
                .is_some_and(|r| r >= WCAG_AA_TEXT_CONTRAST)
        })
        .cloned()
        .unwrap_or(computed.accent_fg);

    Some((
        ThemePalette {
            is_dark,
            window_bg: bg.clone(),
            window_fg: fg.clone(),
            view_bg: bg,
            view_fg: fg,
            headerbar_bg,
            sidebar_bg,
            card_bg,
            popover_bg,
            accent_bg: computed.accent_bg,
            accent: computed.accent,
            accent_fg,
        },
        follows_system_scheme,
    ))
}

/// Parses pywal `~/.cache/wal/colors.json`.
pub fn parse_pywal_colors_json(content: &str) -> Option<ThemePalette> {
    parse_wal_style_colors_json(content, true)
}

/// Parses wallust `colors.json` (both pywal-compatible nested format and flat JSON).
pub fn parse_wallust_colors_json(content: &str) -> Option<ThemePalette> {
    parse_wal_style_colors_json(content, false)
}

fn parse_wal_style_colors_json(
    content: &str,
    require_nested_structure: bool,
) -> Option<ThemePalette> {
    let root: serde_json::Value = serde_json::from_str(content).ok()?;
    let root_obj = root.as_object()?;
    let special = root_obj
        .get("special")
        .and_then(serde_json::Value::as_object);
    let colors = root_obj
        .get("colors")
        .and_then(serde_json::Value::as_object);

    if require_nested_structure && special.is_none() && colors.is_none() {
        return None;
    }

    let lookup = |key: &str| -> Option<String> {
        special
            .and_then(|m| m.get(key))
            .and_then(json_hex_value)
            .or_else(|| colors.and_then(|m| m.get(key)).and_then(json_hex_value))
            .or_else(|| root_obj.get(key).and_then(json_hex_value))
    };

    let bg = lookup("background").or_else(|| lookup("color0"))?;
    let fg = lookup("foreground")
        .or_else(|| lookup("color15"))
        .or_else(|| lookup("color7"))?;

    let candidates = [
        "accent", "color4", "color2", "color1", "color6", "color5", "color3", "color12",
    ];
    let accent_raw = candidates
        .iter()
        .filter_map(|k| lookup(k))
        .find(|hex| wcag_contrast_ratio_hex(hex, &bg).is_some_and(|r| r >= 1.5))
        .or_else(|| candidates.iter().find_map(|k| lookup(k)))?;

    let is_dark = is_dark_hex(&bg);
    let (headerbar_bg, sidebar_bg, card_bg, popover_bg) = derive_surfaces(&bg, is_dark);
    let computed = compute_accessible_cover_accent(&accent_raw, is_dark, &bg, &card_bg)?;

    Some(ThemePalette {
        is_dark,
        window_bg: bg.clone(),
        window_fg: fg.clone(),
        view_bg: bg,
        view_fg: fg,
        headerbar_bg,
        sidebar_bg,
        card_bg,
        popover_bg,
        accent_bg: computed.accent_bg,
        accent: computed.accent,
        accent_fg: computed.accent_fg,
    })
}

fn derive_surfaces(bg: &str, is_dark: bool) -> (String, String, String, String) {
    if is_dark {
        let headerbar = blend_hex(bg, "#000000", 0.14).unwrap_or_else(|| bg.to_string());
        let sidebar = blend_hex(bg, "#000000", 0.25).unwrap_or_else(|| bg.to_string());
        let card = blend_hex(bg, "#ffffff", 0.06).unwrap_or_else(|| bg.to_string());
        let popover = blend_hex(bg, "#ffffff", 0.10).unwrap_or_else(|| bg.to_string());
        (headerbar, sidebar, card, popover)
    } else {
        let headerbar = blend_hex(bg, "#000000", 0.04).unwrap_or_else(|| bg.to_string());
        let sidebar = blend_hex(bg, "#000000", 0.07).unwrap_or_else(|| bg.to_string());
        let card = blend_hex(bg, "#ffffff", 0.50).unwrap_or_else(|| bg.to_string());
        let popover = blend_hex(bg, "#ffffff", 0.70).unwrap_or_else(|| bg.to_string());
        (headerbar, sidebar, card, popover)
    }
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if in_double && !escaped && ch == '\\' {
            escaped = true;
            continue;
        }
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => return &line[..idx],
            _ => {}
        }
    }
    line
}

fn trim_toml_quotes(raw: &str) -> &str {
    if raw.len() >= 2
        && ((raw.starts_with('"') && raw.ends_with('"'))
            || (raw.starts_with('\'') && raw.ends_with('\'')))
    {
        &raw[1..raw.len() - 1]
    } else {
        raw
    }
}

pub fn valid_hex_color(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if !(trimmed.len() == 7 || trimmed.len() == 9) || !trimmed.starts_with('#') {
        return None;
    }
    if trimmed[1..].chars().all(|c| c.is_ascii_hexdigit()) {
        Some(trimmed.to_ascii_lowercase())
    } else {
        None
    }
}

fn parse_rgb_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let clean = valid_hex_color(hex)?;
    let r = u8::from_str_radix(&clean[1..3], 16).ok()?;
    let g = u8::from_str_radix(&clean[3..5], 16).ok()?;
    let b = u8::from_str_radix(&clean[5..7], 16).ok()?;
    Some((r, g, b))
}

fn rgb_to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn rgba_to_hex(rgba: gdk::RGBA) -> String {
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    rgb_to_hex(to_u8(rgba.red()), to_u8(rgba.green()), to_u8(rgba.blue()))
}

fn blend_rgb(from: (u8, u8, u8), to: (u8, u8, u8), ratio: f64) -> (u8, u8, u8) {
    let t = ratio.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| {
        (f64::from(a) + (f64::from(b) - f64::from(a)) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    (mix(from.0, to.0), mix(from.1, to.1), mix(from.2, to.2))
}

fn blend_hex(from: &str, to: &str, ratio: f64) -> Option<String> {
    let a = parse_rgb_hex(from)?;
    let b = parse_rgb_hex(to)?;
    let (r, g, bl) = blend_rgb(a, b, ratio);
    Some(rgb_to_hex(r, g, bl))
}

fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    let channel = |c: u8| {
        let s = f64::from(c) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn wcag_contrast_ratio(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let la = relative_luminance(a.0, a.1, a.2);
    let lb = relative_luminance(b.0, b.1, b.2);
    let (lighter, darker) = if la >= lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}

pub fn wcag_contrast_ratio_hex(a: &str, b: &str) -> Option<f64> {
    Some(wcag_contrast_ratio(parse_rgb_hex(a)?, parse_rgb_hex(b)?))
}

fn is_dark_hex(hex: &str) -> bool {
    parse_rgb_hex(hex).is_none_or(|(r, g, b)| relative_luminance(r, g, b) < 0.20)
}

/// Picks a crisp foreground (`#111111`, `#ffffff`, or `#000000`) guaranteed to
/// achieve at least WCAG AA 4.5:1 contrast against any 24-bit RGB background.
pub fn contrasting_fg_for_hex(hex: &str) -> String {
    let Some(rgb) = parse_rgb_hex(hex) else {
        return "#ffffff".to_string();
    };
    let c_near_black = wcag_contrast_ratio(rgb, (17, 17, 17));
    let c_white = wcag_contrast_ratio(rgb, (255, 255, 255));
    if c_near_black >= c_white && c_near_black >= WCAG_AA_TEXT_CONTRAST {
        "#111111".to_string()
    } else if c_white >= WCAG_AA_TEXT_CONTRAST {
        "#ffffff".to_string()
    } else {
        // Crossover luminance band where `#111111` dips just below 4.5:1 while
        // pure `#000000` stays >= 4.66:1.
        "#000000".to_string()
    }
}

fn min_contrast(color: (u8, u8, u8), surfaces: &[(u8, u8, u8)]) -> f64 {
    surfaces
        .iter()
        .map(|&s| wcag_contrast_ratio(color, s))
        .fold(f64::INFINITY, f64::min)
}

fn ensure_contrast_against(
    color: (u8, u8, u8),
    surfaces: &[(u8, u8, u8)],
    min_ratio: f64,
    prefer_light_target: bool,
) -> (u8, u8, u8) {
    if min_contrast(color, surfaces) >= min_ratio {
        return color;
    }

    let preferred = if prefer_light_target {
        (255, 255, 255)
    } else {
        (0, 0, 0)
    };
    let opposite = if prefer_light_target {
        (0, 0, 0)
    } else {
        (255, 255, 255)
    };
    let target = if min_contrast(preferred, surfaces) >= min_contrast(opposite, surfaces) {
        preferred
    } else {
        opposite
    };

    let mut lo = 0.0;
    let mut hi = 1.0;
    let mut best = target;
    for _ in 0..14 {
        let mid = (lo + hi) * 0.5;
        let cand = blend_rgb(color, target, mid);
        if min_contrast(cand, surfaces) >= min_ratio {
            best = cand;
            hi = mid;
        } else {
            lo = mid;
        }
    }
    best
}

/// Computes contrast-safe UI accent tokens (`accent_bg`, `accent`, `accent_fg`)
/// from a raw cover/palette hex color for the active dark or light surfaces:
/// - `accent_bg` meets >= 3.0:1 (WCAG 1.4.11 non-text UI contrast) vs surfaces
/// - `accent` meets >= 4.5:1 (WCAG AA text contrast) vs surfaces
/// - `accent_fg` meets >= 4.5:1 (WCAG AA text contrast) vs `accent_bg`
pub fn compute_accessible_cover_accent(
    cover_hex: &str,
    is_dark: bool,
    view_bg: &str,
    card_bg: &str,
) -> Option<ComputedAccent> {
    let raw = parse_rgb_hex(cover_hex)?;
    let default_bg = if is_dark {
        (18, 18, 18)
    } else {
        (250, 250, 250)
    };
    let vbg = parse_rgb_hex(view_bg).unwrap_or(default_bg);
    let cbg = parse_rgb_hex(card_bg).unwrap_or(vbg);
    let surfaces = [vbg, cbg];

    let accent_bg_rgb =
        ensure_contrast_against(raw, &surfaces, WCAG_UI_COMPONENT_CONTRAST, is_dark);
    let accent_rgb = ensure_contrast_against(raw, &surfaces, WCAG_AA_TEXT_CONTRAST, is_dark);

    let accent_bg = rgb_to_hex(accent_bg_rgb.0, accent_bg_rgb.1, accent_bg_rgb.2);
    let accent = rgb_to_hex(accent_rgb.0, accent_rgb.1, accent_rgb.2);
    let accent_fg = contrasting_fg_for_hex(&accent_bg);

    Some(ComputedAccent {
        accent_bg,
        accent,
        accent_fg,
    })
}

pub fn extract_cover_accent_from_rgba(rgba: &[u8]) -> Option<String> {
    let mut weighted_r = 0.0f64;
    let mut weighted_g = 0.0f64;
    let mut weighted_b = 0.0f64;
    let mut weight_sum = 0.0f64;

    for pixel in rgba.as_chunks::<4>().0 {
        if pixel[3] < 64 {
            continue;
        }
        let r = f64::from(pixel[0]);
        let g = f64::from(pixel[1]);
        let b = f64::from(pixel[2]);
        let max_c = r.max(g).max(b);
        let min_c = r.min(g).min(b);
        let chroma = max_c - min_c;
        // Favor vivid tones while keeping monochrome art balanced.
        let weight = 0.15 + (chroma / 255.0) * 3.0;
        weighted_r += r * weight;
        weighted_g += g * weight;
        weighted_b += b * weight;
        weight_sum += weight;
    }

    if weight_sum <= 0.0 {
        return None;
    }

    let avg_r = (weighted_r / weight_sum).clamp(42.0, 224.0) as u8;
    let avg_g = (weighted_g / weight_sum).clamp(42.0, 224.0) as u8;
    let avg_b = (weighted_b / weight_sum).clamp(42.0, 224.0) as u8;
    Some(format!("#{avg_r:02x}{avg_g:02x}{avg_b:02x}"))
}

pub fn extract_cover_accent_from_texture(texture: &gdk::Texture) -> Option<String> {
    let mut downloader = gdk::TextureDownloader::new(texture);
    downloader.set_format(gdk::MemoryFormat::R8g8b8a8);
    let (bytes, _stride) = downloader.download_bytes();
    extract_cover_accent_from_rgba(bytes.as_ref())
}

pub fn build_theme_css(palette: Option<&ThemePalette>, cover_accent: Option<&str>) -> String {
    let is_dark = palette.is_none_or(|p| p.is_dark);
    build_theme_css_with_options(palette, cover_accent, false, is_dark, None)
}

pub fn build_theme_css_with_options(
    palette: Option<&ThemePalette>,
    cover_accent: Option<&str>,
    apply_cover_accent_to_ui: bool,
    is_dark: bool,
    system_accent: Option<&str>,
) -> String {
    let mut css = String::new();
    let valid_cover = cover_accent.and_then(valid_hex_color);
    let cover_color = valid_cover
        .clone()
        .or_else(|| palette.map(|p| p.accent.clone()))
        .or_else(|| system_accent.and_then(valid_hex_color))
        .unwrap_or_else(|| "#1ed760".to_string());

    let ui_cover_accent = if apply_cover_accent_to_ui {
        valid_cover.as_deref().and_then(|hex| match palette {
            Some(p) => compute_accessible_cover_accent(hex, p.is_dark, &p.view_bg, &p.card_bg),
            None => {
                let (vbg, cbg) = if is_dark {
                    (ADWAITA_DARK_VIEW_BG, ADWAITA_DARK_CARD_BG)
                } else {
                    (ADWAITA_LIGHT_VIEW_BG, ADWAITA_LIGHT_CARD_BG)
                };
                compute_accessible_cover_accent(hex, is_dark, vbg, cbg)
            }
        })
    } else {
        None
    };

    if let Some(p) = palette {
        let (accent_bg, accent, accent_fg) = match &ui_cover_accent {
            Some(c) => (
                c.accent_bg.as_str(),
                c.accent.as_str(),
                c.accent_fg.as_str(),
            ),
            None => (
                p.accent_bg.as_str(),
                p.accent.as_str(),
                p.accent_fg.as_str(),
            ),
        };

        css.push_str(&format!(
            "@define-color window_bg_color {window_bg};
@define-color window_fg_color {window_fg};
@define-color view_bg_color {view_bg};
@define-color view_fg_color {view_fg};
@define-color headerbar_bg_color {headerbar_bg};
@define-color headerbar_fg_color {window_fg};
@define-color sidebar_bg_color {sidebar_bg};
@define-color sidebar_fg_color {window_fg};
@define-color secondary_sidebar_bg_color {sidebar_bg};
@define-color secondary_sidebar_fg_color {window_fg};
@define-color card_bg_color {card_bg};
@define-color card_fg_color {window_fg};
@define-color popover_bg_color {popover_bg};
@define-color popover_fg_color {window_fg};
@define-color dialog_bg_color {popover_bg};
@define-color dialog_fg_color {window_fg};
@define-color accent_bg_color {accent_bg};
@define-color accent_color {accent};
@define-color accent_fg_color {accent_fg};

:root {{
  --window-bg-color: {window_bg};
  --window-fg-color: {window_fg};
  --view-bg-color: {view_bg};
  --view-fg-color: {view_fg};
  --headerbar-bg-color: {headerbar_bg};
  --headerbar-fg-color: {window_fg};
  --sidebar-bg-color: {sidebar_bg};
  --sidebar-fg-color: {window_fg};
  --secondary-sidebar-bg-color: {sidebar_bg};
  --secondary-sidebar-fg-color: {window_fg};
  --card-bg-color: {card_bg};
  --card-fg-color: {window_fg};
  --popover-bg-color: {popover_bg};
  --popover-fg-color: {window_fg};
  --dialog-bg-color: {popover_bg};
  --dialog-fg-color: {window_fg};
  --accent-bg-color: {accent_bg};
  --accent-color: {accent};
  --accent-fg-color: {accent_fg};
}}
",
            window_bg = p.window_bg,
            window_fg = p.window_fg,
            view_bg = p.view_bg,
            view_fg = p.view_fg,
            headerbar_bg = p.headerbar_bg,
            sidebar_bg = p.sidebar_bg,
            card_bg = p.card_bg,
            popover_bg = p.popover_bg,
            accent_bg = accent_bg,
            accent = accent,
            accent_fg = accent_fg,
        ));
    } else if let Some(c) = &ui_cover_accent {
        // Native libadwaita theme: override only accent tokens so system
        // light/dark surfaces remain completely untouched.
        css.push_str(&format!(
            "@define-color accent_bg_color {accent_bg};
@define-color accent_color {accent};
@define-color accent_fg_color {accent_fg};

:root {{
  --accent-bg-color: {accent_bg};
  --accent-color: {accent};
  --accent-fg-color: {accent_fg};
}}
",
            accent_bg = c.accent_bg,
            accent = c.accent,
            accent_fg = c.accent_fg,
        ));
    }

    css.push_str(&format!(
        "@define-color canto_cover_accent {cover_color};\n"
    ));
    css
}

struct ApplyGuard<'a>(&'a Cell<bool>);

impl Drop for ApplyGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

fn is_reload_monitor_event(event: gio::FileMonitorEvent) -> bool {
    matches!(
        event,
        gio::FileMonitorEvent::Changed
            | gio::FileMonitorEvent::ChangesDoneHint
            | gio::FileMonitorEvent::Created
            | gio::FileMonitorEvent::MovedIn
            | gio::FileMonitorEvent::Renamed
            | gio::FileMonitorEvent::Deleted
    )
}

pub struct ThemeEngine {
    provider: gtk::CssProvider,
    settings: gio::Settings,
    paths: SystemThemePaths,
    cover_accent: RefCell<Option<String>>,
    snapshot: RefCell<ThemeSnapshot>,
    monitors: RefCell<Vec<gio::FileMonitor>>,
    observers: RefCell<Vec<Rc<dyn Fn()>>>,
    applying: Cell<bool>,
    pending_reload: Cell<bool>,
}

impl Drop for ThemeEngine {
    fn drop(&mut self) {
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_remove_provider_for_display(&display, &self.provider);
        }
    }
}

impl ThemeEngine {
    pub fn install() -> Rc<Self> {
        Self::install_with_paths(SystemThemePaths::default_system())
    }

    pub fn install_with_paths(paths: SystemThemePaths) -> Rc<Self> {
        let provider = gtk::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                THEME_PROVIDER_PRIORITY,
            );
        }

        let settings = gio::Settings::new(SETTINGS);
        let initial_snapshot = ThemeSnapshot {
            kind: "system",
            is_dark: true,
            background: ADWAITA_DARK_VIEW_BG.into(),
            surface: ADWAITA_DARK_CARD_BG.into(),
            foreground: "#ffffff".into(),
            accent: "#3584e4".into(),
            highlight: "#3584e4".into(),
        };
        let engine = Rc::new(Self {
            provider,
            settings,
            paths,
            cover_accent: RefCell::new(None),
            snapshot: RefCell::new(initial_snapshot),
            monitors: RefCell::new(Vec::new()),
            observers: RefCell::new(Vec::new()),
            applying: Cell::new(false),
            pending_reload: Cell::new(false),
        });

        engine.apply();
        engine.setup_watchers();
        engine
    }

    fn setup_watchers(self: &Rc<Self>) {
        self.settings.connect_changed(
            Some(APP_THEME),
            glib::clone!(
                #[weak(rename_to = engine)]
                self,
                move |_, _| {
                    engine.sync_system_monitors();
                    engine.apply();
                }
            ),
        );

        self.settings.connect_changed(
            Some("theme-preference"),
            glib::clone!(
                #[weak(rename_to = engine)]
                self,
                move |_, _| {
                    engine.apply();
                }
            ),
        );

        self.settings.connect_changed(
            Some(ADAPTIVE_COVER_COLORS),
            glib::clone!(
                #[weak(rename_to = engine)]
                self,
                move |_, _| {
                    engine.apply();
                }
            ),
        );

        self.settings.connect_changed(
            Some(ADAPTIVE_COVER_ACCENT),
            glib::clone!(
                #[weak(rename_to = engine)]
                self,
                move |_, _| {
                    engine.apply();
                }
            ),
        );

        let style_manager = adw::StyleManager::default();
        style_manager.connect_dark_notify(glib::clone!(
            #[weak(rename_to = engine)]
            self,
            move |_| {
                if !engine.applying.get() {
                    engine.apply();
                }
            }
        ));
        style_manager.connect_accent_color_rgba_notify(glib::clone!(
            #[weak(rename_to = engine)]
            self,
            move |_| {
                if !engine.applying.get() {
                    engine.apply();
                }
            }
        ));

        self.sync_system_monitors();
    }

    fn sync_system_monitors(self: &Rc<Self>) {
        self.refresh_system_monitors(false);
    }

    fn schedule_reload(self: &Rc<Self>, reattach_monitors: bool) {
        if self.pending_reload.replace(true) && !reattach_monitors {
            return;
        }
        glib::idle_add_local_once(glib::clone!(
            #[weak(rename_to = engine)]
            self,
            move || {
                engine.pending_reload.set(false);
                if reattach_monitors {
                    engine.refresh_system_monitors(true);
                }
                engine.apply();
            }
        ));
    }

    fn refresh_system_monitors(self: &Rc<Self>, force: bool) {
        let kind = ThemeKind::from_setting(&self.settings.string(APP_THEME));
        if kind != ThemeKind::Omarchy {
            self.monitors.borrow_mut().clear();
            return;
        }
        if !force && !self.monitors.borrow().is_empty() {
            return;
        }
        self.monitors.borrow_mut().clear();

        // 1. Omarchy theme directory (`colors.toml`) and parent `current/` directory.
        let path = &self.paths.omarchy;
        let theme_dir = path.parent().unwrap_or(path.as_path());
        if theme_dir.is_dir() {
            if let Ok(mon) = gio::File::for_path(theme_dir)
                .monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
            {
                mon.connect_changed(glib::clone!(
                    #[weak(rename_to = engine)]
                    self,
                    move |_, changed_file, other_file, event_type| {
                        let matches_colors = |f: &gio::File| {
                            f.basename()
                                .and_then(|n| n.to_str().map(|s| s == "colors.toml"))
                                .unwrap_or(false)
                        };
                        let is_colors =
                            matches_colors(changed_file) || other_file.is_some_and(matches_colors);
                        if is_colors && is_reload_monitor_event(event_type) {
                            engine.schedule_reload(false);
                        }
                    }
                ));
                self.monitors.borrow_mut().push(mon);
            }
        }

        // Also watch the parent `~/.local/state/omarchy/current/` directory because
        // `omarchy-theme-set` can replace `theme/` or update `theme.generation` / `theme.name`.
        if let Some(current_dir) = theme_dir.parent().filter(|d| d.is_dir()) {
            if let Ok(parent_mon) = gio::File::for_path(current_dir)
                .monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
            {
                parent_mon.connect_changed(glib::clone!(
                    #[weak(rename_to = engine)]
                    self,
                    move |_, changed_file, other_file, event_type| {
                        let file_name = |f: &gio::File| {
                            f.basename().and_then(|n| n.to_str().map(str::to_string))
                        };
                        let name = file_name(changed_file);
                        let other_name = other_file.and_then(file_name);
                        let is_relevant = |n: Option<&str>| {
                            matches!(n, Some("theme" | "theme.generation" | "theme.name"))
                        };
                        if (is_relevant(name.as_deref()) || is_relevant(other_name.as_deref()))
                            && is_reload_monitor_event(event_type)
                        {
                            let reattach = name.as_deref() == Some("theme")
                                || other_name.as_deref() == Some("theme");
                            engine.schedule_reload(reattach);
                        }
                    }
                ));
                self.monitors.borrow_mut().push(parent_mon);
            }
        }

        // 2. Matugen, pywal, and wallust directories (only watched when present).
        let mut watched_dirs: HashMap<PathBuf, HashSet<String>> = HashMap::new();
        for file_path in self.paths.optional_palette_files() {
            let Some(dir) = file_path.parent().filter(|d| d.is_dir()) else {
                continue;
            };
            let Some(file_name) = file_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            watched_dirs
                .entry(dir.to_path_buf())
                .or_default()
                .insert(file_name.to_string());
        }

        for (dir, target_names) in watched_dirs {
            if let Ok(mon) = gio::File::for_path(&dir)
                .monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
            {
                mon.connect_changed(glib::clone!(
                    #[weak(rename_to = engine)]
                    self,
                    move |_, changed_file, other_file, event_type| {
                        let matches = |f: &gio::File| {
                            f.basename()
                                .and_then(|n| n.to_str().map(|s| target_names.contains(s)))
                                .unwrap_or(false)
                        };
                        let matches_target =
                            matches(changed_file) || other_file.is_some_and(matches);
                        if matches_target && is_reload_monitor_event(event_type) {
                            engine.schedule_reload(false);
                        }
                    }
                ));
                self.monitors.borrow_mut().push(mon);
            }
        }
    }

    pub fn set_cover_accent(&self, accent_hex: Option<String>) {
        let clean = accent_hex.and_then(|h| valid_hex_color(&h));
        if *self.cover_accent.borrow() == clean {
            return;
        }
        *self.cover_accent.borrow_mut() = clean;
        if self.settings.boolean(ADAPTIVE_COVER_COLORS) {
            self.apply();
        }
    }

    pub fn adaptive_enabled(&self) -> bool {
        self.settings.boolean(ADAPTIVE_COVER_COLORS)
    }

    pub fn apply(&self) {
        if self.applying.get() {
            return;
        }
        self.applying.set(true);
        let _guard = ApplyGuard(&self.applying);

        let kind = ThemeKind::from_setting(&self.settings.string(APP_THEME));
        let style_manager = adw::StyleManager::default();

        let (resolved_kind, palette) = match kind {
            ThemeKind::Omarchy => {
                let initial_dark = style_manager.is_dark();
                let mut resolved = resolve_system_palette(&self.paths, initial_dark);
                let needs_default_scheme =
                    resolved.as_ref().is_none_or(|r| r.follows_system_scheme);

                if needs_default_scheme {
                    if style_manager.color_scheme() != adw::ColorScheme::Default {
                        style_manager.set_color_scheme(adw::ColorScheme::Default);
                        let system_dark = style_manager.is_dark();
                        if system_dark != initial_dark && resolved.is_some() {
                            resolved = resolve_system_palette(&self.paths, system_dark);
                        }
                    }
                } else if let Some(r) = &resolved {
                    let desired = if r.palette.is_dark {
                        adw::ColorScheme::ForceDark
                    } else {
                        adw::ColorScheme::ForceLight
                    };
                    if style_manager.color_scheme() != desired {
                        style_manager.set_color_scheme(desired);
                    }
                }

                match resolved {
                    Some(r) => (r.source.setting_key(), Some(r.palette)),
                    None => ("system", None),
                }
            }
            ThemeKind::Neutral => {
                let pref = match self.settings.string("theme-preference").as_str() {
                    "light" => adw::ColorScheme::ForceLight,
                    "dark" => adw::ColorScheme::ForceDark,
                    _ => adw::ColorScheme::Default,
                };
                if style_manager.color_scheme() != pref {
                    style_manager.set_color_scheme(pref);
                }
                (kind.setting_key(), None)
            }
            fixed => {
                let palette = ThemePalette::fixed_preset(fixed);
                if let Some(p) = &palette {
                    let desired = if p.is_dark {
                        adw::ColorScheme::ForceDark
                    } else {
                        adw::ColorScheme::ForceLight
                    };
                    if style_manager.color_scheme() != desired {
                        style_manager.set_color_scheme(desired);
                    }
                }
                (kind.setting_key(), palette)
            }
        };

        let is_dark = palette
            .as_ref()
            .map_or_else(|| style_manager.is_dark(), |p| p.is_dark);
        let system_accent = rgba_to_hex(style_manager.accent_color_rgba());

        let adaptive_enabled = self.settings.boolean(ADAPTIVE_COVER_COLORS);
        let apply_cover_accent = adaptive_enabled && self.settings.boolean(ADAPTIVE_COVER_ACCENT);
        let cover_accent = if adaptive_enabled {
            self.cover_accent.borrow().clone()
        } else {
            None
        };

        let css = build_theme_css_with_options(
            palette.as_ref(),
            cover_accent.as_deref(),
            apply_cover_accent,
            is_dark,
            Some(&system_accent),
        );
        self.provider.load_from_string(&css);

        let computed_cover = if apply_cover_accent {
            cover_accent.as_deref().and_then(|hex| match &palette {
                Some(p) => compute_accessible_cover_accent(hex, p.is_dark, &p.view_bg, &p.card_bg),
                None => {
                    let (vbg, cbg) = if is_dark {
                        (ADWAITA_DARK_VIEW_BG, ADWAITA_DARK_CARD_BG)
                    } else {
                        (ADWAITA_LIGHT_VIEW_BG, ADWAITA_LIGHT_CARD_BG)
                    };
                    compute_accessible_cover_accent(hex, is_dark, vbg, cbg)
                }
            })
        } else {
            None
        };

        let snapshot = match palette {
            Some(p) => {
                let (accent, highlight) = match computed_cover {
                    Some(c) => (c.accent, c.accent_bg),
                    None => (
                        cover_accent.unwrap_or_else(|| p.accent.clone()),
                        p.accent_bg,
                    ),
                };
                ThemeSnapshot {
                    kind: resolved_kind,
                    is_dark: p.is_dark,
                    background: p.window_bg,
                    surface: p.card_bg,
                    foreground: p.window_fg,
                    accent,
                    highlight,
                }
            }
            None => {
                let (bg, surf, fg) = if is_dark {
                    (ADWAITA_DARK_VIEW_BG, ADWAITA_DARK_CARD_BG, "#ffffff")
                } else {
                    ("#fafafa", ADWAITA_LIGHT_CARD_BG, "#241f31")
                };
                let (accent, highlight) = match computed_cover {
                    Some(c) => (c.accent, c.accent_bg),
                    None => {
                        let a = cover_accent.unwrap_or_else(|| system_accent.clone());
                        (a, system_accent)
                    }
                };
                ThemeSnapshot {
                    kind: resolved_kind,
                    is_dark,
                    background: bg.to_string(),
                    surface: surf.to_string(),
                    foreground: fg.to_string(),
                    accent,
                    highlight,
                }
            }
        };
        *self.snapshot.borrow_mut() = snapshot;

        let observers = self.observers.borrow().clone();
        for cb in observers {
            cb();
        }
    }

    pub fn subscribe(&self, cb: Rc<dyn Fn()>) {
        self.observers.borrow_mut().push(cb);
    }

    pub fn current_snapshot(&self) -> ThemeSnapshot {
        self.snapshot.borrow().clone()
    }
}

pub struct ThemeController {
    app_model: Rc<AppModel>,
    engine: Rc<ThemeEngine>,
    current_art_url: Rc<RefCell<Option<String>>>,
    _settings: gio::Settings,
}

impl ThemeController {
    pub fn new(app_model: Rc<AppModel>, engine: Rc<ThemeEngine>) -> Self {
        let current_art_url = Rc::new(RefCell::new(None));
        let settings = gio::Settings::new(SETTINGS);

        let weak_model = Rc::downgrade(&app_model);
        let weak_engine = Rc::downgrade(&engine);
        let url_cell = Rc::clone(&current_art_url);
        settings.connect_changed(Some(ADAPTIVE_COVER_COLORS), move |_, _| {
            if let (Some(model), Some(engine)) = (weak_model.upgrade(), weak_engine.upgrade()) {
                refresh_cover_accent_for(&model, &engine, &url_cell);
            }
        });

        Self {
            app_model,
            engine,
            current_art_url,
            _settings: settings,
        }
    }

    fn refresh_cover_accent(&self) {
        refresh_cover_accent_for(&self.app_model, &self.engine, &self.current_art_url);
    }
}

fn refresh_cover_accent_for(
    app_model: &Rc<AppModel>,
    engine: &Rc<ThemeEngine>,
    current_art_url: &Rc<RefCell<Option<String>>>,
) {
    if !engine.adaptive_enabled() {
        *current_art_url.borrow_mut() = None;
        engine.set_cover_accent(None);
        return;
    }
    let art_url = app_model
        .get_state()
        .playback
        .current_song()
        .and_then(|song| song.art.best_for_width(64).map(str::to_owned));

    let Some(url) = art_url else {
        *current_art_url.borrow_mut() = None;
        engine.set_cover_accent(None);
        return;
    };

    if current_art_url.borrow().as_deref() == Some(url.as_str()) {
        return;
    }
    *current_art_url.borrow_mut() = Some(url.clone());

    let api = app_model.api();
    let engine = Rc::clone(engine);
    let url_cell = Rc::clone(current_art_url);

    glib::spawn_future_local(async move {
        let tag = Load::background();
        if let Some(texture) = api.load_image(&url, 32, 32, tag).await {
            if url_cell.borrow().as_deref() == Some(url.as_str()) {
                let accent = extract_cover_accent_from_texture(&texture);
                engine.set_cover_accent(accent);
            }
        }
    });
}

impl EventListener for ThemeController {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::PlaybackEvent(
                PlaybackEvent::TrackChanged(_) | PlaybackEvent::TrackSynced,
            ) => self.refresh_cover_accent(),
            AppEvent::PlaybackEvent(PlaybackEvent::PlaybackStopped) => {
                *self.current_art_url.borrow_mut() = None;
                self.engine.set_cover_accent(None);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "canto_theme_test_{tag}_{}_{}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&dir).expect("create test temp dir");
        dir
    }

    #[test]
    fn parses_omarchy_colors_toml_dark_and_light() {
        let toml = r##"
# Comment
mode = "dark"
accent = "#d4b483" # trailing comment after quoted hex
background = "#141414"
dark_background = "#0e0e0e"
darker_background = "#090909"
lighter_background = "#1e1d1a"
foreground = "#e6e1d6"
"##;
        let palette = parse_omarchy_colors_toml(toml).expect("valid dark omarchy colors");
        assert!(palette.is_dark);
        assert_eq!(palette.window_bg, "#141414");
        assert_eq!(palette.sidebar_bg, "#090909");
        assert_eq!(palette.card_bg, "#1e1d1a");
        assert_eq!(palette.accent, "#d4b483");
        assert_eq!(palette.accent_fg, "#111111");

        let light = r##"
mode = 'light'
accent = "#1d4ed8"
background = "#f8fafc"
foreground = "#0f172a"
"##;
        let light_palette = parse_omarchy_colors_toml(light).expect("valid light omarchy colors");
        assert!(!light_palette.is_dark);
        assert_eq!(light_palette.accent_fg, "#ffffff");
    }

    #[test]
    fn rejects_invalid_or_incomplete_colors_toml() {
        assert_eq!(parse_omarchy_colors_toml("accent = \"#fff\""), None);
        assert_eq!(
            parse_omarchy_colors_toml("background = \"not-a-color\"\nforeground = \"#ffffff\""),
            None
        );
    }

    #[test]
    fn parses_matugen_colors_json_mode_first_color_first_and_flat() {
        // Mode-first with both dark and light sections -> follows system dark/light.
        let mode_first = r##"{
            "colors": {
                "dark": {
                    "background": "#111318",
                    "on_background": "#e2e2e9",
                    "surface_container": "#1e2025",
                    "surface_container_low": "#191c20",
                    "surface_container_lowest": "#0c0e13",
                    "surface_container_high": "#282a2f",
                    "primary": "#adc6ff",
                    "on_primary": "#102f60"
                },
                "light": {
                    "background": "#f9f9ff",
                    "on_background": "#191c20",
                    "surface_container": "#ededf4",
                    "primary": "#445e91",
                    "on_primary": "#ffffff"
                }
            }
        }"##;
        let (dark, follows_dark) =
            parse_matugen_colors_json(mode_first, true).expect("dark matugen");
        assert!(dark.is_dark);
        assert!(follows_dark);
        assert_eq!(dark.window_bg, "#111318");
        assert_eq!(dark.sidebar_bg, "#0c0e13");
        assert_eq!(dark.card_bg, "#1e2025");
        assert_eq!(dark.accent_bg, "#adc6ff");
        assert_eq!(dark.accent_fg, "#102f60");

        let (light, follows_light) =
            parse_matugen_colors_json(mode_first, false).expect("light matugen");
        assert!(!light.is_dark);
        assert!(follows_light);
        assert_eq!(light.window_bg, "#f9f9ff");
        assert_eq!(light.accent_bg, "#445e91");
        assert_eq!(light.accent_fg, "#ffffff");

        // Color-first v3 nested `{ "color": "#..." }` with explicit mode lock.
        let color_first = r##"{
            "mode": "dark",
            "colors": {
                "background": { "dark": { "color": "#101418" }, "light": { "color": "#f8f9ff" } },
                "on_background": { "dark": { "color": "#e0e2e8" }, "light": { "color": "#181c20" } },
                "primary": { "dark": { "color": "#94cdf7" }, "light": { "color": "#256489" } }
            }
        }"##;
        let (cf, follows_cf) =
            parse_matugen_colors_json(color_first, false).expect("color-first matugen");
        assert!(cf.is_dark);
        assert!(!follows_cf, "explicit mode locks scheme");
        assert_eq!(cf.window_bg, "#101418");
        assert_eq!(cf.accent_bg, "#94cdf7");

        // Flat template JSON.
        let flat = r##"{
            "background": "#181214",
            "on_surface": "#ede0e2",
            "primary": "#ffb1c4"
        }"##;
        let (flat_pal, follows_flat) = parse_matugen_colors_json(flat, true).expect("flat matugen");
        assert!(flat_pal.is_dark);
        assert!(!follows_flat);
        assert_eq!(flat_pal.window_bg, "#181214");
        assert_eq!(flat_pal.accent_bg, "#ffb1c4");
    }

    #[test]
    fn parses_pywal_and_wallust_colors_json() {
        let pywal = r##"{
            "wallpaper": "/tmp/wall.jpg",
            "special": {
                "background": "#121316",
                "foreground": "#c5c8c6"
            },
            "colors": {
                "color0": "#121316",
                "color1": "#a54242",
                "color2": "#8c9440",
                "color4": "#5f819d",
                "color15": "#c5c8c6"
            }
        }"##;
        let pal = parse_pywal_colors_json(pywal).expect("valid pywal");
        assert!(pal.is_dark);
        assert_eq!(pal.window_bg, "#121316");
        assert_eq!(pal.window_fg, "#c5c8c6");
        assert_eq!(pal.accent_bg, "#5f819d");
        assert!(
            wcag_contrast_ratio_hex(&pal.accent, &pal.view_bg).unwrap() >= WCAG_AA_TEXT_CONTRAST
        );
        assert!(
            wcag_contrast_ratio_hex(&pal.accent_fg, &pal.accent_bg).unwrap()
                >= WCAG_AA_TEXT_CONTRAST
        );

        // Wallust flat JSON with a light background.
        let wallust_light = r##"{
            "background": "#f5f2eb",
            "foreground": "#24211c",
            "color0": "#f5f2eb",
            "color4": "#d9c862"
        }"##;
        let wpal = parse_wallust_colors_json(wallust_light).expect("valid flat wallust");
        assert!(!wpal.is_dark);
        assert_eq!(wpal.window_bg, "#f5f2eb");
        // Pale yellow color4 on light bg must be darkened to remain legible.
        assert!(
            wcag_contrast_ratio_hex(&wpal.accent, &wpal.view_bg).unwrap() >= WCAG_AA_TEXT_CONTRAST
        );
        assert!(
            wcag_contrast_ratio_hex(&wpal.accent_fg, &wpal.accent_bg).unwrap()
                >= WCAG_AA_TEXT_CONTRAST
        );

        // Pywal rejects flat JSON without special/colors and incomplete data.
        assert_eq!(parse_pywal_colors_json(wallust_light), None);
        assert_eq!(
            parse_wallust_colors_json(r##"{"background": "#111111"}"##),
            None
        );
    }

    #[test]
    fn system_theme_cascade_respects_order_and_falls_back_to_native_none() {
        let dir = temp_test_dir("cascade");
        let omarchy = dir.join("omarchy/colors.toml");
        let matugen = dir.join("matugen/colors.json");
        let pywal = dir.join("wal/colors.json");
        let wallust = dir.join("wallust/colors.json");
        for p in [&omarchy, &matugen, &pywal, &wallust].iter() {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        }
        let paths = SystemThemePaths {
            omarchy: omarchy.clone(),
            matugen: vec![matugen.clone()],
            pywal: vec![pywal.clone()],
            wallust: vec![wallust.clone()],
        };

        // 5. No palette files -> native system fallback (None), never SpotifyGreen.
        assert_eq!(resolve_system_palette(&paths, true), None);

        // 4. Only wallust -> resolves Wallust.
        std::fs::write(
            &wallust,
            r##"{"background":"#1a1820","foreground":"#eeeaf5","color4":"#8b6fe8"}"##,
        )
        .unwrap();
        let res = resolve_system_palette(&paths, true).expect("wallust");
        assert_eq!(res.source, SystemThemeSource::Wallust);
        assert_eq!(res.palette.window_bg, "#1a1820");

        // 3. Add pywal -> Pywal wins over Wallust.
        std::fs::write(
            &pywal,
            r##"{"special":{"background":"#11151c","foreground":"#d8dee9"},"colors":{"color4":"#5e81ac"}}"##,
        )
        .unwrap();
        let res = resolve_system_palette(&paths, true).expect("pywal");
        assert_eq!(res.source, SystemThemeSource::Pywal);
        assert_eq!(res.palette.window_bg, "#11151c");

        // 2. Add matugen -> Matugen wins over Pywal.
        std::fs::write(
            &matugen,
            r##"{"colors":{"dark":{"background":"#10131a","on_background":"#e2e2e9","primary":"#a8c7fa"}}}"##,
        )
        .unwrap();
        let res = resolve_system_palette(&paths, true).expect("matugen");
        assert_eq!(res.source, SystemThemeSource::Matugen);
        assert_eq!(res.palette.window_bg, "#10131a");

        // 1. Add Omarchy -> Omarchy wins over Matugen.
        std::fs::write(
            &omarchy,
            "mode = \"dark\"\nbackground = \"#141414\"\nforeground = \"#e6e1d6\"\naccent = \"#d4b483\"\n",
        )
        .unwrap();
        let res = resolve_system_palette(&paths, true).expect("omarchy");
        assert_eq!(res.source, SystemThemeSource::Omarchy);
        assert_eq!(res.palette.window_bg, "#141414");

        // Corrupted Omarchy file gracefully falls through to Matugen instead of breaking.
        std::fs::write(&omarchy, "not valid colors").unwrap();
        let res = resolve_system_palette(&paths, true).expect("fallback to matugen");
        assert_eq!(res.source, SystemThemeSource::Matugen);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wcag_contrast_and_accessible_cover_accent_hold_on_dark_and_light_themes() {
        let samples = [
            "#000000", "#182448", "#2a2a2a", "#1db954", "#767676", "#808080", "#d63636", "#d4b483",
            "#f0e27b", "#e0e0e0", "#ffffff",
        ];

        for &hex in &samples {
            // Every RGB color gets >= 4.5:1 button/badge foreground contrast.
            let fg = contrasting_fg_for_hex(hex);
            let fg_ratio = wcag_contrast_ratio_hex(&fg, hex).expect("valid hex");
            assert!(
                fg_ratio >= WCAG_AA_TEXT_CONTRAST,
                "contrasting_fg_for_hex({}) = {} had contrast {:.2} < 4.5",
                hex,
                fg,
                fg_ratio
            );

            // Dark theme surfaces.
            let dark = compute_accessible_cover_accent(hex, true, "#121212", "#1b1b1b")
                .expect("dark computed accent");
            assert!(
                wcag_contrast_ratio_hex(&dark.accent_bg, "#1b1b1b").unwrap()
                    >= WCAG_UI_COMPONENT_CONTRAST,
                "dark accent_bg for {} too dim",
                hex
            );
            assert!(
                wcag_contrast_ratio_hex(&dark.accent, "#1b1b1b").unwrap() >= WCAG_AA_TEXT_CONTRAST,
                "dark accent text for {} too dim",
                hex
            );
            assert!(
                wcag_contrast_ratio_hex(&dark.accent_fg, &dark.accent_bg).unwrap()
                    >= WCAG_AA_TEXT_CONTRAST,
                "dark accent_fg for {} failed AA",
                hex
            );

            // Light theme surfaces.
            let light = compute_accessible_cover_accent(hex, false, "#fafafa", "#ffffff")
                .expect("light computed accent");
            assert!(
                wcag_contrast_ratio_hex(&light.accent_bg, "#ffffff").unwrap()
                    >= WCAG_UI_COMPONENT_CONTRAST,
                "light accent_bg for {} too bright",
                hex
            );
            assert!(
                wcag_contrast_ratio_hex(&light.accent, "#ffffff").unwrap() >= WCAG_AA_TEXT_CONTRAST,
                "light accent text for {} too bright",
                hex
            );
            assert!(
                wcag_contrast_ratio_hex(&light.accent_fg, &light.accent_bg).unwrap()
                    >= WCAG_AA_TEXT_CONTRAST,
                "light accent_fg for {} failed AA",
                hex
            );
        }
    }

    #[test]
    fn extracts_dominant_chromatic_color_from_rgba() {
        // Two vivid red pixels + one muted gray pixel -> skewed cleanly toward warm red.
        let rgba = [
            220, 40, 40, 255, // vivid red
            220, 40, 40, 255, // vivid red
            120, 120, 120, 255, // gray
            0, 0, 0, 0, // transparent ignored
        ];
        let hex = extract_cover_accent_from_rgba(&rgba).expect("cover color");
        assert!(hex.starts_with('#') && hex.len() == 7);
        let (r, g, b) = parse_rgb_hex(&hex).expect("valid rgb");
        assert!(r > g + 80 && r > b + 80);
    }

    #[test]
    fn builds_css_variables_and_cover_accent() {
        let p = ThemePalette::spotify_green();
        let css = build_theme_css(Some(&p), Some("#ab4422"));
        assert!(css.contains("@define-color window_bg_color #121212;"));
        assert!(css.contains("@define-color accent_bg_color #1db954;"));
        assert!(css.contains("--accent-color: #1ed760;"));
        assert!(css.contains("@define-color canto_cover_accent #ab4422;"));

        let neutral_css = build_theme_css(None, None);
        assert!(!neutral_css.contains("window_bg_color"));
        assert!(neutral_css.contains("@define-color canto_cover_accent #1ed760;"));
    }

    #[test]
    fn builds_css_with_dynamic_cover_accent_on_preset_and_native_themes() {
        let p = ThemePalette::spotify_green();
        // When enabled on a preset palette, accent variables use the contrast-safe cover accent.
        let css = build_theme_css_with_options(Some(&p), Some("#182448"), true, true, None);
        let expected =
            compute_accessible_cover_accent("#182448", true, &p.view_bg, &p.card_bg).unwrap();
        assert!(css.contains(&format!("--accent-bg-color: {};", expected.accent_bg)));
        assert!(css.contains(&format!("--accent-color: {};", expected.accent)));
        assert!(css.contains(&format!("--accent-fg-color: {};", expected.accent_fg)));
        assert!(!css.contains("--accent-bg-color: #1db954;"));

        // On native system theme (None) with cover accent on controls enabled:
        // overrides only accent tokens, never window_bg_color.
        let native_light =
            build_theme_css_with_options(None, Some("#f0e27b"), true, false, Some("#1c71d8"));
        let expected_light = compute_accessible_cover_accent(
            "#f0e27b",
            false,
            ADWAITA_LIGHT_VIEW_BG,
            ADWAITA_LIGHT_CARD_BG,
        )
        .unwrap();
        assert!(!native_light.contains("window_bg_color"));
        assert!(native_light.contains(&format!("--accent-bg-color: {};", expected_light.accent_bg)));
        assert!(native_light.contains(&format!("--accent-color: {};", expected_light.accent)));
        assert!(native_light.contains(&format!("--accent-fg-color: {};", expected_light.accent_fg)));

        // On native system theme with cover accent on controls disabled:
        // leaves all --accent-* variables to libadwaita and uses system accent for cover fallback.
        let native_plain = build_theme_css_with_options(None, None, false, false, Some("#9141ac"));
        assert!(!native_plain.contains("--accent-bg-color"));
        assert!(!native_plain.contains("window_bg_color"));
        assert!(native_plain.contains("@define-color canto_cover_accent #9141ac;"));
    }
}
