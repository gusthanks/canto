//! Full-screen modal cover artwork lightbox triggered from `DetailsHeader`.

use std::fmt;
use std::sync::Arc;

use gettextrs::gettext;
use gtk::prelude::*;
use gtk::{gdk, glib, graphene};

use super::tint;
use crate::app::load;

/// Data needed to open the fullscreen artwork viewer for a detail header.
#[derive(Clone)]
pub struct ArtworkViewerSource {
    pub url: String,
    pub full_url: String,
    pub title: String,
    pub subtitle: String,
    pub tint: Option<String>,
    pub circular: bool,
    pub preview: Option<gdk::Paintable>,
    pub api_service: Arc<canto_api::ApiService>,
}

impl fmt::Debug for ArtworkViewerSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArtworkViewerSource")
            .field("url", &self.url)
            .field("full_url", &self.full_url)
            .field("title", &self.title)
            .field("circular", &self.circular)
            .finish()
    }
}

/// Computes a square viewport edge length with comfortable margins around the
/// cover artwork inside `parent` (or sensible fallback dimensions).
fn artwork_edge_size(parent: Option<&gtk::Window>) -> i32 {
    parent
        .map(|w| (w.width().min(w.height()) - 180).clamp(320, 760))
        .unwrap_or(560)
}

/// Builds and presents an undecorated fullscreen lightbox showing `source`.
pub fn present_fullscreen_artwork(
    parent: Option<&gtk::Window>,
    source: ArtworkViewerSource,
) -> gtk::Window {
    let win = gtk::Window::builder()
        .decorated(false)
        .modal(true)
        .title(&gettext("Artwork"))
        .css_classes(["artwork-viewer-window"])
        .build();

    if let Some(parent) = parent {
        win.set_transient_for(Some(parent));
        win.fullscreen();
    } else {
        win.set_default_size(900, 720);
    }

    if let Some(digits) = source.tint.as_deref().and_then(tint::hex_digits) {
        if let Some(display) = gdk::Display::default() {
            let class_name = format!("artwork-viewer-tint-{digits}");
            let css = format!(
                ".{class_name} {{ background: radial-gradient(circle at 50% 42%, alpha(#{digits}, 0.42), rgba(10, 11, 16, 0.96) 72%); }}"
            );
            let provider = gtk::CssProvider::new();
            provider.load_from_string(&css);
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
            win.add_css_class(&class_name);
            let display_ref = display.clone();
            let provider_ref = provider.clone();
            win.connect_destroy(move |_| {
                gtk::style_context_remove_provider_for_display(&display_ref, &provider_ref);
            });
        }
    }

    let overlay = gtk::Overlay::new();
    overlay.set_hexpand(true);
    overlay.set_vexpand(true);

    let center_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .margin_top(32)
        .margin_bottom(32)
        .margin_start(32)
        .margin_end(32)
        .build();

    let edge = artwork_edge_size(parent);
    let frame = gtk::Box::builder()
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .overflow(gtk::Overflow::Hidden)
        .width_request(edge)
        .height_request(edge)
        .css_classes(["artwork-viewer__frame"])
        .build();
    if source.circular {
        frame.add_css_class("artwork-viewer__frame--circular");
    }

    let picture = gtk::Picture::builder()
        .can_shrink(true)
        .content_fit(gtk::ContentFit::Contain)
        .hexpand(true)
        .vexpand(true)
        .build();
    picture.set_paintable(source.preview.as_ref());
    frame.append(&picture);
    center_box.append(&frame);

    if !source.title.is_empty() {
        let meta = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .halign(gtk::Align::Center)
            .build();
        let title = gtk::Label::builder()
            .label(&source.title)
            .wrap(true)
            .justify(gtk::Justification::Center)
            .max_width_chars(48)
            .css_classes(["artwork-viewer__title"])
            .build();
        meta.append(&title);
        if !source.subtitle.is_empty() {
            let subtitle = gtk::Label::builder()
                .label(&source.subtitle)
                .wrap(true)
                .justify(gtk::Justification::Center)
                .max_width_chars(56)
                .css_classes(["artwork-viewer__subtitle"])
                .build();
            meta.append(&subtitle);
        }
        center_box.append(&meta);
    }

    overlay.set_child(Some(&center_box));

    let close_button = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .halign(gtk::Align::End)
        .valign(gtk::Align::Start)
        .margin_top(20)
        .margin_end(20)
        .tooltip_text(&gettext("Close"))
        .css_classes(["circular", "osd", "artwork-viewer__close"])
        .build();
    let win_weak = win.downgrade();
    close_button.connect_clicked(move |_| {
        if let Some(w) = win_weak.upgrade() {
            w.close();
        }
    });
    overlay.add_overlay(&close_button);

    let key_ctrl = gtk::EventControllerKey::new();
    let win_weak = win.downgrade();
    key_ctrl.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            if let Some(w) = win_weak.upgrade() {
                w.close();
            }
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    win.add_controller(key_ctrl);

    let backdrop_click = gtk::GestureClick::new();
    backdrop_click.set_button(gdk::BUTTON_PRIMARY);
    let win_weak = win.downgrade();
    let frame_weak = frame.downgrade();
    let overlay_weak = overlay.downgrade();
    backdrop_click.connect_released(move |_, _, x, y| {
        let (Some(win), Some(frame), Some(overlay)) = (
            win_weak.upgrade(),
            frame_weak.upgrade(),
            overlay_weak.upgrade(),
        ) else {
            return;
        };
        let inside = frame
            .compute_bounds(&overlay)
            .is_some_and(|bounds| bounds.contains_point(&graphene::Point::new(x as f32, y as f32)));
        if !inside {
            win.close();
        }
    });
    overlay.add_controller(backdrop_click);

    win.set_child(Some(&overlay));
    win.present();

    let pic_weak = picture.downgrade();
    let full_url = source.full_url;
    let api_service = source.api_service;
    glib::MainContext::default().spawn_local(async move {
        let texture = api_service
            .load_image(&full_url, 1024, 1024, load::hero())
            .await;
        if let (Some(pic), Some(texture)) = (pic_weak.upgrade(), texture) {
            pic.set_paintable(Some(&texture));
        }
    });

    win
}
