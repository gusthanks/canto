use std::collections::HashMap;
use std::convert::{Into, TryFrom};
use std::time::Instant;
use zbus::zvariant::Type;
use zbus::zvariant::{Dict, Signature, Str, Value};

fn boxed_value<'a, V: Into<Value<'a>>>(v: V) -> Value<'a> {
    Value::new(v.into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopStatus {
    None,
    Track,
    Playlist,
}

impl Type for LoopStatus {
    const SIGNATURE: &'static Signature = Str::SIGNATURE;
}

// TryFrom, not From: zbus turns a property setter's conversion error into a
// D-Bus error, and with an infallible From that error path was unreachable code.
impl TryFrom<Value<'_>> for LoopStatus {
    type Error = zbus::zvariant::Error;

    fn try_from(value: Value<'_>) -> Result<Self, Self::Error> {
        String::try_from(value).map(|s| match s.as_str() {
            "Track" => LoopStatus::Track,
            "Playlist" => LoopStatus::Playlist,
            _ => LoopStatus::None,
        })
    }
}

impl From<LoopStatus> for Value<'_> {
    fn from(status: LoopStatus) -> Self {
        match status {
            LoopStatus::None => "None".into(),
            LoopStatus::Track => "Track".into(),
            LoopStatus::Playlist => "Playlist".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

impl Type for PlaybackStatus {
    const SIGNATURE: &'static Signature = Str::SIGNATURE;
}

impl From<PlaybackStatus> for Value<'_> {
    fn from(status: PlaybackStatus) -> Self {
        match status {
            PlaybackStatus::Playing => "Playing".into(),
            PlaybackStatus::Paused => "Paused".into(),
            PlaybackStatus::Stopped => "Stopped".into(),
        }
    }
}

struct PositionMicros {
    last_known_position: u128,
    last_resume_instant: Option<Instant>,
    rate: f32,
}

impl PositionMicros {
    fn new(rate: f32) -> Self {
        Self {
            last_known_position: 0,
            last_resume_instant: None,
            rate,
        }
    }

    fn current(&self) -> u128 {
        let current_progress = self.last_resume_instant.map(|ri| {
            let elapsed = ri.elapsed().as_micros() as f32;
            let real_elapsed = self.rate * elapsed;
            real_elapsed.ceil() as u128
        });
        self.last_known_position + current_progress.unwrap_or(0)
    }

    fn set(&mut self, position: u128, playing: bool) {
        self.last_known_position = position;
        self.last_resume_instant = if playing { Some(Instant::now()) } else { None }
    }

    fn pause(&mut self) {
        self.last_known_position = self.current();
        self.last_resume_instant = None;
    }

    fn resume(&mut self) {
        self.last_resume_instant = Some(Instant::now());
    }
}

#[derive(Debug, Clone)]
pub struct TrackMetadata {
    pub id: String,
    pub length: u64,
    pub artist: Vec<String>,
    pub album: String,
    pub title: String,
    pub art: Option<String>,
}

impl Type for TrackMetadata {
    const SIGNATURE: &'static Signature = <HashMap<&str, Value<'_>>>::SIGNATURE;
}

impl From<TrackMetadata> for Value<'_> {
    fn from(meta: TrackMetadata) -> Self {
        let mut d = Dict::new(Str::SIGNATURE, Value::SIGNATURE);
        d.append("mpris:trackid".into(), boxed_value(meta.id))
            .unwrap();
        d.append("mpris:length".into(), boxed_value(meta.length))
            .unwrap();
        d.append("xesam:title".into(), boxed_value(meta.title))
            .unwrap();
        d.append("xesam:artist".into(), boxed_value(meta.artist.clone()))
            .unwrap();
        d.append("xesam:albumArtist".into(), boxed_value(meta.artist))
            .unwrap();
        d.append("xesam:album".into(), boxed_value(meta.album))
            .unwrap();
        if let Some(art) = meta.art {
            d.append("mpris:artUrl".into(), boxed_value(art)).unwrap();
        }
        Value::Dict(d)
    }
}

pub struct MprisState {
    status: PlaybackStatus,
    loop_status: LoopStatus,
    volume: f64,
    shuffled: bool,
    position: PositionMicros,
    metadata: Option<TrackMetadata>,
    has_prev: bool,
    has_next: bool,
}

impl MprisState {
    pub fn new() -> Self {
        Self {
            status: PlaybackStatus::Stopped,
            loop_status: LoopStatus::None,
            shuffled: false,
            position: PositionMicros::new(1.0),
            metadata: None,
            has_prev: false,
            has_next: false,
            volume: 1f64,
        }
    }

    pub fn volume(&self) -> f64 {
        self.volume
    }

    pub fn set_volume(&mut self, volume: f64) {
        self.volume = volume;
    }

    pub fn status(&self) -> PlaybackStatus {
        self.status
    }

    pub fn loop_status(&self) -> LoopStatus {
        self.loop_status
    }

    pub fn is_shuffled(&self) -> bool {
        self.shuffled
    }

    pub fn current_track(&self) -> Option<&TrackMetadata> {
        self.metadata.as_ref()
    }

    pub fn has_prev(&self) -> bool {
        self.has_prev
    }

    pub fn has_next(&self) -> bool {
        self.has_next
    }

    pub fn set_has_prev(&mut self, has_prev: bool) {
        self.has_prev = has_prev;
    }

    pub fn set_has_next(&mut self, has_next: bool) {
        self.has_next = has_next;
    }

    pub fn set_current_track(&mut self, track: Option<TrackMetadata>) {
        let playing = self.status == PlaybackStatus::Playing;
        self.metadata = track;
        self.position.set(0, playing);
    }

    pub fn position(&self) -> u128 {
        self.position.current()
    }

    pub fn set_position(&mut self, position: u128) {
        let playing = self.status == PlaybackStatus::Playing;
        self.position.set(position, playing);
    }

    pub fn set_loop_status(&mut self, loop_status: LoopStatus) {
        self.loop_status = loop_status;
    }

    pub fn set_shuffled(&mut self, shuffled: bool) {
        self.shuffled = shuffled;
    }

    pub fn set_playing(&mut self, status: PlaybackStatus) {
        self.status = status;
        match status {
            PlaybackStatus::Playing => {
                self.position.resume();
            }
            PlaybackStatus::Paused => {
                self.position.pause();
            }
            PlaybackStatus::Stopped => {
                self.position.set(0, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_status_from_a_dbus_value() {
        let status = |v: Value<'static>| LoopStatus::try_from(v).ok();
        assert_eq!(status(Value::from("Track")), Some(LoopStatus::Track));
        assert_eq!(status(Value::from("Playlist")), Some(LoopStatus::Playlist));
        assert_eq!(status(Value::from("None")), Some(LoopStatus::None));
        assert_eq!(status(Value::from("Shuffle")), Some(LoopStatus::None));
        assert_eq!(
            status(Value::from(3u32)),
            None,
            "a LoopStatus that is not a string is a D-Bus error, not None"
        );
    }
}
