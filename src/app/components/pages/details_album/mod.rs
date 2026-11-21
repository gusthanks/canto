mod model;
mod release_details;
mod widget;

pub use model::DetailsModel;
// The GTK tests build every template widget
#[cfg(test)]
pub use release_details::ReleaseDetailsDialog;
pub use widget::Details;
