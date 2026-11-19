//! One HTTP client for the whole core, so connections are reused.

use std::{sync::OnceLock, time::Duration};

pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("madow/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default()
    })
}
