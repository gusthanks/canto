//! A card list's filters as pill buttons on the page, in the style of the
//! Library's (an artist's releases: Albums, Singles, EPs, Compilations).
//! Clicking one shows only its kind; clicking it again shows everything.
//! Each shows only while a card of its kind is in the list, and the row
//! shows only when there are two kinds or more to choose between.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gtk::prelude::*;

use crate::app::models::FilterOption;

pub struct FilterChips {
    row: gtk::FlowBox,
    /// Each button with the category it filters to
    chips: Vec<(gtk::ToggleButton, String)>,
    on_select: Rc<dyn Fn(&str)>,
    /// The category shown alone, empty when everything shows
    active: RefCell<String>,
}

impl FilterChips {
    /// The buttons for `filters` (the "All" one left out: it is a click on
    /// the active button). `on_select` gets the category to show, empty for
    /// everything.
    pub fn new(filters: &[FilterOption], on_select: impl Fn(&str) + 'static) -> Rc<Self> {
        // Wraps on a narrow page rather than running past its edge
        let row = gtk::FlowBox::new();
        row.set_selection_mode(gtk::SelectionMode::None);
        row.set_homogeneous(false);
        row.set_column_spacing(6);
        row.set_row_spacing(6);
        row.set_max_children_per_line(16);
        row.set_hexpand(true);
        row.add_css_class("filter-chips");
        row.set_visible(false);
        let chips = filters
            .iter()
            .filter(|option| !option.category.is_empty())
            .map(|option| {
                let button = gtk::ToggleButton::with_label(&option.label);
                button.add_css_class("pill");
                button.add_css_class("library-filter");
                button.set_focus_on_click(false);
                button.set_visible(false);
                row.append(&button);
                // The FlowBox's own cell takes no focus: Tab goes to the button
                if let Some(cell) = button.parent() {
                    cell.set_focusable(false);
                }
                (button, option.category.clone())
            })
            .collect();
        let this = Rc::new(Self {
            row,
            chips,
            on_select: Rc::new(on_select),
            active: RefCell::new(String::new()),
        });
        for (button, category) in &this.chips {
            let weak = Rc::downgrade(&this);
            let category = category.clone();
            button.connect_clicked(move |button| {
                if let Some(this) = weak.upgrade() {
                    let chosen = if button.is_active() {
                        category.as_str()
                    } else {
                        ""
                    };
                    this.select(chosen);
                }
            });
        }
        this
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.row.upcast_ref()
    }

    /// Shows only `category` (empty: everything), with only its button on.
    fn select(&self, category: &str) {
        for (button, own) in &self.chips {
            let on = !category.is_empty() && own == category;
            if button.is_active() != on {
                button.set_active(on);
            }
        }
        if *self.active.borrow() != category {
            self.active.replace(category.to_string());
            (self.on_select)(category);
        }
    }

    /// Shows the buttons whose kind is among `present`, and the row when two
    /// or more show. A kind shown alone that left the list shows everything
    /// again.
    pub fn show_categories(&self, present: &HashSet<String>) {
        let mut shown = 0;
        for (button, category) in &self.chips {
            let here = present.contains(category);
            button.set_visible(here);
            if let Some(cell) = button.parent() {
                cell.set_visible(here);
            }
            shown += usize::from(here);
        }
        self.row.set_visible(shown >= 2);
        let active = self.active.borrow().clone();
        if !active.is_empty() && (shown < 2 || !present.contains(&active)) {
            self.select("");
        }
    }

    /// The labels of the buttons that show, and the one that is on.
    #[cfg(test)]
    pub fn shown_for_tests(&self) -> (bool, Vec<String>, Option<String>) {
        let shown = self
            .chips
            .iter()
            .filter(|(button, _)| button.is_visible())
            .map(|(button, _)| button.label().unwrap_or_default().to_string())
            .collect();
        let on = self
            .chips
            .iter()
            .find(|(button, _)| button.is_active())
            .map(|(button, _)| button.label().unwrap_or_default().to_string());
        (self.row.is_visible(), shown, on)
    }

    /// Clicks the button labeled `label`, as a pointer would (a toggle
    /// button's click flips it, then runs the handlers).
    #[cfg(test)]
    pub fn click_for_tests(&self, label: &str) {
        if let Some((button, _)) = self
            .chips
            .iter()
            .find(|(button, _)| button.label().as_deref() == Some(label))
        {
            button.emit_clicked();
        }
    }
}

/// The kinds of the cards in `store` (CardModel categories).
pub fn categories_in(store: &gio::ListStore) -> HashSet<String> {
    (0..store.n_items())
        .filter_map(|i| {
            store
                .item(i)
                .and_downcast::<crate::app::models::CardModel>()
        })
        .map(|card| card.category())
        .filter(|category| !category.is_empty())
        .collect()
}
