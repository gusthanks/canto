// Shown for approval before anything calls it outside the tests
#[cfg_attr(not(test), allow(dead_code))]
pub mod add_lyrics;
mod component;
mod widget;

pub use component::{genius_fallback, LyricsPage};
pub use widget::{expose_widgets, open_external_url, LyricsPageWidget};
