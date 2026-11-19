use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};

use crate::app::components::labels;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/io/github/gusthanks/Canto/components/queue_page.ui")]
    pub struct QueuePageWidget {
        #[template_child]
        pub selection_bar: TemplateChild<gtk::CenterBox>,
        #[template_child]
        pub selection_cancel: TemplateChild<gtk::Button>,
        #[template_child]
        pub selection_count: TemplateChild<gtk::Label>,
        #[template_child]
        pub select_all: TemplateChild<gtk::Button>,
        #[template_child]
        pub recent_section: TemplateChild<gtk::Box>,
        #[template_child]
        pub recent_toggle: TemplateChild<gtk::Button>,
        #[template_child]
        pub recent_chevron: TemplateChild<gtk::Image>,
        #[template_child]
        pub recent_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub recent_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub recent_scroller: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub recent_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub recent_status: TemplateChild<gtk::Label>,
        #[template_child]
        pub queue_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub queue_scroller: TemplateChild<gtk::ScrolledWindow>,

        pub queue_list: RefCell<Option<gtk::Widget>>,
        pub extra_margin_x: Cell<i32>,
        pub selection_bar_hidden: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for QueuePageWidget {
        const NAME: &'static str = "QueuePageWidget";
        type Type = super::QueuePageWidget;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for QueuePageWidget {
        fn constructed(&self) {
            self.parent_constructed();
            let revealer = self.recent_revealer.get();
            self.recent_toggle.connect_clicked(move |_| {
                revealer.set_reveal_child(!revealer.reveals_child());
            });
            let obj = self.obj().downgrade();
            self.recent_revealer
                .connect_reveal_child_notify(move |revealer| {
                    if let Some(obj) = obj.upgrade() {
                        obj.show_recent_expanded(revealer.reveals_child());
                    }
                });
            self.obj().show_recent_expanded(false);
        }
    }
    impl WidgetImpl for QueuePageWidget {}
    impl BinImpl for QueuePageWidget {}
}

glib::wrapper! {
    pub struct QueuePageWidget(ObjectSubclass<imp::QueuePageWidget>)
        @extends gtk::Widget, libadwaita::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

const LIST_MARGIN: i32 = 12;
/// The size of a cover in the Recently Played list.
pub const RECENT_COVER_PX: i32 = 36;

/// One song of the Recently Played list, as shown.
#[derive(Clone, Debug, Default)]
pub struct RecentRow {
    pub title: String,
    /// Artists, and when it played.
    pub subtitle: String,
}

impl Default for QueuePageWidget {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl QueuePageWidget {
    pub fn set_queue_list(&self, list: &impl IsA<gtk::Widget>) {
        list.set_margin_top(LIST_MARGIN);
        list.set_margin_bottom(LIST_MARGIN);
        let imp = self.imp();
        imp.queue_scroller.set_child(Some(list));
        imp.queue_list.replace(Some(list.clone().upcast()));
        self.update_list_margins();
    }

    pub fn set_queue_empty(&self, empty: bool) {
        let page = if empty { "empty" } else { "list" };
        self.imp().queue_stack.set_visible_child_name(page);
    }

    pub fn connect_selection_cancel<F: Fn() + 'static>(&self, f: F) {
        self.imp().selection_cancel.connect_clicked(move |_| f());
    }

    pub fn connect_select_all<F: Fn() + 'static>(&self, f: F) {
        self.imp().select_all.connect_clicked(move |_| f());
    }

    pub fn set_selection_bar_hidden(&self, hidden: bool) {
        let imp = self.imp();
        imp.selection_bar_hidden.set(hidden);
        if hidden {
            imp.selection_bar.set_visible(false);
        }
    }

    pub fn set_selection(&self, selected: Option<usize>, all_selected: bool) {
        let imp = self.imp();
        imp.selection_bar
            .set_visible(selected.is_some() && !imp.selection_bar_hidden.get());
        // Selecting picks songs of the queue: Recently Played steps aside
        imp.recent_section.set_visible(selected.is_none());
        let Some(count) = selected else {
            return;
        };
        imp.selection_count
            .set_label(&labels::n_tracks_selected_label(count));
        imp.select_all.set_label(&if all_selected {
            // translators: Button label. Deselects all songs, once they are all selected.
            gettext("Deselect All")
        } else {
            // translators: Button label. Selects all visible songs.
            gettext("Select All")
        });
    }

    pub fn scroll_to_top(&self) {
        self.imp().queue_scroller.vadjustment().set_value(0.0);
    }

    pub fn set_extra_margin_x(&self, margin: i32) {
        self.imp().extra_margin_x.set(margin);
        self.update_list_margins();
    }

    pub fn set_fit_content_height(&self, fit: bool) {
        self.imp().queue_scroller.set_propagate_natural_height(fit);
    }

    fn update_list_margins(&self) {
        let imp = self.imp();
        let Some(list) = imp.queue_list.borrow().clone() else {
            return;
        };
        let margin_x = LIST_MARGIN + imp.extra_margin_x.get();
        list.set_margin_start(margin_x);
        list.set_margin_end(margin_x);
    }

    /// Whether Recently Played is unfolded.
    pub fn is_recent_expanded(&self) -> bool {
        self.imp().recent_revealer.reveals_child()
    }

    #[cfg(test)]
    pub fn set_recent_expanded(&self, expanded: bool) {
        self.imp().recent_revealer.set_reveal_child(expanded);
    }

    /// Calls `f` with the new state when Recently Played folds or unfolds.
    pub fn connect_recent_toggled<F: Fn(bool) + 'static>(&self, f: F) {
        self.imp()
            .recent_revealer
            .connect_reveal_child_notify(move |revealer| f(revealer.reveals_child()));
    }

    /// Calls `f` with the position of the Recently Played song clicked.
    pub fn connect_recent_activated<F: Fn(usize) + 'static>(&self, f: F) {
        self.imp().recent_list.connect_row_activated(move |_, row| {
            let index = row.index();
            if index >= 0 {
                f(index as usize);
            }
        });
    }

    fn show_recent_expanded(&self, expanded: bool) {
        let imp = self.imp();
        imp.recent_chevron.set_icon_name(Some(if expanded {
            "pan-down-symbolic"
        } else {
            "pan-end-symbolic"
        }));
        imp.recent_toggle
            .update_state(&[gtk::accessible::State::Expanded(Some(expanded))]);
    }

    pub fn show_recent_loading(&self) {
        self.imp().recent_stack.set_visible_child_name("loading");
    }

    /// A short line in place of the list (nothing played, or an error).
    pub fn show_recent_message(&self, text: &str) {
        let imp = self.imp();
        imp.recent_status.set_label(text);
        imp.recent_stack.set_visible_child_name("status");
    }

    /// Fills the list with `rows`, newest first; returns each row's cover
    /// image, for the caller to load.
    pub fn show_recent_rows(&self, rows: &[RecentRow]) -> Vec<gtk::Image> {
        let imp = self.imp();
        imp.recent_list.remove_all();
        let covers = rows
            .iter()
            .map(|row| {
                let (widget, cover) = recent_row(row);
                imp.recent_list.append(&widget);
                cover
            })
            .collect();
        imp.recent_scroller.vadjustment().set_value(0.0);
        imp.recent_stack.set_visible_child_name("list");
        covers
    }
}

fn recent_row(row: &RecentRow) -> (gtk::ListBoxRow, gtk::Image) {
    let cover = gtk::Image::new();
    cover.set_pixel_size(RECENT_COVER_PX);
    cover.set_size_request(RECENT_COVER_PX, RECENT_COVER_PX);
    cover.set_overflow(gtk::Overflow::Hidden);
    cover.add_css_class("queue-page__recent-cover");

    let title = gtk::Label::new(Some(&row.title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let subtitle = gtk::Label::new(Some(&row.subtitle));
    subtitle.set_xalign(0.0);
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.add_css_class("caption");
    subtitle.add_css_class("dim-label");

    let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
    texts.set_valign(gtk::Align::Center);
    texts.set_hexpand(true);
    texts.append(&title);
    texts.append(&subtitle);

    let line = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    line.append(&cover);
    line.append(&texts);

    let widget = gtk::ListBoxRow::new();
    widget.set_child(Some(&line));
    widget.set_tooltip_text(Some(&format!("{} · {}", row.title, row.subtitle)));
    widget.add_css_class("queue-page__recent-row");
    (widget, cover)
}
