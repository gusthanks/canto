use std::sync::Arc;
use std::time::Duration;

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::StreamExt;
use tokio::{task, time};

use crate::app::AppAction;
use canto_api::ApiService;

mod official;
mod player;
pub mod soloist;
mod spotify_mpris;
pub use official::{
    account_needs_official_client, is_this_computer, remember_account_needs_official_client,
};
pub use player::ConnectCommand;

/// Quits the Spotify app Canto plays through (accounts that need it), so it
/// doesn't keep playing once Canto is gone. Other accounts' apps are left
/// alone: there the Spotify app is the user's own.
pub fn quit_official_client() {
    soloist::stop_spawned();
    if !official::account_needs_official_client() || !spotify_mpris::is_running() {
        return;
    }
    let _ = spotify_mpris::pause();
    match spotify_mpris::quit() {
        Ok(()) => log::info!("quit the hidden Spotify app along with Canto"),
        Err(e) => log::warn!("couldn't quit the Spotify app: {e}"),
    }
}

/// Whether a Connect device is the player on this computer: Canto's own
/// Soloist (it calls itself a speaker) or the Spotify app here.
pub fn plays_here(label: &str) -> bool {
    label == soloist::DEVICE_NAME || is_this_computer(label)
}

#[tokio::main]
async fn connect_server(
    api: Arc<ApiService>,
    action_sender: UnboundedSender<AppAction>,
    receiver: UnboundedReceiver<ConnectCommand>,
) {
    let player = Arc::new(player::ConnectPlayer::new(api, action_sender));

    let player_clone = Arc::clone(&player);
    task::spawn(async move {
        // Soon after start, so a reopened Canto picks up what is already
        // playing (the Spotify app here, an Alexa, a phone).
        // Ticks of a second, the pace re-read every tick: a track started a
        // moment ago shows up in about two seconds, not after a long wait
        // chosen while nothing played
        let mut waited = Duration::ZERO;
        let mut delay = Duration::from_secs(1);
        loop {
            time::sleep(Duration::from_secs(1)).await;
            waited += Duration::from_secs(1);
            let pace = if player_clone.has_device() && player_clone.polls_locally() {
                // Soloist / MPRIS answers locally in a few ms with no network calls
                Duration::from_secs(1)
            } else if player_clone.has_device() && player_clone.is_remote_playing() {
                Duration::from_secs(2)
            } else {
                Duration::from_secs(12)
            };
            delay = delay.min(pace);
            if waited < delay {
                continue;
            }
            waited = Duration::ZERO;
            delay = pace;
            if player_clone.has_device() {
                player_clone.sync_state().await;
            } else {
                player_clone.attach_to_active_device().await;
            }
        }
    });

    let mut receiver = receiver;
    let mut pending: Option<ConnectCommand> = None;
    loop {
        let mut command = match pending.take() {
            Some(cmd) => cmd,
            None => match receiver.next().await {
                Some(cmd) => cmd,
                None => break,
            },
        };
        // Skip superseded queued commands (e.g. rapid track clicks, seeks, or
        // volume drags) so stale HTTP requests never pile up behind the latest one.
        while let Ok(next) = receiver.try_recv() {
            match coalesce_connect_command(command, next) {
                Coalesced::Merged(merged) => command = merged,
                Coalesced::KeepBoth(first, second) => {
                    command = first;
                    pending = Some(second);
                    break;
                }
            }
        }
        player.handle_command(command).await;
    }
}

#[derive(Debug)]
enum Coalesced {
    Merged(ConnectCommand),
    KeepBoth(ConnectCommand, ConnectCommand),
}

fn coalesce_connect_command(current: ConnectCommand, next: ConnectCommand) -> Coalesced {
    let supersedes = matches!(
        (&current, &next),
        (
            ConnectCommand::PlayerLoad { .. } | ConnectCommand::PlayerLoadInContext { .. },
            ConnectCommand::PlayerLoad { .. } | ConnectCommand::PlayerLoadInContext { .. }
        ) | (ConnectCommand::PlayerSeek(_), ConnectCommand::PlayerSeek(_))
            | (
                ConnectCommand::PlayerSetVolume(_),
                ConnectCommand::PlayerSetVolume(_)
            )
    );
    if supersedes {
        Coalesced::Merged(next)
    } else {
        Coalesced::KeepBoth(current, next)
    }
}

pub fn start_connect_server(
    api: Arc<ApiService>,
    action_sender: UnboundedSender<AppAction>,
) -> UnboundedSender<ConnectCommand> {
    let (sender, receiver) = unbounded();

    std::thread::spawn(move || connect_server(api, action_sender, receiver));

    sender
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_superseded_loads_seeks_and_volume() {
        let first = ConnectCommand::PlayerLoad {
            songs: vec!["a".into()],
            offset: 0,
            position_ms: None,
        };
        let second = ConnectCommand::PlayerLoad {
            songs: vec!["b".into()],
            offset: 0,
            position_ms: None,
        };
        match coalesce_connect_command(first, second) {
            Coalesced::Merged(ConnectCommand::PlayerLoad { songs, .. }) => {
                assert_eq!(songs, vec!["b"]);
            }
            other => panic!("expected merged load, got {:?}", other),
        }

        match coalesce_connect_command(
            ConnectCommand::PlayerSetVolume(20),
            ConnectCommand::PlayerSetVolume(80),
        ) {
            Coalesced::Merged(ConnectCommand::PlayerSetVolume(80)) => {}
            other => panic!("expected merged volume, got {:?}", other),
        }

        match coalesce_connect_command(ConnectCommand::PlayerNext, ConnectCommand::PlayerNext) {
            Coalesced::KeepBoth(ConnectCommand::PlayerNext, ConnectCommand::PlayerNext) => {}
            other => panic!(
                "consecutive next presses must never be dropped, got {:?}",
                other
            ),
        }
    }
}
