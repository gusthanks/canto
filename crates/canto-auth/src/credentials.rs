use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: String,
    pub token_expiry_time: Option<SystemTime>,
    /// Spotify OAuth Client ID that minted this token (`None` for tokens
    /// saved before custom Client IDs were supported).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
}

impl Credentials {
    pub fn token_expired(&self) -> bool {
        match self.token_expiry_time {
            Some(v) => SystemTime::now() > v,
            None => true,
        }
    }
}
