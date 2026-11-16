use std::sync::Arc;
use std::time::Duration;

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::StreamExt;
use tokio::{task, time};

use crate::app::AppAction;
use riff_api::ApiService;

mod official;
mod player;
mod spotify_mpris;
pub use official::{account_needs_official_client, remember_account_needs_official_client};
pub use player::ConnectCommand;

#[tokio::main]
async fn connect_server(
    api: Arc<ApiService>,
    action_sender: UnboundedSender<AppAction>,
    receiver: UnboundedReceiver<ConnectCommand>,
) {
    let player = Arc::new(player::ConnectPlayer::new(api, action_sender));

    let player_clone = Arc::clone(&player);
    task::spawn(async move {
        // Soon after start, so a reopened Canto picks up what the Spotify
        // app is already playing
        let mut delay = Duration::from_secs(4);
        loop {
            time::sleep(delay).await;
            if player_clone.has_device() {
                player_clone.sync_state().await;
            } else {
                player_clone.attach_to_playing_official().await;
            }
            delay = if player_clone.has_device() && player_clone.is_remote_playing() {
                Duration::from_secs(3)
            } else {
                Duration::from_secs(12)
            };
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
