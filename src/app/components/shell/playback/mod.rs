mod component;
mod playback_controls;
mod playback_info;
mod playback_widget;
mod volume_scroll;
pub use component::*;
#[cfg(test)]
pub(crate) use playback_widget::PlaybackWidget;
pub(crate) use playback_widget::{scroll_steps_from_event, volume_after_scroll};

use glib::prelude::*;

pub fn expose_widgets() {
    playback_widget::PlaybackWidget::static_type();
}
