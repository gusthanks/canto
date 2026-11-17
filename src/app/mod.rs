#[cfg(debug_assertions)]
use crate::player::Command;
use crate::settings::{RiffSettings, StateTracker};
use futures::channel::mpsc::UnboundedSender;
use gio::prelude::*;
use glib::object::Cast;
use riff_auth::{SessionTokens, TokenStore};
use riff_config::feature_flags::FeatureFlag;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

pub mod dispatch;
pub use dispatch::{DispatchLoop, Dispatcher};

pub mod components;
use components::*;

pub mod models;
pub use models::SongsSource;

mod list_store;
pub use list_store::*;

pub mod state;
pub mod theme;
pub use state::{
    AppAction, AppEvent, AppModel, AppState, BrowserAction, BrowserEvent, PaginationTarget,
    ProvidesApi,
};

pub mod load;

pub mod playlist_actions;

#[cfg(debug_assertions)]
mod dev_tools;

// Where all the app logic happens
pub struct App {
    settings: RiffSettings,
    // The builder instance used to properly configure all the widgets created at startup
    builder: gtk::Builder,
    // All the "components" that will be notified of things happening throughout the app
    components: Vec<Box<dyn EventListener>>,
    // Holds the app state
    model: Rc<AppModel>,
    // Allows sending actions that are handled by the model above
    sender: UnboundedSender<AppAction>,
    // Sends commands to the librespot player. Only the dev tools menu uses it,
    // so it is compiled out of release builds entirely.
    #[cfg(debug_assertions)]
    player_command_sender: UnboundedSender<Command>,
    theme_engine: Rc<theme::ThemeEngine>,
    sleep_timer: Rc<SleepTimerController>,
    _live_settings: gio::Settings,
}

impl App {
    pub fn new(
        settings: RiffSettings,
        builder: gtk::Builder,
        sender: UnboundedSender<AppAction>,
    ) -> Self {
        let theme_engine = theme::ThemeEngine::install();
        let state = AppState::new();

        // Token
        let token_store = TokenStore::new();
        let token_provider: Arc<dyn riff_api::TokenProvider> = Arc::new(token_store.clone());
        let session_tokens = SessionTokens::new();
        let session_token_provider: Arc<dyn riff_api::TokenProvider> =
            Arc::new(session_tokens.clone());

        // Setting
        let gsettings = gio::Settings::new(crate::settings::SETTINGS);
        let level_meter = riff_audio_engine::LevelMeterController::new(
            gsettings.boolean(riff_config::settings::VISUAL_BRIDGE_ENABLED),
        );

        // API
        let memory_cache_mb = gsettings.uint("memory-cache-size-mb").max(1) as usize;
        let disk_cache_mb = gsettings.uint("disk-cache-size-mb").max(1) as usize;
        let api_service = Arc::new(riff_api::spotify_service(
            token_provider,
            session_token_provider,
            memory_cache_mb * 1024 * 1024,
            disk_cache_mb * 1024 * 1024,
        ));

        let live_settings =
            App::watch_live_settings(gsettings, Arc::clone(&api_service), sender.clone());
        let model = Rc::new(AppModel::new(state, api_service));

        let player_command_sender = crate::player::start_player_service(
            settings.player_settings.clone(),
            sender.clone(),
            token_store,
            session_tokens,
            level_meter.clone(),
        );

        let api = model.api();
        let connect_command_sender = crate::connect::start_connect_server(api, sender.clone());
        let sleep_timer =
            SleepTimerController::new(Rc::clone(&model), Dispatcher::new(sender.clone()));

        // Non widget components
        let components: Vec<Box<dyn EventListener>> = vec![
            Box::new(PlayerNotifier::new(
                Rc::clone(&model),
                Dispatcher::new(sender.clone()),
                player_command_sender.clone(),
                connect_command_sender,
            )),
            Box::new(QueueLoader::new(
                Rc::clone(&model),
                Dispatcher::new(sender.clone()),
            )),
            Box::new(StateTracker::new_from_gsettings()),
            Box::new(SleepTimerListener::new(Rc::clone(&sleep_timer))),
            Box::new(ScrobblerController::new(Rc::clone(&model))),
            Box::new(SmartQueueController::new(
                Rc::clone(&model),
                Dispatcher::new(sender.clone()),
                Rc::clone(&sleep_timer),
            )),
            Box::new(VisualBridge::new(
                Rc::clone(&model),
                Rc::clone(&theme_engine),
                level_meter,
            )),
            App::make_dbus(Rc::clone(&model), sender.clone()),
            App::make_inhibitor(&builder, Rc::clone(&model)),
        ];

        Self {
            settings,
            builder,
            components,
            model,
            sender,
            #[cfg(debug_assertions)]
            player_command_sender,
            theme_engine,
            sleep_timer,
            _live_settings: live_settings,
        }
    }

