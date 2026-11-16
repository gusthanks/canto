use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use super::NavigationPanelItem;

/// Side of a cover in the sidebar.
pub const COVER_PX: i32 = 32;

impl NavigationPanelRow {
    pub fn new(item: NavigationPanelItem) -> Self {
        glib::Object::builder().property("item", item).build()
    }

    /// The image the cover goes into, when the item has one.
    pub fn cover_target(&self) -> Option<(gtk::Image, String)> {
        let image = self.item().image();
        (!image.is_empty()).then(|| (self.imp().icon.get(), image))
    }

    /// Icons only, for the collapsed sidebar.
    pub fn set_rail(&self, rail: bool) {
        let imp = self.imp();
        imp.title.set_visible(!rail);
        imp.content.set_halign(if rail {
            gtk::Align::Center
        } else {
            gtk::Align::Fill
        });
    }
}

mod imp {
    use super::*;
    use glib::Properties;
    use std::cell::RefCell;

    #[derive(Debug, CompositeTemplate, Properties)]
    #[template(resource = "/dev/diegovsky/Riff/navigation_panel/navigation_panel_row.ui")]
    #[properties(wrapper_type = super::NavigationPanelRow)]
    pub struct NavigationPanelRow {
        #[template_child]
        pub content: TemplateChild<gtk::Box>,

        #[template_child]
        pub icon_clip: TemplateChild<gtk::Box>,

        #[template_child]
        pub icon: TemplateChild<gtk::Image>,

        #[template_child]
        pub title: TemplateChild<gtk::Label>,

        #[property(get, set = Self::set_item)]
        pub item: RefCell<NavigationPanelItem>,
    }

    impl NavigationPanelRow {
        fn set_item(&self, item: NavigationPanelItem) {
            self.title.set_text(item.title().as_str());
            self.icon.set_icon_name(item.icon());
            if !item.image().is_empty() {
                self.icon.set_pixel_size(COVER_PX);
                self.icon_clip.add_css_class("navigation-row__cover");
                if item.round_cover() {
                    self.icon_clip.add_css_class("navigation-row__cover--round");
                }
            }
            self.obj().set_tooltip_text(Some(item.title().as_str()));
            self.item.replace(item);
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NavigationPanelRow {
        const NAME: &'static str = "NavigationPanelRow";
        type Type = super::NavigationPanelRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }

        fn new() -> Self {
            Self {
                content: Default::default(),
                icon_clip: Default::default(),
                icon: Default::default(),
                title: Default::default(),
                item: RefCell::new(glib::Object::new()),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for NavigationPanelRow {}
    impl WidgetImpl for NavigationPanelRow {}
    impl ListBoxRowImpl for NavigationPanelRow {}
}

glib::wrapper! {
    pub struct NavigationPanelRow(ObjectSubclass<imp::NavigationPanelRow>) @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}
