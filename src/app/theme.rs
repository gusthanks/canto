use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use canto_api::Load;
use canto_config::settings::{ADAPTIVE_COVER_COLORS, APP_THEME, SETTINGS};
use gio::prelude::{FileExt, FileMonitorExt, SettingsExt};
use gtk::gdk;
use libadwaita as adw;

use crate::app::components::EventListener;
use crate::app::state::PlaybackEvent;
use crate::app::{AppEvent, AppModel};

const THEME_PROVIDER_PRIORITY: u32 = gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1;

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

impl ThemePalette {
    fn preset(kind: ThemeKind) -> Option<Self> {
        match kind {
            ThemeKind::Omarchy => load_omarchy_palette().or_else(|| Some(Self::spotify_green())),
            ThemeKind::Neutral => None,
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

pub fn omarchy_colors_path() -> PathBuf {
    let state_base = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| glib::home_dir().join(".local").join("state"));
    state_base
        .join("omarchy")
        .join("current")
        .join("theme")
        .join("colors.toml")
}

fn load_omarchy_palette() -> Option<ThemePalette> {
    let path = omarchy_colors_path();
    let content = std::fs::read_to_string(&path).ok()?;
    parse_omarchy_colors_toml(&content)
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

fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    let channel = |c: u8| {
        let s = f64::from(c) / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn contrasting_fg_for_hex(hex: &str) -> String {
    match parse_rgb_hex(hex) {
        Some((r, g, b)) if relative_luminance(r, g, b) > 0.42 => "#111111".to_string(),
        _ => "#ffffff".to_string(),
    }
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
    let mut css = String::new();
    let cover_color = cover_accent
        .and_then(valid_hex_color)
        .or_else(|| palette.map(|p| p.accent.clone()))
        .unwrap_or_else(|| "#1ed760".to_string());

    if let Some(p) = palette {
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
            accent_bg = p.accent_bg,
            accent = p.accent,
            accent_fg = p.accent_fg,
        ));
    }

    css.push_str(&format!(
        "@define-color canto_cover_accent {cover_color};\n"
    ));
    css
}

pub struct ThemeEngine {
    provider: gtk::CssProvider,
    settings: gio::Settings,
    cover_accent: RefCell<Option<String>>,
    monitors: RefCell<Vec<gio::FileMonitor>>,
    observers: RefCell<Vec<Rc<dyn Fn()>>>,
}

impl ThemeEngine {
    pub fn install() -> Rc<Self> {
        let provider = gtk::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                THEME_PROVIDER_PRIORITY,
            );
        }

        let settings = gio::Settings::new(SETTINGS);
        let engine = Rc::new(Self {
            provider,
            settings,
            cover_accent: RefCell::new(None),
            monitors: RefCell::new(Vec::new()),
            observers: RefCell::new(Vec::new()),
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
                    engine.sync_omarchy_monitor();
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

        self.sync_omarchy_monitor();
    }

    fn sync_omarchy_monitor(self: &Rc<Self>) {
        self.refresh_omarchy_monitor(false);
    }

    fn refresh_omarchy_monitor(self: &Rc<Self>, force: bool) {
        let kind = ThemeKind::from_setting(&self.settings.string(APP_THEME));
        if kind != ThemeKind::Omarchy {
            self.monitors.borrow_mut().clear();
            return;
        }
        if !force && !self.monitors.borrow().is_empty() {
            return;
        }
        self.monitors.borrow_mut().clear();

        let path = omarchy_colors_path();
        let theme_dir = path.parent().unwrap_or(path.as_path());
        if let Ok(mon) = gio::File::for_path(theme_dir)
            .monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
        {
            mon.connect_changed(glib::clone!(
                #[weak(rename_to = engine)]
                self,
                move |_, changed_file, _, event_type| {
                    let is_colors = changed_file
                        .basename()
                        .and_then(|n| n.to_str().map(|s| s == "colors.toml"))
                        .unwrap_or(true);
                    if is_colors
                        && matches!(
                            event_type,
                            gio::FileMonitorEvent::Changed
                                | gio::FileMonitorEvent::ChangesDoneHint
                                | gio::FileMonitorEvent::Created
                                | gio::FileMonitorEvent::MovedIn
                        )
                    {
                        engine.apply();
                    }
                }
            ));
            self.monitors.borrow_mut().push(mon);
        }

        // Also watch the parent `~/.local/state/omarchy/current/` directory because
        // `omarchy-theme-set` can replace `theme/` or update `theme.generation` / `theme.name`.
        if let Some(current_dir) = theme_dir.parent() {
            if let Ok(parent_mon) = gio::File::for_path(current_dir)
                .monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
            {
                parent_mon.connect_changed(glib::clone!(
                    #[weak(rename_to = engine)]
                    self,
                    move |_, changed_file, _, event_type| {
                        let name = changed_file
                            .basename()
                            .and_then(|n| n.to_str().map(str::to_string));
                        let relevant = matches!(
                            name.as_deref(),
                            Some("theme" | "theme.generation" | "theme.name")
                        );
                        if relevant
                            && matches!(
                                event_type,
                                gio::FileMonitorEvent::Changed
                                    | gio::FileMonitorEvent::ChangesDoneHint
                                    | gio::FileMonitorEvent::Created
                                    | gio::FileMonitorEvent::MovedIn
                                    | gio::FileMonitorEvent::Deleted
                            )
                        {
                            let reattach = name.as_deref() == Some("theme");
                            glib::idle_add_local_once(glib::clone!(
                                #[weak]
                                engine,
                                move || {
                                    if reattach {
                                        engine.refresh_omarchy_monitor(true);
                                    }
                                    engine.apply();
                                }
                            ));
                        }
                    }
                ));
                self.monitors.borrow_mut().push(parent_mon);
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
        let kind = ThemeKind::from_setting(&self.settings.string(APP_THEME));
        let palette = ThemePalette::preset(kind);

        let style_manager = adw::StyleManager::default();
        match &palette {
            None => {
                let pref = match self.settings.string("theme-preference").as_str() {
                    "light" => adw::ColorScheme::ForceLight,
                    "dark" => adw::ColorScheme::ForceDark,
                    _ => adw::ColorScheme::Default,
                };
                style_manager.set_color_scheme(pref);
            }
            Some(p) if p.is_dark => style_manager.set_color_scheme(adw::ColorScheme::ForceDark),
            Some(_) => style_manager.set_color_scheme(adw::ColorScheme::ForceLight),
        }

        let adaptive_enabled = self.settings.boolean(ADAPTIVE_COVER_COLORS);
        let cover_accent = if adaptive_enabled {
            self.cover_accent.borrow().clone()
        } else {
            None
        };
        let css = build_theme_css(palette.as_ref(), cover_accent.as_deref());
        self.provider.load_from_string(&css);
        let observers = self.observers.borrow().clone();
        for cb in observers {
            cb();
        }
    }

    pub fn subscribe(&self, cb: Rc<dyn Fn()>) {
        self.observers.borrow_mut().push(cb);
    }

    pub fn current_snapshot(&self) -> ThemeSnapshot {
        let kind = ThemeKind::from_setting(&self.settings.string(APP_THEME));
        let palette = ThemePalette::preset(kind);
        let adaptive_enabled = self.settings.boolean(ADAPTIVE_COVER_COLORS);
        let cover_accent = if adaptive_enabled {
            self.cover_accent.borrow().clone()
        } else {
            None
        };
        let is_dark = adw::StyleManager::default().is_dark();

        match palette {
            Some(p) => {
                let accent = cover_accent.unwrap_or_else(|| p.accent.clone());
                ThemeSnapshot {
                    kind: kind.setting_key(),
                    is_dark: p.is_dark,
                    background: p.window_bg,
                    surface: p.card_bg,
                    foreground: p.window_fg,
                    accent: accent.clone(),
                    highlight: p.accent_bg,
                }
            }
            None => {
                let (bg, surf, fg, default_accent) = if is_dark {
                    ("#1c1c1e", "#2c2c2e", "#ffffff", "#3584e4")
                } else {
                    ("#fafafa", "#ffffff", "#241f31", "#1c71d8")
                };
                let accent = cover_accent.unwrap_or_else(|| default_accent.to_string());
                ThemeSnapshot {
                    kind: kind.setting_key(),
                    is_dark,
                    background: bg.to_string(),
                    surface: surf.to_string(),
                    foreground: fg.to_string(),
                    accent: accent.clone(),
                    highlight: accent,
                }
            }
        }
    }
}

pub struct ThemeController {
    app_model: Rc<AppModel>,
    engine: Rc<ThemeEngine>,
    current_art_url: Rc<RefCell<Option<String>>>,
}

impl ThemeController {
    pub fn new(app_model: Rc<AppModel>, engine: Rc<ThemeEngine>) -> Self {
        Self {
            app_model,
            engine,
            current_art_url: Rc::new(RefCell::new(None)),
        }
    }

    fn refresh_cover_accent(&self) {
        if !self.engine.adaptive_enabled() {
            *self.current_art_url.borrow_mut() = None;
            self.engine.set_cover_accent(None);
            return;
        }
        let art_url = self
            .app_model
            .get_state()
            .playback
            .current_song()
            .and_then(|song| song.art.best_for_width(64).map(str::to_owned));

        let Some(url) = art_url else {
            *self.current_art_url.borrow_mut() = None;
            self.engine.set_cover_accent(None);
            return;
        };

        if self.current_art_url.borrow().as_deref() == Some(url.as_str()) {
            return;
        }
        *self.current_art_url.borrow_mut() = Some(url.clone());

        let api = self.app_model.api();
        let engine = Rc::clone(&self.engine);
        let url_cell = Rc::clone(&self.current_art_url);

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
}
