//! Sleep Timer controller shared between PlaybackWidget popover and Ctrl+K Command Palette.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gettextrs::gettext;

use crate::app::components::EventListener;
use crate::app::state::{PlaybackAction, PlaybackEvent};
use crate::app::{AppAction, AppEvent, AppModel, Dispatcher};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepTimerMode {
    Off,
    EndOfTrack,
    Minutes(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SleepTimerSnapshot {
    pub mode: SleepTimerMode,
    pub remaining_secs: Option<u32>,
}

impl SleepTimerSnapshot {
    pub fn is_active(&self) -> bool {
        self.mode != SleepTimerMode::Off
    }

    pub fn compact_label(&self) -> Option<String> {
        match self.mode {
            SleepTimerMode::Off => None,
            SleepTimerMode::EndOfTrack => Some(gettext("End")),
            SleepTimerMode::Minutes(_) => {
                let secs = self.remaining_secs.unwrap_or(0);
                let mins = secs / 60;
                let rem = secs % 60;
                Some(format!("{mins}:{rem:02}"))
            }
        }
    }

    pub fn tooltip(&self) -> String {
        match self.mode {
            SleepTimerMode::Off => gettext("Sleep Timer"),
            SleepTimerMode::EndOfTrack => gettext("Sleep Timer: Pause at end of current track"),
            SleepTimerMode::Minutes(_) => {
                let label = self.compact_label().unwrap_or_default();
                format!("{}: {label}", gettext("Sleep Timer"))
            }
        }
    }
}

type Observer = Rc<dyn Fn(&SleepTimerSnapshot)>;

pub struct SleepTimerController {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    mode: Cell<SleepTimerMode>,
    remaining_secs: Cell<u32>,
    armed_has_track: Cell<bool>,
    suppress_continuation: Cell<bool>,
    timer_source: RefCell<Option<glib::SourceId>>,
    observers: RefCell<Vec<Observer>>,
}

impl SleepTimerController {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Rc<Self> {
        Rc::new(Self {
            app_model,
            dispatcher,
            mode: Cell::new(SleepTimerMode::Off),
            remaining_secs: Cell::new(0),
            armed_has_track: Cell::new(false),
            suppress_continuation: Cell::new(false),
            timer_source: RefCell::new(None),
            observers: RefCell::new(Vec::new()),
        })
    }

    pub fn snapshot(&self) -> SleepTimerSnapshot {
        let mode = self.mode.get();
        let remaining_secs = match mode {
            SleepTimerMode::Minutes(_) => Some(self.remaining_secs.get()),
            _ => None,
        };
        SleepTimerSnapshot {
            mode,
            remaining_secs,
        }
    }

    pub fn subscribe<F: Fn(&SleepTimerSnapshot) + 'static>(&self, cb: F) {
        let snap = self.snapshot();
        cb(&snap);
        self.observers.borrow_mut().push(Rc::new(cb));
    }

    fn notify(&self) {
        let snap = self.snapshot();
        let observers = self.observers.borrow().clone();
        for cb in observers {
            cb(&snap);
        }
    }

    fn clear_timer(&self) {
        if let Some(id) = self.timer_source.borrow_mut().take() {
            id.remove();
        }
    }

    pub fn set_off(&self) {
        self.clear_timer();
        self.mode.set(SleepTimerMode::Off);
        self.remaining_secs.set(0);
        self.armed_has_track.set(false);
        self.suppress_continuation.set(false);
        self.notify();
    }

    pub fn set_end_of_track(&self) {
        self.clear_timer();
        let has_track = self
            .app_model
            .get_state()
            .playback
            .current_song_id()
            .is_some();
        self.mode.set(SleepTimerMode::EndOfTrack);
        self.remaining_secs.set(0);
        self.armed_has_track.set(has_track);
        self.suppress_continuation.set(false);
        self.notify();
    }

    pub fn set_minutes(self: &Rc<Self>, minutes: u32) {
        if minutes == 0 {
            self.set_off();
            return;
        }
        self.clear_timer();
        self.mode.set(SleepTimerMode::Minutes(minutes));
        self.remaining_secs.set(minutes.saturating_mul(60));
        self.armed_has_track.set(false);
        self.suppress_continuation.set(false);
        self.notify();

        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_seconds_local(1, move || {
            let Some(ctrl) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            ctrl.on_second_tick()
        });
        *self.timer_source.borrow_mut() = Some(source);
    }

    fn on_second_tick(&self) -> glib::ControlFlow {
        if !matches!(self.mode.get(), SleepTimerMode::Minutes(_)) {
            *self.timer_source.borrow_mut() = None;
            return glib::ControlFlow::Break;
        }
        let next = self.remaining_secs.get().saturating_sub(1);
        self.remaining_secs.set(next);
        if next == 0 {
            *self.timer_source.borrow_mut() = None;
            self.expire(false);
            return glib::ControlFlow::Break;
        }
        self.notify();
        glib::ControlFlow::Continue
    }

    /// Consume and return whether an `EndOfTrack` sleep timer expiry stopped the
    /// queue, so smart queue continuation does not restart playback immediately.
    pub fn take_continuation_suppression(&self) -> bool {
        self.suppress_continuation.replace(false)
    }

    fn expire(&self, suppress_continuation: bool) {
        self.clear_timer();
        self.mode.set(SleepTimerMode::Off);
        self.remaining_secs.set(0);
        self.armed_has_track.set(false);
        if suppress_continuation {
            self.suppress_continuation.set(true);
        }
        self.notify();

        if self.app_model.get_state().playback.is_playing() {
            self.dispatcher.dispatch(PlaybackAction::Pause.into());
        }
        self.dispatcher
            .dispatch(AppAction::ShowNotification(gettext(
                "Sleep timer paused playback",
            )));
    }
}

pub struct SleepTimerListener {
    controller: Rc<SleepTimerController>,
}

impl SleepTimerListener {
    pub fn new(controller: Rc<SleepTimerController>) -> Self {
        Self { controller }
    }
}

impl EventListener for SleepTimerListener {
    fn on_event(&mut self, event: &AppEvent) {
        if self.controller.mode.get() != SleepTimerMode::EndOfTrack {
            return;
        }
        match event {
            AppEvent::PlaybackEvent(
                PlaybackEvent::TrackChanged(_) | PlaybackEvent::TrackSynced,
            ) => {
                let has_track = self
                    .controller
                    .app_model
                    .get_state()
                    .playback
                    .current_song_id()
                    .is_some();
                match decide_end_of_track_transition(
                    self.controller.armed_has_track.get(),
                    has_track,
                ) {
                    EndOfTrackAction::ArmNow => self.controller.armed_has_track.set(true),
                    EndOfTrackAction::Expire => self.controller.expire(false),
                    EndOfTrackAction::Wait => {}
                }
            }
            AppEvent::PlaybackEvent(PlaybackEvent::PlaybackStopped) => {
                self.controller.expire(true);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EndOfTrackAction {
    Wait,
    ArmNow,
    Expire,
}

fn decide_end_of_track_transition(
    armed_has_track: bool,
    current_has_track: bool,
) -> EndOfTrackAction {
    if !armed_has_track {
        if current_has_track {
            EndOfTrackAction::ArmNow
        } else {
            EndOfTrackAction::Wait
        }
    } else {
        EndOfTrackAction::Expire
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_countdown_labels_and_hides_when_off() {
        let off = SleepTimerSnapshot {
            mode: SleepTimerMode::Off,
            remaining_secs: None,
        };
        assert!(!off.is_active());
        assert_eq!(off.compact_label(), None);

        let mins = SleepTimerSnapshot {
            mode: SleepTimerMode::Minutes(15),
            remaining_secs: Some(905),
        };
        assert!(mins.is_active());
        assert_eq!(mins.compact_label().as_deref(), Some("15:05"));

        let eot = SleepTimerSnapshot {
            mode: SleepTimerMode::EndOfTrack,
            remaining_secs: None,
        };
        assert!(eot.is_active());
        assert!(eot.compact_label().is_some());
    }

    #[test]
    fn end_of_track_waits_for_first_track_when_armed_idle() {
        assert_eq!(
            decide_end_of_track_transition(false, false),
            EndOfTrackAction::Wait
        );
        assert_eq!(
            decide_end_of_track_transition(false, true),
            EndOfTrackAction::ArmNow
        );
        assert_eq!(
            decide_end_of_track_transition(true, true),
            EndOfTrackAction::Expire
        );
        assert_eq!(
            decide_end_of_track_transition(true, false),
            EndOfTrackAction::Expire
        );
    }
}
