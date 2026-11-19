//! What the About and Stats tabs share: a page of cards like the Now
//! Playing panel's, with a short message instead when there's nothing to
//! show and a spinner while it loads.

use gettextrs::gettext;
use gtk::prelude::*;
use std::rc::Rc;

use super::{folded_bio, wrapping_label};

/// Runs a search in the app (the Search page, with `query` typed in).
pub(super) type OnSearch = Rc<dyn Fn(String)>;

pub(super) struct InfoPage {
    pub stack: gtk::Stack,
    pub content: gtk::Box,
    scroller: gtk::ScrolledWindow,
    status: libadwaita::StatusPage,
}

impl InfoPage {
    pub fn new(class: &str) -> Self {
        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.add_css_class("now-panel");
        stack.add_css_class(class);

        let status = libadwaita::StatusPage::new();
        status.add_css_class("compact");
        stack.add_named(&status, Some("status"));

        let spinner = libadwaita::Spinner::new();
        spinner.set_size_request(32, 32);
        spinner.set_halign(gtk::Align::Center);
        spinner.set_valign(gtk::Align::Center);
        stack.add_named(&spinner, Some("loading"));

        let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
        content.set_margin_top(8);
        content.set_margin_bottom(20);
        content.set_margin_start(16);
        content.set_margin_end(16);
        content.add_css_class("info-page__content");
        let clamp = libadwaita::Clamp::new();
        clamp.set_maximum_size(560);
        clamp.set_child(Some(&content));
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&clamp));
        stack.add_named(&scroller, Some("content"));

        let page = Self {
            stack,
            content,
            scroller,
            status,
        };
        page.show_nothing_playing();
        page
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.stack.upcast_ref()
    }

    pub fn is_shown(&self) -> bool {
        self.stack.is_mapped()
    }

    /// A short message in place of the cards, with an optional button.
    pub fn show_status(
        &self,
        icon: &str,
        title: &str,
        description: &str,
        button: Option<gtk::Button>,
    ) {
        self.status.set_icon_name(Some(icon));
        self.status.set_title(title);
        self.status
            .set_description(Some(description).filter(|d| !d.is_empty()));
        self.status.set_child(button.as_ref());
        self.stack.set_visible_child_name("status");
    }

    pub fn show_nothing_playing(&self) {
        self.show_status(
            "audio-x-generic-symbolic",
            &gettext("Nothing Playing"),
            &gettext("Play something and it shows up here."),
            None,
        );
    }

    pub fn show_loading(&self) {
        self.stack.set_visible_child_name("loading");
    }

    /// Empties the page for new cards and shows it from the top.
    pub fn clear(&self) {
        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }
        self.scroller.vadjustment().set_value(0.0);
        self.stack.set_visible_child_name("content");
    }
}

/// A button for a status message, leading to the fix.
pub(super) fn status_button(label: &str, on_click: impl Fn(&gtk::Button) + 'static) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("pill");
    button.set_halign(gtk::Align::Center);
    button.connect_clicked(on_click);
    button
}

/// Opens Preferences on the page where Genius and Last.fm are set up.
pub(super) fn setup_button(label: &str) -> gtk::Button {
    status_button(label, |button| {
        let _ = button.activate_action("win.preferences-integrations", None);
    })
}

/// A small rounded button, sized like the tag chips.
pub(super) fn pill_button(label: &str, on_click: impl Fn(&gtk::Button) + 'static) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("info-pill");
    button.set_halign(gtk::Align::Start);
    button.connect_clicked(on_click);
    button
}

/// A small rounded button that opens `url` in the browser.
pub(super) fn link_button(label: &str, url: String) -> gtk::Button {
    let button = pill_button(label, move |button| {
        crate::app::components::open_external_url(button, &url)
    });
    button.set_tooltip_text(Some(&gettext("Opens in your browser")));
    button
}

/// What something is, small and dim, over its value (like the Credits rows).
pub(super) fn row(name: &str, value: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let name_label = wrapping_label(&["caption", "dim-label"]);
    name_label.set_text(name);
    let value_label = wrapping_label(&["body"]);
    value_label.set_text(value);
    row.append(&name_label);
    row.append(&value_label);
    row
}

