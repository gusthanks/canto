//! The Spotify Soloist API key, kept in the Secret Service next to the login.
//!
//! Soloist (Spotify's own headless player) needs a developer key the user
//! makes in Spotify's dashboard. It is a password: it never goes to a
//! settings file or a log.

use anyhow::Result;
use oo7::Keyring;

const ATTRS: [(&str, &str); 2] = [("service", "canto-soloist"), ("kind", "api-key")];
const LABEL: &str = "Spotify Soloist API key (Canto)";

async fn keyring() -> Result<Keyring> {
    let keyring = Keyring::new().await?;
    if let Err(e) = keyring.unlock().await {
        warn!("Failed to unlock keyring: {e}");
    }
    Ok(keyring)
}

/// The saved key, if there is one.
pub async fn load() -> Option<String> {
    let keyring = keyring().await.ok()?;
    let items = keyring.search_items(&ATTRS).await.ok()?;
    let secret = items.first()?.secret().await.ok()?;
    let key = String::from_utf8_lossy(secret.as_bytes())
        .trim()
        .to_string();
    (!key.is_empty()).then_some(key)
}

/// Saves `key` (trimmed), replacing the one saved before. An empty key
/// removes it.
pub async fn store(key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() {
        return clear().await;
    }
    keyring()
        .await?
        .create_item(LABEL, &ATTRS, key.as_bytes(), true)
        .await?;
    info!("Saved the Soloist key");
    Ok(())
}

pub async fn clear() -> Result<()> {
    let keyring = keyring().await?;
    for item in keyring.search_items(&ATTRS).await? {
        item.delete().await?;
    }
    Ok(())
}

/// Runs `future` on a small runtime of its own, for callers outside one
/// (the settings window).
fn block_on<T>(future: impl std::future::Future<Output = T>) -> Result<T> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    Ok(runtime.block_on(future))
}

/// Whether a key is saved. Blocks: call it off the main thread.
pub fn is_saved_blocking() -> bool {
    block_on(load()).ok().flatten().is_some()
}

/// [`store`], blocking: call it off the main thread.
pub fn store_blocking(key: &str) -> Result<()> {
    block_on(store(key))?
}