    fn watch_live_settings(
        settings: gio::Settings,
        api: Arc<riff_api::ApiService>,
        sender: UnboundedSender<AppAction>,
    ) -> gio::Settings {
        let dispatcher = Dispatcher::new(sender);
        settings.connect_changed(Some(FeatureFlag::PinnedObjects.key()), move |_, _| {
            dispatcher.dispatch(BrowserAction::NotifyPinnedPlaylistsUpdated.into());
        });
        let memory_api = Arc::clone(&api);
        settings.connect_changed(Some("memory-cache-size-mb"), move |settings, key| {
            let mb = settings.uint(key).max(1) as usize;
            memory_api.set_memory_cache_bytes(mb * 1024 * 1024);
        });
        settings.connect_changed(Some("disk-cache-size-mb"), move |settings, key| {
            let mb = settings.uint(key).max(1) as usize;
            let api = Arc::clone(&api);
            tokio::spawn(async move { api.set_disk_cache_bytes(mb * 1024 * 1024).await });
        });
        settings
    }

    fn add_ui_components(&mut self) {
        // Most components will need some or all of these to work
        // ie some way to retrieve widgets
        let builder = &self.builder;
        // ...some way to read the app state
        let model = &self.model;
        // ...some (basic) way to send actions that will change the app state
        let sender = &self.sender;
        // ...ALSO some way to send actions, but more conveniently
        let dispatcher = Dispatcher::new(sender.clone());

        // Send gsettings updates for saved settings like repeat mode, shuffle, etc.
        // has to be done after the UI loads, otherwise visual glitches occour.
        for action in self.settings.player_settings.actions() {
            sender.unbounded_send(action).unwrap();
        }

        // The app-wide header bar and the registry screens contribute to it.
        let app_header: AppHeaderBar = builder.object("app_header").unwrap();
        let header_models: HeaderModelRegistry = Rc::new(RefCell::new(HashMap::new()));
        let registrar = HeaderRegistrar::new(app_header.clone(), Rc::clone(&header_models));

        let layout = WindowLayout::new(builder);
        let main_window: libadwaita::ApplicationWindow = builder.object("window").unwrap();
        install_space_toggle(&main_window);

        // All components that will be available initially
        let mut components: Vec<Box<dyn EventListener>> = vec![
            App::make_window(&self.settings, builder, Rc::clone(model)),
            App::make_selection_toolbar(builder, Rc::clone(model), dispatcher.clone()),
            App::make_playback(
                builder,
                Rc::clone(model),
                dispatcher.clone(),
                Rc::clone(&self.sleep_timer),
            ),
            App::make_panel_manager(
                builder,
                layout,
                app_header.clone(),
                Rc::clone(model),
                dispatcher.clone(),
            ),
            App::make_login(builder, dispatcher.clone()),
            App::make_navigation(builder, Rc::clone(model), dispatcher.clone(), registrar),
            // After navigation, so a pushed screen registers before the header
            // refreshes.
            App::make_app_header(
                app_header,
                Rc::clone(model),
                dispatcher.clone(),
                header_models,
            ),
            App::make_search_button(builder, dispatcher.clone()),
            App::make_clipboard_import(builder, Rc::clone(model), dispatcher.clone()),
            App::make_user_menu(builder, Rc::clone(model), dispatcher.clone()),
            App::make_notification(builder),
            Box::new(theme::ThemeController::new(
                Rc::clone(model),
                Rc::clone(&self.theme_engine),
            )),
        ];

        let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();
        let palette_action = gio::SimpleAction::new("command-palette", None);
        let palette_win = window.clone();
        let palette_model = Rc::clone(model);
        let palette_disp = dispatcher.clone();
        let palette_timer = Rc::clone(&self.sleep_timer);
        palette_action.connect_activate(move |_, _| {
            CommandPaletteDialog::open(
                &palette_win,
                Rc::clone(&palette_model),
                palette_disp.clone(),
                Rc::clone(&palette_timer),
            );
        });
        window.add_action(&palette_action);

        let stats_action = gio::SimpleAction::new("listening-stats", None);
        let stats_win = window.clone();
        let stats_model = Rc::clone(model);
        let stats_disp = dispatcher.clone();
        stats_action.connect_activate(move |_, _| {
            crate::app::components::stats_dialog::open(
                &stats_win,
                Rc::clone(&stats_model),
                stats_disp.clone(),
            );
        });
        window.add_action(&stats_action);

        let radio_action = gio::SimpleAction::new("start-track-radio", None);
        let radio_model = Rc::clone(model);
        let radio_disp = dispatcher;
        radio_action.connect_activate(move |_, _| {
            if let Some(track) = radio_model.get_state().playback.current_song() {
                start_track_radio(&radio_model, &radio_disp, track);
            } else {
                radio_disp.dispatch(AppAction::ShowNotification(gettextrs::gettext(
                    "No track is currently playing",
                )));
            }
        });
        window.add_action(&radio_action);

        // Wire up dev tools menu (debug builds only)
        #[cfg(debug_assertions)]
        dev_tools::wire_dev_tools(builder, sender, &self.player_command_sender, model);

        self.components.append(&mut components);
    }