/// A small dim line of text.
pub(super) fn caption(text: &str) -> gtk::Label {
    let label = wrapping_label(&["caption", "dim-label"]);
    label.set_text(text);
    label
}

/// Widgets laid out as words, wrapping onto more lines as needed. A wrap
/// box, not a flow box: a flow box lines its children up in columns, which
/// left gaps after short chips.
pub(super) fn flow(children: Vec<gtk::Widget>) -> libadwaita::WrapBox {
    let flow = libadwaita::WrapBox::new();
    flow.set_child_spacing(6);
    flow.set_line_spacing(6);
    flow.set_hexpand(true);
    flow.add_css_class("info-flow");
    for child in children {
        flow.append(&child);
    }
    flow
}

/// Tags as small rounded labels.
pub(super) fn chips(tags: &[String]) -> libadwaita::WrapBox {
    flow(
        tags.iter()
            .map(|tag| {
                let chip = gtk::Label::new(Some(tag));
                chip.add_css_class("info-chip");
                chip.upcast()
            })
            .collect(),
    )
}

/// A long text (Pango markup, paragraphs split by a blank line), folded
/// to its first paragraphs with "Show More".
pub(super) fn folding_text(full: String) -> gtk::Box {
    let block = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let folded = folded_bio(&full);
    let text = wrapping_label(&["body"]);
    text.set_markup(folded.as_deref().unwrap_or(&full));
    block.append(&text);
    if let Some(folded) = folded {
        let more = gtk::Button::with_label(&gettext("Show More"));
        more.add_css_class("flat");
        more.add_css_class("small");
        more.set_halign(gtk::Align::Start);
        let expanded = std::cell::Cell::new(false);
        more.connect_clicked(move |button| {
            let open = !expanded.get();
            expanded.set(open);
            text.set_markup(if open { &full } else { &folded });
            button.set_label(&if open {
                gettext("Show Less")
            } else {
                gettext("Show More")
            });
        });
        block.append(&more);
    }
    block
}

/// A song or an artist to look up in the app: a flat row with its name,
/// a dim line under it, and an optional dim number at the end.
pub(super) fn search_row(
    rank: Option<usize>,
    name: &str,
    detail: Option<&str>,
    trailing: Option<&str>,
    query: String,
    on_search: &OnSearch,
) -> gtk::Button {
    let on_search = Rc::clone(on_search);
    // translators: Tooltip of a song or artist in the About and Stats tabs.
    action_row(
        rank,
        name,
        detail,
        trailing,
        &gettext("Search in Canto"),
        move || on_search(query.clone()),
    )
}

/// Same row as `search_row`, doing something else on click.
pub(super) fn action_row(
    rank: Option<usize>,
    name: &str,
    detail: Option<&str>,
    trailing: Option<&str>,
    tooltip: &str,
    on_click: impl Fn() + 'static,
) -> gtk::Button {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    if let Some(rank) = rank {
        let number = gtk::Label::new(Some(&rank.to_string()));
        number.add_css_class("dim-label");
        number.add_css_class("numeric");
        number.set_width_chars(2);
        number.set_xalign(1.0);
        line.append(&number);
    }
    let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let title = gtk::Label::new(Some(name));
    title.set_halign(gtk::Align::Start);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&title);
    if let Some(detail) = detail.filter(|d| !d.is_empty()) {
        let sub = gtk::Label::new(Some(detail));
        sub.set_halign(gtk::Align::Start);
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        sub.add_css_class("dim-label");
        sub.add_css_class("caption");
        text.append(&sub);
    }
    line.append(&text);
    if let Some(trailing) = trailing {
        let end = gtk::Label::new(Some(trailing));
        end.add_css_class("dim-label");
        end.add_css_class("caption");
        end.add_css_class("numeric");
        line.append(&end);
    }
    let button = gtk::Button::new();
    button.set_child(Some(&line));
    button.add_css_class("flat");
    button.add_css_class("info-row");
    button.set_tooltip_text(Some(tooltip));
    button.connect_clicked(move |_| on_click());
    button
}
