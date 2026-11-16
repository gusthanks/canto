use gettextrs::gettext;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, CompositeTemplate};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

use crate::app::components::labels;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/dev/diegovsky/Riff/components/utility_panel.ui")]
    pub struct UtilityPanelWidget {
        #[template_child]
        pub resize_handle: TemplateChild<gtk::Box>,
        #[template_child]
        pub header: TemplateChild<libadwaita::HeaderBar>,
        #[template_child]
        pub now_tab: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub queue_tab: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub lyrics_tab: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub selection_header: TemplateChild<libadwaita::HeaderBar>,
        #[template_child]
        pub selection_title: TemplateChild<libadwaita::WindowTitle>,
        #[template_child]
        pub selection_cancel: TemplateChild<gtk::Button>,
        #[template_child]
        pub select_all: TemplateChild<gtk::Button>,
        #[template_child]
        pub panel_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub now_content: TemplateChild<libadwaita::Bin>,
        #[template_child]
        pub queue_content: TemplateChild<libadwaita::Bin>,
        #[template_child]
        pub lyrics_content: TemplateChild<libadwaita::Bin>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for UtilityPanelWidget {
        const NAME: &'static str = "UtilityPanelWidget";
        type Type = super::UtilityPanelWidget;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for UtilityPanelWidget {}
    impl WidgetImpl for UtilityPanelWidget {}
    impl BinImpl for UtilityPanelWidget {}
}

glib::wrapper! {
    pub struct UtilityPanelWidget(ObjectSubclass<imp::UtilityPanelWidget>)
        @extends gtk::Widget, libadwaita::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl UtilityPanelWidget {
    pub fn resize_handle(&self) -> &gtk::Box {
        &self.imp().resize_handle
    }

    fn tabs(&self) -> [(&'static str, gtk::ToggleButton); 3] {
        let imp = self.imp();
        [
            ("now", imp.now_tab.get()),
            ("queue", imp.queue_tab.get()),
            ("lyrics", imp.lyrics_tab.get()),
        ]
    }

    pub fn wire_tabs(&self) {
        // Grouped toggles: clicking the active one does nothing.
        for (name, tab) in self.tabs().iter().cloned() {
            let stack = self.imp().panel_stack.get();
            tab.connect_toggled(move |tab| {
                if tab.is_active() {
                    stack.set_visible_child_name(name);
                }
            });
        }
    }

    pub fn set_pages(
        &self,
        now: Option<&gtk::Widget>,
        queue: Option<&gtk::Widget>,
        lyrics: Option<&gtk::Widget>,
    ) {
        let imp = self.imp();
        imp.now_content.set_child(now);
        imp.queue_content.set_child(queue);
        imp.lyrics_content.set_child(lyrics);
        imp.now_tab.set_visible(now.is_some());
        imp.queue_tab.set_visible(queue.is_some());
        imp.lyrics_tab.set_visible(lyrics.is_some());
        // Off a tab that went away
        let shown = self.active_tab();
        let available = |name: &str| match name {
            "now" => now.is_some(),
            "queue" => queue.is_some(),
            _ => lyrics.is_some(),
        };
        if !available(shown) {
            if let Some((name, _)) = self.tabs().iter().find(|(name, _)| available(name)) {
                self.show_tab(name);
            }
        }
    }

    pub fn set_content(&self, content: Option<&gtk::Widget>, _title: &str) {
        self.set_pages(None, content, None);
    }

    pub fn connect_selection_cancel<F: Fn() + 'static>(&self, f: F) {
        self.imp().selection_cancel.connect_clicked(move |_| f());
    }

    pub fn connect_select_all<F: Fn() + 'static>(&self, f: F) {
        self.imp().select_all.connect_clicked(move |_| f());
    }

    pub fn set_selection(&self, selected: Option<usize>, all_selected: bool) {
        let imp = self.imp();
        imp.header.set_visible(selected.is_none());
        imp.selection_header.set_visible(selected.is_some());
        if selected.is_some() {
            imp.queue_tab.set_active(true);
            imp.panel_stack.set_visible_child_name("queue");
        }
        let Some(count) = selected else {
            return;
        };
        imp.selection_title
            .set_title(&labels::n_tracks_selected_label(count));
        imp.select_all.set_label(&if all_selected {
            // translators: Button label. Deselects all songs, once they are all selected.
            gettext("Deselect All")
        } else {
            // translators: Button label. Selects all visible songs.
            gettext("Select All")
        });
    }

    /// What the panel shows, read from the page itself (not the tab buttons).
    pub fn active_tab(&self) -> &'static str {
        match self.imp().panel_stack.visible_child_name().as_deref() {
            Some("queue") => "queue",
            Some("lyrics") => "lyrics",
            _ => "now",
        }
    }

    pub fn show_tab(&self, tab: &str) {
        if let Some((name, button)) = self.tabs().iter().find(|(name, _)| *name == tab) {
            button.set_active(true);
            self.imp().panel_stack.set_visible_child_name(name);
        }
    }
}

pub fn expose_widgets() {
    UtilityPanelWidget::static_type();
}
