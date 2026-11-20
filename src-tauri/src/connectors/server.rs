//! A server to keep an eye on: a port that must accept connections (a game server, a
//! database, anything) or a web address that must answer. While it does, it gives nothing;
//! when it stops, it gives one alert, which goes away when the server is back.

use super::Connector;
use crate::{
    http::client,
    items::{Kind, NewItem, now},
};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tokio::net::TcpStream;

/// A server that takes longer than this to answer is as good as down.
const PATIENCE: Duration = Duration::from_secs(8);

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Options {
    /// `host:port`, or a web address in https.
    pub address: String,
}

/// What is checked, read from the address the user typed.
#[derive(Debug, PartialEq)]
enum Target {
    Port(String, u16),
    Web(String),
}

pub struct Server {
    source: String,
    /// The name the user gave the server: the title of its alert.
    name: String,
    address: String,
}

impl Server {
    pub fn new(source: String, name: String, options: Options) -> Self {
        Self {
            source,
            name,
            address: options.address.trim().to_owned(),
        }
    }
}

fn target(address: &str) -> Result<Target, String> {
    const HELP: &str = "A server address is host:port, or starts with https://.";
    if address.starts_with("https://") {
        return Ok(Target::Web(address.to_owned()));
    }
    let (host, port) = address.rsplit_once(':').ok_or(HELP)?;
    let port = port.parse().map_err(|_| HELP)?;
    if host.is_empty() || host.contains(['/', ' ']) {
        return Err(HELP.into());
    }
    Ok(Target::Port(host.to_owned(), port))
}

/// Why the server counts as down; `None` while it answers.
async fn failure(target: &Target) -> Option<String> {
    match target {
        Target::Port(host, port) => {
            let connection = TcpStream::connect((host.as_str(), *port));
            match tokio::time::timeout(PATIENCE, connection).await {
                Ok(Ok(_)) => None,
                Ok(Err(error)) => Some(error.to_string()),
                Err(_) => Some(format!("no answer within {} seconds", PATIENCE.as_secs())),
            }
        }
        Target::Web(address) => match client().get(address).timeout(PATIENCE).send().await {
            // A page that is missing or forbidden is still a server that answers.
            Ok(response) if !response.status().is_server_error() => None,
            Ok(response) => Some(format!("answered {}", response.status())),
            Err(error) if error.is_timeout() => {
                Some(format!("no answer within {} seconds", PATIENCE.as_secs()))
            }
            Err(error) => Some(error.to_string()),
        },
    }
}

#[async_trait]
impl Connector for Server {
    fn source(&self) -> &str {
        &self.source
    }

    async fn sync(&self) -> Result<Vec<NewItem>, String> {
        let target = target(&self.address)?;
        let Some(reason) = failure(&target).await else {
            return Ok(Vec::new());
        };
        Ok(vec![NewItem {
            external_id: "down".into(),
            kind: Kind::Alert,
            title: self.name.clone(),
            body: format!("{}: {reason}", self.address),
            author: None,
            url: matches!(target, Target::Web(_)).then(|| self.address.clone()),
            state: "down".into(),
            // When it was last seen down: the check itself.
            occurred_at: now(),
            ends_at: None,
            meta: json!({ "server": true }),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

    fn server(address: &str) -> Server {
        let options = Options {
            address: address.into(),
        };
        Server::new("s".into(), "Lobby".into(), options)
    }

    #[test]
    fn an_address_is_a_port_or_a_web_address() {
        assert_eq!(
            target("play.example.com:25565"),
            Ok(Target::Port("play.example.com".into(), 25565))
        );
        assert_eq!(target("[::1]:5432"), Ok(Target::Port("[::1]".into(), 5432)));
        let page = "https://status.example.com/health";
        assert_eq!(target(page), Ok(Target::Web(page.into())));
        for wrong in [
            "",
            "play.example.com",
            "http://example.com:80",
            ":25565",
            "host:port",
            "a b:1",
        ] {
            assert!(target(wrong).is_err(), "{wrong}");
        }
    }

    #[tokio::test]
    async fn a_port_that_accepts_gives_nothing_and_one_that_refuses_gives_an_alert() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        assert_eq!(server(&address).sync().await.unwrap(), []);

        // Nothing listens on the port any more.
        drop(listener);
        let alerts = server(&address).sync().await.unwrap();
        assert_eq!(alerts.len(), 1);
        let alert = &alerts[0];
        assert_eq!(
            (alert.kind, alert.title.as_str(), alert.state.as_str()),
            (Kind::Alert, "Lobby", "down")
        );
        assert!(
            alert.body.starts_with(&format!("{address}: ")),
            "{}",
            alert.body
        );
        assert_eq!(
            (alert.url.as_deref(), &alert.meta),
            (None, &json!({ "server": true }))
        );
    }

    #[tokio::test]
    async fn a_web_address_is_up_unless_the_server_itself_fails() {
        let web = MockServer::start().await;
        for (page, status) in [("/ok", 200), ("/missing", 404), ("/broken", 503)] {
            Mock::given(path(page))
                .respond_with(ResponseTemplate::new(status))
                .mount(&web)
                .await;
        }
        let check = |page: &str| Target::Web(format!("{}{page}", web.uri()));
        assert_eq!(failure(&check("/ok")).await, None);
        assert_eq!(failure(&check("/missing")).await, None);
        let broken = failure(&check("/broken")).await;
        assert_eq!(broken.as_deref(), Some("answered 503 Service Unavailable"));
    }

    #[tokio::test]
    async fn an_address_that_cannot_be_read_is_an_error_not_an_alert() {
        let error = server("play.example.com").sync().await.unwrap_err();
        assert_eq!(
            error,
            "A server address is host:port, or starts with https://."
        );
    }
}
