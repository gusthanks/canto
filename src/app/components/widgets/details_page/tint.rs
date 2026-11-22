// Spotify-style tint for album, playlist and artist pages: a gradient of the
// cover's main color behind the top of the page.

use gtk::prelude::*;
use std::cell::RefCell;

const CLASS_PREFIX: &str = "details-tint-";
/// Distinct colors kept in the stylesheet before it starts over.
const MAX_COLORS: usize = 128;

thread_local! {
    static TINTS: RefCell<Tints> = RefCell::new(Tints::default());
}

#[derive(Default)]
struct Tints {
    provider: Option<gtk::CssProvider>,
    colors: Vec<String>,
}

/// Lowercase hex digits of a "#rrggbb" color, or `None` when it isn't one.
pub(super) fn hex_digits(color: &str) -> Option<String> {
    let digits = color.strip_prefix('#')?;
    (digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| digits.to_ascii_lowercase())
}

/// How strong the tint is at the top of the page, where the title sits.
const TOP_ALPHA: f64 = 0.62;
/// The least contrast the header's title keeps over the tint's top: WCAG
/// AA for large text. 4.5 (normal text) darkened every warm cover in dark
/// themes; 3 fixes white and pale covers and leaves the others as they were.
const MIN_TEXT_CONTRAST: f64 = 3.0;
/// The page and its text, in a dark and in a light theme. The text is a
/// dim worst case (some themes' text is grey, not white or black).
const DARK_PAGE: (u8, u8, u8) = (30, 30, 30);
const DARK_TEXT: (u8, u8, u8) = (170, 170, 170);
const LIGHT_PAGE: (u8, u8, u8) = (250, 250, 250);
const LIGHT_TEXT: (u8, u8, u8) = (60, 60, 60);

fn rgb_of(digits: &str) -> Option<(u8, u8, u8)> {
    let byte = |i: usize| u8::from_str_radix(digits.get(i..i + 2)?, 16).ok();
    Some((byte(0)?, byte(2)?, byte(4)?))
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
    let ch = |x: u8, y: u8| (f64::from(x) * (1.0 - t) + f64::from(y) * t).round() as u8;
    (ch(a.0, b.0), ch(a.1, b.1), ch(a.2, b.2))
}

/// The cover's color, darkened in a dark theme or lightened in a light one
/// until the header's text stays readable over the tint's top: a white
/// cover made the title nearly vanish in dark themes (Q27). Colors that
/// already leave the text readable come back unchanged.
pub(super) fn readable_tint(color: (u8, u8, u8), dark: bool) -> (u8, u8, u8) {
    let (page, text, toward) = if dark {
        (DARK_PAGE, DARK_TEXT, (0, 0, 0))
    } else {
        (LIGHT_PAGE, LIGHT_TEXT, (255, 255, 255))
    };
    let mut c = color;
    for _ in 0..60 {
        let top = mix(page, c, TOP_ALPHA);
        if crate::app::theme::wcag_contrast_ratio(text, top) >= MIN_TEXT_CONTRAST {
            break;
        }
        c = mix(c, toward, 0.06);
    }
    c
}

/// The CSS rule that paints the tint for one color (the class keeps the
/// cover's own color; the gradient uses its readable version).
pub fn tint_rule(digits: &str, dark: bool) -> String {
    let paint = rgb_of(digits)
        .map(|rgb| {
            let (r, g, b) = readable_tint(rgb, dark);
            format!("{r:02x}{g:02x}{b:02x}")
        })
        .unwrap_or_else(|| digits.to_string());
    format!(
        ".{CLASS_PREFIX}{digits} {{ background-image: linear-gradient(to bottom, \
         alpha(#{paint}, {TOP_ALPHA}) 0px, alpha(#{paint}, 0.22) 340px, transparent 620px); }}"
    )
}

fn is_dark() -> bool {
    libadwaita::StyleManager::default().is_dark()
}

/// Rewrites every tint for the current light or dark theme.
fn reload(tints: &Tints) {
    if let Some(provider) = &tints.provider {
        let dark = is_dark();
        let css: String = tints.colors.iter().map(|d| tint_rule(d, dark)).collect();
        provider.load_from_string(&css);
    }
}

