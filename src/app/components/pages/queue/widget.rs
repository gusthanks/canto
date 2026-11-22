use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::app::components::labels;
use crate::app::components::now_panel::show_like_state;

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
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
        pub queue_actions: TemplateChild<gtk::Box>,
        #[template_child]
        pub save_playlist_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub queue_scroller: TemplateChild<gtk::ScrolledWindow>,

        pub queue_list: RefCell<Option<gtk::Widget>>,
        pub extra_margin_x: Cell<i32>,
        pub selection_bar_hidden: Cell<bool>,
        /// Recently Played follows its end (the newest song, next to Now
        /// Playing) as its rows are laid out: from when it's filled or
        /// unfolded until a scroll by hand moves away from the end, and
        /// again once one comes back to it.
        pub recent_at_end: Cell<bool>,
        /// A move to the end is waiting for the current layout to finish.
        pub recent_end_queued: Cell<bool>,
        /// The heart at the end of each Recently Played row, in row order.
        pub recent_hearts: RefCell<Vec<gtk::Button>>,
        /// Called with the row's position when its heart is clicked.
        pub recent_like: RefCell<Option<Rc<dyn Fn(usize)>>>,
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
                        if revealer.reveals_child() {
                            obj.scroll_recent_to_end();
                        }
                    }
                });
            // The list's height is only known once it's laid out, and it
            // grows as rows come: while it's at its end, keep it there. Not
            // from inside "changed" itself: it fires while the list is being
            // laid out, and a scroll asked for then is kept but never drawn,
            // so the list opened at its oldest songs and only jumped down on
            // the first scroll (20/11). No time limit either: on a busy
            // machine the layout came after it ran out
            let adjustment = self.recent_scroller.vadjustment();
            let obj = self.obj().downgrade();
            adjustment.connect_changed(move |_| {
                if let Some(obj) = obj.upgrade() {
                    if obj.imp().recent_at_end.get() {
                        obj.queue_recent_end();
                    }
                }
            });
            // A scroll by hand away from the end lets the list stay put
            let obj = self.obj().downgrade();
            adjustment.connect_value_changed(move |adjustment| {
                if let Some(obj) = obj.upgrade() {
                    let end = adjustment.upper() - adjustment.page_size();
                    obj.imp().recent_at_end.set(adjustment.value() >= end - 1.0);
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
/// How many songs Recently Played shows at once when unfolded; the rest
/// scroll. Three leave Now Playing and the next song in view below it.
pub const RECENT_VISIBLE_ROWS: i32 = 3;
/// The list's bottom padding (app.css, `.queue-page__recent-list`).
const RECENT_LIST_BOTTOM_PADDING: i32 = 6;
/// The size of a cover in the Recently Played list.
pub const RECENT_COVER_PX: i32 = 36;

/// One song of the Recently Played list, as shown.
#[derive(Clone, Debug, Default)]
pub struct RecentRow {
    pub title: String,
    /// Artists, and when it played.
    pub subtitle: String,
    /// In the saved tracks: a full heart that stays on.
    pub liked: bool,
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

    pub fn set_save_playlist_visible(&self, visible: bool) {
        self.imp().queue_actions.set_visible(visible);
    }

    pub fn is_save_playlist_visible(&self) -> bool {
        self.imp().queue_actions.is_visible()
    }

    pub fn connect_save_playlist<F: Fn(&gtk::Button) + 'static>(&self, f: F) {
        self.imp()
            .save_playlist_button
            .connect_clicked(move |button| f(button));
    }

    #[cfg(test)]
    pub fn save_playlist_button_for_tests(&self) -> gtk::Button {
        self.imp().save_playlist_button.get()
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

    /// Calls `f` with the position of the Recently Played song whose heart
    /// was clicked.
    pub fn connect_recent_like<F: Fn(usize) + 'static>(&self, f: F) {
        self.imp().recent_like.replace(Some(Rc::new(f)));
    }

    /// Shows each row's heart full or empty, in row order.
    pub fn set_recent_liked(&self, liked: &[bool]) {
        for (heart, liked) in self.imp().recent_hearts.borrow().iter().zip(liked) {
            show_like_state(heart, Some(*liked));
        }
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

    /// Shows the end of Recently Played, the newest song, and keeps it there
    /// while the list settles.
    fn scroll_recent_to_end(&self) {
        self.imp().recent_at_end.set(true);
        self.queue_recent_end();
    }

    /// Scrolls Recently Played to its end once the layout under way is over
    /// (once, however many times it's asked meanwhile).
    fn queue_recent_end(&self) {
        let imp = self.imp();
        if imp.recent_end_queued.replace(true) {
            return;
        }
        let obj = self.downgrade();
        glib::idle_add_local_once(move || {
            let Some(obj) = obj.upgrade() else {
                return;
            };
            let imp = obj.imp();
            imp.recent_end_queued.set(false);
            // The rows' real height is known once they're on screen (filled
            // before, they may have been measured without their style)
            obj.fit_recent_rows();
            let adj = imp.recent_scroller.vadjustment();
            adj.set_value(adj.upper() - adj.page_size());
            // Placed again even when the value was already right: that is
            // the case where only the drawing lagged behind
            if let Some(viewport) = imp.recent_scroller.child() {
                viewport.queue_allocate();
            }
        });
    }

    /// Lets the list show RECENT_VISIBLE_ROWS songs at most, measured from
    /// the rows themselves (they follow the text size).
    fn fit_recent_rows(&self) {
        let imp = self.imp();
        let heights: Vec<i32> = (0..RECENT_VISIBLE_ROWS)
            .filter_map(|i| imp.recent_list.row_at_index(i))
            .map(|row| row.measure(gtk::Orientation::Vertical, -1).1)
            .collect();
        let Some(&tallest) = heights.iter().max() else {
            return;
        };
        imp.recent_scroller
            .set_max_content_height(tallest * RECENT_VISIBLE_ROWS + RECENT_LIST_BOTTOM_PADDING);
    }

    /// The scroller and rows of Recently Played, for tests.
    #[cfg(test)]
    pub fn recent_rows_for_tests(&self) -> (gtk::ScrolledWindow, Vec<gtk::ListBoxRow>) {
        let imp = self.imp();
        let mut rows = Vec::new();
        let mut i = 0;
        while let Some(row) = imp.recent_list.row_at_index(i) {
            rows.push(row);
            i += 1;
        }
        (imp.recent_scroller.get(), rows)
    }

    /// Fills the list with `rows`, oldest first (the newest ends up next to
    /// Now Playing, below); returns each row's cover image, for the caller to
    /// load.
    pub fn show_recent_rows(&self, rows: &[RecentRow]) -> Vec<gtk::Image> {
        let imp = self.imp();
        imp.recent_list.remove_all();
        let mut hearts = Vec::with_capacity(rows.len());
        let covers = rows
            .iter()
            .map(|row| {
                let (widget, cover, heart) = recent_row(row);
                let obj = self.downgrade();
                let list_row = widget.downgrade();
                heart.connect_clicked(move |_| {
                    let (Some(obj), Some(list_row)) = (obj.upgrade(), list_row.upgrade()) else {
                        return;
                    };
                    let index = list_row.index();
                    let like = obj.imp().recent_like.borrow().clone();
                    if let (Some(like), true) = (like, index >= 0) {
                        like(index as usize);
                    }
                });
                imp.recent_list.append(&widget);
                hearts.push(heart);
                cover
            })
            .collect();
        imp.recent_hearts.replace(hearts);
        self.fit_recent_rows();
        imp.recent_stack.set_visible_child_name("list");
        self.scroll_recent_to_end();
        covers
    }
}

fn recent_row(row: &RecentRow) -> (gtk::ListBoxRow, gtk::Image, gtk::Button) {
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

    // Like a song that played before without going back to it. Shown on
    // hover and kept on once liked, as in the track lists
    let heart = gtk::Button::new();
    heart.set_has_frame(false);
    heart.set_valign(gtk::Align::Center);
    heart.set_focus_on_click(false);
    for class in [
        "circular",
        "flat",
        "heart-button",
        "queue-page__recent-like",
    ] {
        heart.add_css_class(class);
    }
    show_like_state(&heart, Some(row.liked));

    let line = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    line.append(&cover);
    line.append(&texts);
    line.append(&heart);

    let widget = gtk::ListBoxRow::new();
    widget.set_child(Some(&line));
    widget.set_tooltip_text(Some(&format!("{} · {}", row.title, row.subtitle)));
    widget.add_css_class("queue-page__recent-row");
    (widget, cover, heart)
}
