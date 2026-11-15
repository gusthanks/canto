// Widget for the "Now Playing" page.
// The current track in the header, then the queue, or a placeholder when
// nothing plays.

use gettextrs::gettext;
use gtk::prelude::*;
use std::rc::Rc;

use super::NowPlayingModel;
use crate::app::components::{
    Component, DetailsPageComponent, EventListener, HasHeaderBarModel, LyricsPage, PageHeader,
    QueueList,
};
use crate::app::state::PlaybackEvent;
use crate::app::AppEvent;

thread_local! {
    // The Now Playing page's queue/lyrics stack, so the player bar can drive it.
    static INLINE_STACK: std::cell::RefCell<glib::WeakRef<gtk::Stack>> =
        std::cell::RefCell::new(glib::WeakRef::new());
    // What the side panel shows while open ("queue" or "lyrics"), if open.
    static SIDE_PANEL_TAB: std::cell::Cell<Option<&'static str>> =
        const { std::cell::Cell::new(None) };
}

/// The view the page shows instead of `name`, so it never repeats the side panel.
fn other_view(name: &str) -> &'static str {
    if name == "lyrics" {
        "queue"
    } else {
        "lyrics"
    }
}

/// Shows `name` ("queue" or "lyrics") inside the Now Playing page when that
/// page is on screen, and returns `true`. Returns `false` otherwise, so the
/// caller can fall back to the side panel.
pub fn show_inline_view(name: &str) -> bool {
    INLINE_STACK.with(|cell| match cell.borrow().upgrade() {
        Some(stack) if stack.is_mapped() && stack.child_by_name(name).is_some() => {
            stack.set_visible_child_name(name);
            true
        }
        _ => false,
    })
}

/// Tells the page what the side panel shows (`None` when closed). While the
/// panel is open, the page switches away from that view, so the same lyrics
/// (or queue) never scroll in two places at once.
pub fn note_side_panel(tab: Option<&'static str>) {
    SIDE_PANEL_TAB.with(|cell| cell.set(tab));
    avoid_side_panel_duplicate();
}

fn avoid_side_panel_duplicate() {
    let Some(tab) = SIDE_PANEL_TAB.with(|cell| cell.get()) else {
        return;
    };
    INLINE_STACK.with(|cell| {
        if let Some(stack) = cell.borrow().upgrade() {
            if stack.visible_child_name().as_deref() == Some(tab) {
                stack.set_visible_child_name(other_view(tab));
            }
        }
    });
}

/// GTK widget for the now-playing/queue detail page.
pub struct NowPlaying {
    stack: gtk::Stack,
    component: DetailsPageComponent<NowPlayingModel>,
    #[allow(dead_code)]
    status_page: libadwaita::StatusPage,
}

impl NowPlaying {
    pub fn new(model: Rc<NowPlayingModel>, header: PageHeader) -> Self {
        let mut component =
            DetailsPageComponent::new(model.clone(), model.to_headerbar_model(), header);
        component
            .get_root_widget()
            .add_css_class("adaptive-cover-surface");
        let queue_list = QueueList::new(model.queue_list_model());
        let lyrics_page = LyricsPage::new(model.app_model.clone(), model.dispatcher.clone());

        // Queue or lyrics, switched by the player bar buttons (see `show_inline_view`).
        let content_stack = gtk::Stack::new();
        content_stack.set_vexpand(true);
        content_stack.add_named(queue_list.widget(), Some("queue"));
        content_stack.add_named(lyrics_page.widget(), Some("lyrics"));
        content_stack.set_visible_child_name("queue");
        INLINE_STACK.with(|cell| cell.borrow().set(Some(&content_stack)));
        // Coming back to the page with the side panel already open
        content_stack.connect_map(|_| avoid_side_panel_duplicate());

        component.append_content(&content_stack);
        // Once the list is in the page's ScrolledWindow
        queue_list.connect_scrolling();
        component.add_child(Box::new(queue_list));
        component.add_child(Box::new(lyrics_page));

        let status_page = libadwaita::StatusPage::new();
        status_page.set_title(&gettext("No Track Playing"));
        status_page.set_icon_name(Some("audio-x-generic-symbolic"));

        let stack = gtk::Stack::new();
        stack.add_named(component.get_root_widget(), Some("content"));
        stack.add_named(&status_page, Some("empty"));

        let visible = if model.has_queue() {
            "content"
        } else {
            "empty"
        };
        stack.set_visible_child_name(visible);

        Self {
            stack,
            component,
            status_page,
        }
    }

    fn update_empty_state(&self) {
        let name = if self.component.model().has_queue() {
            "content"
        } else {
            "empty"
        };
        self.stack.set_visible_child_name(name);
    }
}

impl Component for NowPlaying {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.stack.upcast_ref()
    }
    fn get_children(&mut self) -> Option<&mut Vec<Box<dyn EventListener>>> {
        self.component.get_children()
    }
}

impl EventListener for NowPlaying {
    fn on_event(&mut self, event: &AppEvent) {
        self.component.handle_event(event);
        if let AppEvent::PlaybackEvent(
            PlaybackEvent::TrackChanged(_)
            | PlaybackEvent::TrackSynced
            | PlaybackEvent::PlaybackStopped
            | PlaybackEvent::PlaylistChanged,
        ) = event
        {
            self.update_empty_state();
        }
        self.broadcast_event(event);
    }
}

#[cfg(test)]
mod tests {
    use super::other_view;

    #[test]
    fn page_shows_the_view_the_side_panel_does_not() {
        assert_eq!(other_view("lyrics"), "queue");
        assert_eq!(other_view("queue"), "lyrics");
    }
}