    // A component to handle anything DBUS related
    fn make_dbus(
        app_model: Rc<AppModel>,
        sender: UnboundedSender<AppAction>,
    ) -> Box<impl EventListener> {
        Box::new(crate::dbus::start_dbus_server(app_model, sender))
    }

    fn make_window(
        settings: &RiffSettings,
        builder: &gtk::Builder,
        app_model: Rc<AppModel>,
    ) -> Box<impl EventListener> {
        let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();
        Box::new(MainWindow::new(settings.window.clone(), app_model, window))
    }

    // The app-wide header bar shared by every content screen.
    fn make_app_header(
        widget: AppHeaderBar,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
        models: HeaderModelRegistry,
    ) -> Box<AppHeaderBarComponent> {
        Box::new(AppHeaderBarComponent::new(
            widget, app_model, dispatcher, models,
        ))
    }

    fn make_inhibitor(builder: &gtk::Builder, app_model: Rc<AppModel>) -> Box<impl EventListener> {
        let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();
        Box::new(crate::inhibitor::SuspendInhibitor::new(window, app_model))
    }

    fn make_navigation(
        builder: &gtk::Builder,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
        registrar: HeaderRegistrar,
    ) -> Box<Navigation> {
        let split_view: libadwaita::OverlaySplitView = builder.object("navigation_split").unwrap();
        let content_stack: gtk::Stack = builder.object("content_stack").unwrap();
        let home_listbox: gtk::ListBox = builder.object("home_listbox").unwrap();
        let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();

        let model = NavigationModel::new(Rc::clone(&app_model), dispatcher.clone());
        // This is where components that are not created initially will be assembled
        let screen_factory =
            ScreenFactory::new(Rc::clone(&app_model), dispatcher.clone(), registrar);
        Box::new(Navigation::new(
            model,
            split_view,
            content_stack,
            home_listbox,
            screen_factory,
            window,
        ))
    }

    fn make_login(builder: &gtk::Builder, dispatcher: Dispatcher) -> Box<Login> {
        let parent: gtk::Window = builder.object("window").unwrap();
        let model = LoginModel::new(dispatcher);
        Box::new(Login::new(parent, model))
    }

    fn make_selection_toolbar(
        builder: &gtk::Builder,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
    ) -> Box<impl EventListener> {
        Box::new(SelectionToolbar::new(
            SelectionToolbarModel::new(app_model, dispatcher),
            builder.object("selection_toolbar").unwrap(),
        ))
    }

    fn make_playback(
        builder: &gtk::Builder,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
        sleep_timer: Rc<SleepTimerController>,
    ) -> Box<impl EventListener> {
        let model = PlaybackModel::new(app_model, dispatcher);
        Box::new(PlaybackControl::new(
            model,
            builder.object("playback").unwrap(),
            sleep_timer,
        ))
    }

    fn make_panel_manager(
        builder: &gtk::Builder,
        layout: Rc<WindowLayout>,
        app_header: AppHeaderBar,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
    ) -> Box<PanelManager> {
        let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();
        let panel = UtilityPanel::new(
            builder.object("utility_panel").unwrap(),
            builder.object("utility_split").unwrap(),
            layout,
        );
        let sheet = BottomSheet::new(builder.object("bottom_sheet").unwrap());
        Box::new(PanelManager::new(
            &window, panel, sheet, app_header, app_model, dispatcher,
        ))
    }

