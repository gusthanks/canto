//! Authentication for Canto.
//!
//! Owns everything related to obtaining and persisting credentials:
//! the OAuth2 authorization-code (PKCE) flow ([`oauth2`]) and the secure
//! credential store backed by the Secret Service ([`token_store`]).

#[macro_use]
extern crate log;

mod credentials;
mod oauth2;
mod session_tokens;
pub mod soloist_key;
mod token_store;

pub use credentials::Credentials;
pub use oauth2::{
    effective_client_id, resolve_client_id, AuthcodeChallenge, CantoOauthClient, LoginCredentials,
    OAuthError, CLIENT_ID, ENV_CLIENT_ID, REDIRECT_URI, SESSION_CLIENT_ID,
};
pub use session_tokens::SessionTokens;
pub use token_store::TokenStore;
