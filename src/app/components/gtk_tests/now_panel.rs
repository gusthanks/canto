//! The Now Playing side panel and its About and Stats tabs.

use super::*;

gtk_tests! {
    panel_builds => assert_panel_builds,
    about_and_stats_tabs_fill_every_card => || {
        force_dark();
        assert_about_and_stats_tabs_fill_every_card();
    },
}

/// The Now Playing side panel builds, with its heart icons available.
fn assert_panel_builds() {
    let now_panel = crate::app::components::shell::now_panel::widgets_for_tests();
    drop(now_panel);
}

/// The About and Stats tabs build with every card filled.
fn assert_about_and_stats_tabs_fill_every_card() {
    let about = crate::app::components::shell::now_panel::about_sample_for_tests();
    let stats = crate::app::components::shell::now_panel::stats_sample_for_tests();
    for (tab, cards) in [(&about, 7), (&stats, 7)] {
        let stack = tab.downcast_ref::<gtk::Stack>().unwrap();
        assert_eq!(stack.visible_child_name().as_deref(), Some("content"));
        fn find(widget: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
            if widget.has_css_class(class) {
                return Some(widget.clone());
            }
            let mut child = widget.first_child();
            while let Some(c) = child {
                if let Some(found) = find(&c, class) {
                    return Some(found);
                }
                child = c.next_sibling();
            }
            None
        }
        let content = find(tab, "info-page__content").expect("info page content");
        assert_eq!(content.observe_children().n_items(), cards);
    }
}
