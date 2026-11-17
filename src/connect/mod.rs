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
        let mut delay = Duration::from_secs(4);
        loop {
            time::sleep(Duration::from_secs(1)).await;
            waited += Duration::from_secs(1);
            let pace = if player_clone.has_device() && player_clone.polls_locally() {
                // Soloist answers locally in a few ms: follow it closely
                Duration::from_secs(2)
            } else if player_clone.has_device() && player_clone.is_remote_playing() {
                Duration::from_secs(3)
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

    receiver
        .for_each(|command| async { player.handle_command(command).await.unwrap() })
        .await;
}

pub fn start_connect_server(
    api: Arc<ApiService>,
    action_sender: UnboundedSender<AppAction>,
) -> UnboundedSender<ConnectCommand> {
    let (sender, receiver) = unbounded();

    std::thread::spawn(move || connect_server(api, action_sender, receiver));

    sender
}
