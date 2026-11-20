//! Tracks the last Spotify link that Canto itself wrote to the clipboard.
//!
//! Canto has "Copy link" actions that place `https://open.spotify.com/...` URLs
//! on the clipboard. The clipboard watcher should not turn around and offer to
//! open a link the user just copied *from* Canto, so those actions record the
//! link here and the watcher checks against it.
//!
//! GTK runs entirely on the main-context thread, so a `thread_local!` is a
//! simple and correct place to keep this shared state.

use std::cell::RefCell;
use std::collections::VecDeque;

use gdk::prelude::*;

/// How many of the links Canto put on the clipboard it remembers.
const REMEMBERED_LINKS: usize = 8;

thread_local! {
    static APP_COPIED_LINKS: RefCell<VecDeque<String>> = const { RefCell::new(VecDeque::new()) };
}

/// Record a link that Canto just wrote to the clipboard, or that a label's
/// own menu may copy ("Copy Link Address"). A few are kept: a link opened
/// from a label menu without copying it must not make Canto forget the one
/// it really copied before.
pub fn remember_app_copied_link(link: &str) {
    let link = link.trim().to_string();
    APP_COPIED_LINKS.with(|links| {
        let mut links = links.borrow_mut();
        links.retain(|known| known != &link);
        links.push_back(link);
        while links.len() > REMEMBERED_LINKS {
            links.pop_front();
        }
    });
}

/// Copy a link to the clipboard and record it as app-copied, so the clipboard
/// watcher won't offer to open the link the user just copied from Canto.
pub fn copy_link_to_clipboard(link: &str) {
    let clipboard = gdk::Display::default().unwrap().clipboard();
    clipboard
        .set_content(Some(&gdk::ContentProvider::for_value(&link.to_value())))
        .expect("Failed to set clipboard content");
    remember_app_copied_link(link);
}

/// Whether the given clipboard text is one of the last links Canto copied.
pub fn is_app_copied_link(text: &str) -> bool {
    let text = text.trim();
    APP_COPIED_LINKS.with(|links| links.borrow().iter().any(|known| known == text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remember_and_match() {
        let link = "https://open.spotify.com/track/remember_and_match";
        remember_app_copied_link(link);
        assert!(is_app_copied_link(link));
        // Matching is whitespace-insensitive on the incoming text.
        assert!(is_app_copied_link(
            "  https://open.spotify.com/track/remember_and_match \n"
        ));
    }

    #[test]
    fn test_non_matching_link() {
        remember_app_copied_link("https://open.spotify.com/track/aaa_non_matching");
        assert!(!is_app_copied_link(
            "https://open.spotify.com/track/bbb_non_matching"
        ));
    }

    #[test]
    fn test_a_few_links_are_remembered() {
        let copied = "https://open.spotify.com/track/copied_from_a_menu";
        remember_app_copied_link(copied);
        // Then a title's link right-clicked (it may be copied, or not)
        remember_app_copied_link("https://open.spotify.com/track/right_clicked");
        assert!(is_app_copied_link(copied));
        for i in 0..REMEMBERED_LINKS {
            remember_app_copied_link(&format!("https://open.spotify.com/track/later_{i}"));
        }
        assert!(!is_app_copied_link(copied), "only the last few are kept");
    }
}
