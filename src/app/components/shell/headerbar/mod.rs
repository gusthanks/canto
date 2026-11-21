mod widget;
pub use widget::*;

mod history_menu_row;
// pub(crate) for the GTK tests, which build every template widget
pub(crate) use history_menu_row::HistoryMenuRow;

mod component;
pub use component::*;

mod registrar;
pub use registrar::*;

use glib::prelude::*;

pub fn expose_widgets() {
    widget::AppHeaderBar::static_type();
}
