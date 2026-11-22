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

/// The CSS rule that paints the tint for one color.
pub fn tint_rule(digits: &str) -> String {
    format!(
        ".{CLASS_PREFIX}{digits} {{ background-image: linear-gradient(to bottom, \
         alpha(#{digits}, 0.62) 0px, alpha(#{digits}, 0.22) 340px, transparent 620px); }}"
    )
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
            let css: String = tints.colors.iter().map(|d| tint_rule(d)).collect();
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
            }
            if let Some(provider) = &tints.provider {
                provider.load_from_string(&css);
            }
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
        assert!(tint_rule("a1b2c3").starts_with(".details-tint-a1b2c3 {"));
    }
}