/// The tint `widget` has ("#rrggbb"), if any.
pub fn tint_of(widget: &impl IsA<gtk::Widget>) -> Option<String> {
    widget
        .css_classes()
        .iter()
        .find_map(|class| class.strip_prefix(CLASS_PREFIX).map(|d| format!("#{d}")))
}

/// Tints `widget` with `color` ("#rrggbb"); `None` removes the tint.
pub fn set_tint(widget: &impl IsA<gtk::Widget>, color: Option<&str>) {
    let widget = widget.upcast_ref::<gtk::Widget>();
    for class in widget.css_classes() {
        if class.starts_with(CLASS_PREFIX) {
            widget.remove_css_class(&class);
        }
    }
    let Some(digits) = color.and_then(hex_digits) else {
        return;
    };
    TINTS.with(|tints| {
        let mut tints = tints.borrow_mut();
        if !tints.colors.contains(&digits) {
            if tints.colors.len() >= MAX_COLORS {
                tints.colors.clear();
            }
            tints.colors.push(digits.clone());
            if tints.provider.is_none() {
                let provider = gtk::CssProvider::new();
                if let Some(display) = gtk::gdk::Display::default() {
                    gtk::style_context_add_provider_for_display(
                        &display,
                        &provider,
                        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                    );
                }
                tints.provider = Some(provider);
                // A theme switch between light and dark rewrites the tints
                libadwaita::StyleManager::default().connect_dark_notify(|_| {
                    TINTS.with(|tints| reload(&tints.borrow()));
                });
            }
            reload(&tints);
        }
    });
    widget.add_css_class(&format!("{CLASS_PREFIX}{digits}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_hex_colors_make_a_tint() {
        assert_eq!(hex_digits("#A1b2C3").as_deref(), Some("a1b2c3"));
        assert_eq!(hex_digits("a1b2c3"), None);
        assert_eq!(hex_digits("#a1b2c"), None);
        assert_eq!(hex_digits("#zzzzzz"), None);
        assert!(tint_rule("a1b2c3", true).starts_with(".details-tint-a1b2c3 {"));
    }

    fn top_contrast(color: (u8, u8, u8), dark: bool) -> f64 {
        let (page, text) = if dark {
            (DARK_PAGE, DARK_TEXT)
        } else {
            (LIGHT_PAGE, LIGHT_TEXT)
        };
        crate::app::theme::wcag_contrast_ratio(text, mix(page, color, TOP_ALPHA))
    }

    #[test]
    fn a_white_cover_in_a_dark_theme_leaves_the_title_readable() {
        let white = (240, 240, 240);
        assert!(top_contrast(white, true) < MIN_TEXT_CONTRAST);
        let tint = readable_tint(white, true);
        assert!(
            top_contrast(tint, true) >= MIN_TEXT_CONTRAST,
            "{:?}: {}",
            tint,
            top_contrast(tint, true)
        );
    }

    #[test]
    fn a_black_cover_in_a_light_theme_leaves_the_title_readable() {
        let black = (12, 12, 12);
        assert!(top_contrast(black, false) < MIN_TEXT_CONTRAST);
        let tint = readable_tint(black, false);
        assert!(
            top_contrast(tint, false) >= MIN_TEXT_CONTRAST,
            "{:?}: {}",
            tint,
            top_contrast(tint, false)
        );
    }

    #[test]
    fn a_tint_keeps_its_hue_and_readable_colors_stay_as_they_are() {
        // An orange cover only gets darker: still orange
        let (r, g, b) = readable_tint((214, 120, 48), true);
        assert!(r > g && g > b, "{:?}", (r, g, b));
        // A deep blue already leaves light text readable
        assert_eq!(readable_tint((20, 40, 110), true), (20, 40, 110));
        // The rule paints the readable color, under the cover's own class
        let rule = tint_rule("f0f0f0", true);
        assert!(rule.starts_with(".details-tint-f0f0f0 {"), "{}", rule);
        assert!(!rule.contains("alpha(#f0f0f0"), "{}", rule);
    }
}
