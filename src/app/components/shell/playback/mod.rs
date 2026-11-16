mod component;
mod playback_controls;
mod playback_info;
mod playback_widget;
pub use component::*;
#[cfg(test)]
pub(crate) use playback_widget::PlaybackWidget;

use glib::prelude::*;

pub fn expose_widgets() {
    playback_widget::PlaybackWidget::static_type();
}