    fn make_search_button(builder: &gtk::Builder, dispatcher: Dispatcher) -> Box<SearchButton> {
        let search_button: gtk::Button = builder.object("search_button").unwrap();
        let model = SearchBarModel(dispatcher);
        Box::new(SearchButton::new(model, search_button))
    }

    fn make_user_menu(
        builder: &gtk::Builder,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
    ) -> Box<UserMenu> {
        let parent: libadwaita::ApplicationWindow = builder.object("window").unwrap();
        let settings_model = SettingsModel::new(app_model.clone(), dispatcher.clone());
        let settings = Settings::new(parent.clone().upcast(), settings_model);

        let button: gtk::MenuButton = builder.object("user").unwrap();
        let main_menu: gio::Menu = builder.object("main_menu").unwrap();
        let about: libadwaita::AboutDialog = builder.object("about").unwrap();
        let shortcuts_dialog = make_shortcuts_dialog();
        let model = UserMenuModel::new(app_model, dispatcher);
        let user_menu = UserMenu::new(
            button,
            main_menu,
            settings,
            about,
            shortcuts_dialog,
            parent,
            model,
        );
        Box::new(user_menu)
    }

    fn make_notification(builder: &gtk::Builder) -> Box<Notification> {
        let toast_overlay: libadwaita::ToastOverlay = builder.object("toast_overlay").unwrap();
        Box::new(Notification::new(toast_overlay))
    }

    fn make_clipboard_import(
        builder: &gtk::Builder,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
    ) -> Box<ClipboardImport> {
        let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();
        Box::new(ClipboardImport::new(window, dispatcher, app_model))
    }

    // Main handler called in a loop
    fn handle(&mut self, action: AppAction) {
        let starting = matches!(&action, &AppAction::Start);

        // Update the state based on an incoming action
        // and obtain events representing what that mutation entailed...
        let events = self.model.update_state(action);

        // (AppAction::Start is special and is used to setup the initial components)
        if !events.is_empty() && starting {
            self.add_ui_components();
        }

        // ...and notify every component that we know.
        // They'll be responsible for passing down these events, if they feel like it.
        for event in events.iter() {
            info!("Event: {event:?}");
            for component in self.components.iter_mut() {
                component.on_event(event);
            }
        }
    }

    // Here is the loop
    pub async fn attach(mut self, dispatch_loop: DispatchLoop) {
        // Runtime and guard both leak: GTK callbacks spawn onto this for the
        // life of the process, and a dropped guard makes a later spawn panic.
        let rt = Box::leak(Box::new(
            tokio::runtime::Runtime::new().expect("Failed to acquire tokio runtime"),
        ));
        std::mem::forget(rt.enter());

        // Evict any disk cache left over-budget by a previous session. Run
        // here (rather than at construction in `App::new`) because this is
        // where the app's Tokio runtime is entered; the data layer's async
        // disk I/O requires one.
        let maintenance = self.model.api();
        rt.spawn(async move {
            maintenance.run_cache_maintenance().await;
        });

        let app = &mut self;
        dispatch_loop
            .attach(move |action| {
                info!("Action: {action:?}");
                app.handle(action);
            })
            .await;
    }
}

/// Space plays and pauses from anywhere in the window, like in Spotify: it's
/// caught before the focused button or list row can take it as a click.
/// Text fields keep it for typing.
fn install_space_toggle(window: &libadwaita::ApplicationWindow) {
    use gtk::prelude::{EventControllerExt, GtkWindowExt, WidgetExt};
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = window.downgrade();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let Some(window) = weak.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let plain = !modifiers.intersects(
            gtk::gdk::ModifierType::CONTROL_MASK
                | gtk::gdk::ModifierType::ALT_MASK
                | gtk::gdk::ModifierType::SUPER_MASK
                | gtk::gdk::ModifierType::SHIFT_MASK,
        );
        if key != gtk::gdk::Key::space || !plain || space_is_typing(window.focus()) {
            return glib::Propagation::Proceed;
        }
        let _ = WidgetExt::activate_action(&window, "app.toggle_playback", None);
        glib::Propagation::Stop
    });
    window.add_controller(keys);
}

/// Whether a space typed now goes into a text field.
pub(crate) fn space_is_typing(focus: Option<gtk::Widget>) -> bool {
    use gtk::prelude::WidgetExt;
    let mut widget = focus;
    while let Some(w) = widget {
        if w.is::<gtk::Text>() || w.is::<gtk::TextView>() || w.is::<gtk::Editable>() {
            return true;
        }
        widget = w.parent();
    }
    false
}
