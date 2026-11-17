//! Token provisioning abstraction for the data layer.

pub trait TokenProvider: Send + Sync + 'static {
    fn access_token(&self) -> Option<String>;
}

impl<F> TokenProvider for F
where
    F: Fn() -> Option<String> + Send + Sync + 'static,
{
    fn access_token(&self) -> Option<String> {
        self()
    }
}

impl TokenProvider for canto_auth::TokenStore {
    fn access_token(&self) -> Option<String> {
        self.get_cached_blocking().map(|creds| creds.access_token)
    }
}

impl TokenProvider for canto_auth::SessionTokens {
    fn access_token(&self) -> Option<String> {
        canto_auth::SessionTokens::access_token(self)
    }
}
