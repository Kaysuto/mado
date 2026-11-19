//! Mailboxes, over IMAP: the protocol every mail service speaks (Gmail, iCloud, Fastmail,
//! Proton Bridge, any hosting provider). Reads the recent inbox; can mark a mail as read
//! and archive it.

use super::{Action, Connector};
use crate::items::{Kind, NewItem, now};
use async_imap::{
    Client, Session,
    imap_proto::NameAttribute,
    types::{Fetch, Flag},
};
use async_trait::async_trait;
use futures::TryStreamExt;
use mail_parser::MessageParser;
use serde::Deserialize;
use serde_json::json;
use std::{fmt::Debug, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tokio_rustls::{
    TlsConnector,
    rustls::{ClientConfig, RootCertStore, crypto::aws_lc_rs, pki_types::ServerName},
};

const DAY: i64 = 86_400;
/// How far back the inbox is read, and how many of its most recent mails at most.
const DAYS: i64 = 14;
const MAX_MAILS: usize = 50;
/// Only the start of each mail is downloaded: enough for who, what, and a preview.
const HEAD_BYTES: usize = 16_384;
const PREVIEW_CHARS: usize = 300;
/// A whole conversation with the server never takes longer than this.
const LIMIT: Duration = Duration::from_secs(60);

/// Where the mailbox is and who signs in. The password is in the keychain.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Options {
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
}

pub struct Imap {
    source: String,
    options: Options,
    password: Option<String>,
}

impl Imap {
    pub fn new(source: String, options: Options, password: Option<String>) -> Self {
        Self {
            source,
            options,
            password,
        }
    }

    /// Opens an encrypted connection and signs in.
    async fn session(&self) -> Result<Session<impl Stream>, String> {
        let password = self
            .password
            .as_deref()
            .ok_or("This mailbox has no password yet.")?;
        let host = self.options.host.trim();
        if host.is_empty() || self.options.username.trim().is_empty() {
            return Err("This mailbox needs a server and a user name.".into());
        }
        let tcp = TcpStream::connect((host, self.options.port.unwrap_or(993)))
            .await
            .map_err(|error| format!("Could not reach {host}: {error}"))?;

        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = ClientConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|error| error.to_string())?
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = ServerName::try_from(host.to_owned()).map_err(|error| error.to_string())?;
        let tls = TlsConnector::from(Arc::new(config))
            .connect(name, tcp)
            .await
            .map_err(|error| format!("Could not secure the connection to {host}: {error}"))?;

        sign_in(Client::new(tls), self.options.username.trim(), password).await
    }
}

/// What the IMAP library needs from a connection. A test uses a plain local socket.
trait Stream: AsyncRead + AsyncWrite + Unpin + Debug + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Debug + Send> Stream for T {}

fn explain(error: async_imap::error::Error) -> String {
    format!("The mail server said: {error}")
}

async fn sign_in<T: Stream>(
    mut client: Client<T>,
    user: &str,
    password: &str,
) -> Result<Session<T>, String> {
    client
        .read_response()
        .await
        .map_err(|error| error.to_string())?
        .ok_or("The mail server closed the connection.")?;
    client.login(user, password).await.map_err(|(error, _)| {
        format!("Sign-in refused. Check the user name and the password. ({error})")
    })
}

/// Selects the inbox and returns the number that changes if the server renumbers its mails.
async fn open_inbox<T: Stream>(session: &mut Session<T>) -> Result<u32, String> {
    let inbox = session.select("INBOX").await.map_err(explain)?;
    Ok(inbox.uid_validity.unwrap_or(0))
}

/// The most recent mails of the inbox received since `since` (Unix seconds).
async fn read_inbox<T: Stream>(
    session: &mut Session<T>,
    since: i64,
) -> Result<Vec<NewItem>, String> {
    let validity = open_inbox(session).await?;
    let day = chrono::DateTime::from_timestamp(since, 0).unwrap_or_default();
    // IMAP dates are English and day-precise: 5-Nov-2026.
    let found = session
        .uid_search(format!("SINCE {}", day.format("%-d-%b-%Y")))
        .await
        .map_err(explain)?;
    let mut uids: Vec<u32> = found.into_iter().collect();
    uids.sort_unstable();
    let recent = &uids[uids.len().saturating_sub(MAX_MAILS)..];
    if recent.is_empty() {
        return Ok(Vec::new());
    }
    let set = recent
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    // PEEK: reading a mail here must not mark it as read on the server.
    let query = format!("(UID FLAGS INTERNALDATE BODY.PEEK[]<0.{HEAD_BYTES}>)");
    let fetched: Vec<Fetch> = session
        .uid_fetch(set, query)
        .await
        .map_err(explain)?
        .try_collect()
        .await
        .map_err(explain)?;
    Ok(fetched
        .iter()
        .filter_map(|mail| {
            let seen = mail.flags().any(|flag| flag == Flag::Seen);
            let received = mail.internal_date().map(|date| date.timestamp());
            Some(mail_item(validity, mail.uid?, seen, received, mail.body()?))
        })
        .collect())
}

/// Translates the start of a raw mail into an item.
fn mail_item(validity: u32, uid: u32, seen: bool, received: Option<i64>, head: &[u8]) -> NewItem {
    let message = MessageParser::default().parse(head);
    let message = message.as_ref();
    let sender = message.and_then(|m| m.from()).and_then(|from| from.first());
    let address = sender.and_then(|sender| sender.address());
    NewItem {
        external_id: format!("{validity}:{uid}"),
        kind: Kind::Mail,
        title: message
            .and_then(|m| m.subject())
            .unwrap_or_default()
            .trim()
            .into(),
        body: message
            .and_then(|m| m.body_preview(PREVIEW_CHARS))
            .map(|preview| preview.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default(),
        author: sender
            .and_then(|sender| sender.name())
            .or(address)
            .map(str::to_owned),
        url: None,
        state: if seen { "read" } else { "unread" }.into(),
        occurred_at: received
            .or_else(|| {
                message
                    .and_then(|m| m.date())
                    .map(|date| date.to_timestamp())
            })
            .unwrap_or(0),
        ends_at: None,
        meta: json!({
            "from": address,
            // What this source can do to the item; the interface shows a button for each.
            "actions": if seen { json!(["archive"]) } else { json!(["mark_read", "archive"]) },
        }),
    }
}

async fn act_on<T: Stream>(
    session: &mut Session<T>,
    external_id: &str,
    action: Action,
) -> Result<(), String> {
    const MOVED: &str = "This mail is no longer where it was. Sync and try again.";
    let (validity, uid) = external_id.split_once(':').ok_or(MOVED)?;
    if open_inbox(session).await?.to_string() != validity || uid.parse::<u32>().is_err() {
        return Err(MOVED.into());
    }
    match action {
        Action::MarkRead => {
            let updates = session
                .uid_store(uid, "+FLAGS (\\Seen)")
                .await
                .map_err(explain)?;
            updates.try_collect::<Vec<_>>().await.map_err(explain)?;
        }
        Action::Archive => {
            let folders: Vec<_> = session
                .list(Some(""), Some("*"))
                .await
                .map_err(explain)?
                .try_collect()
                .await
                .map_err(explain)?;
            // Servers flag their archive folder; Gmail has "all mail" instead.
            let flagged = |wanted: &NameAttribute<'_>| {
                folders
                    .iter()
                    .find(|folder| folder.attributes().contains(wanted))
                    .map(|folder| folder.name().to_owned())
            };
            let archive = flagged(&NameAttribute::Archive)
                .or_else(|| flagged(&NameAttribute::All))
                .ok_or("This mailbox has no archive folder.")?;
            session.uid_mv(uid, archive).await.map_err(explain)?;
        }
    }
    Ok(())
}

#[async_trait]
impl Connector for Imap {
    fn source(&self) -> &str {
        &self.source
    }

    async fn sync(&self) -> Result<Vec<NewItem>, String> {
        let work = async {
            let mut session = self.session().await?;
            let items = read_inbox(&mut session, now() - DAYS * DAY).await;
            let _ = session.logout().await;
            items
        };
        tokio::time::timeout(LIMIT, work)
            .await
            .unwrap_or_else(|_| Err("The mail server took too long to answer.".into()))
    }

    async fn act(&self, external_id: &str, action: Action) -> Result<(), String> {
        let work = async {
            let mut session = self.session().await?;
            let outcome = act_on(&mut session, external_id, action).await;
            let _ = session.logout().await;
            outcome
        };
        tokio::time::timeout(LIMIT, work)
            .await
            .unwrap_or_else(|_| Err("The mail server took too long to answer.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
    };

    const MAIL: &str = "From: =?UTF-8?Q?Am=C3=A9lie_Durand?= <amelie@example.com>\r\n\
        Subject: =?UTF-8?Q?R=C3=A9union_d'=C3=A9quipe?=\r\n\
        Date: Mon, 5 Jan 2026 10:00:00 +0000\r\n\
        Content-Type: text/html; charset=utf-8\r\n\
        \r\n\
        <p>Bonjour,</p>\r\n<p>La   r&eacute;union est d&eacute;plac&eacute;e &agrave; 15 h.</p>\r\n";

    #[test]
    fn reads_sender_subject_and_a_plain_preview_out_of_a_raw_mail() {
        let item = mail_item(7, 42, false, Some(1000), MAIL.as_bytes());
        assert_eq!(item.external_id, "7:42");
        assert_eq!(item.kind, Kind::Mail);
        assert_eq!(item.title, "Réunion d'équipe");
        assert_eq!(item.author.as_deref(), Some("Amélie Durand"));
        assert_eq!(item.body, "Bonjour, La réunion est déplacée à 15 h.");
        assert_eq!(item.state, "unread");
        assert_eq!(item.occurred_at, 1000);
        assert_eq!(item.meta["from"], "amelie@example.com");
        assert_eq!(item.meta["actions"], json!(["mark_read", "archive"]));
    }

    #[test]
    fn falls_back_on_the_address_and_the_date_header() {
        let raw = "From: bot@example.com\r\nDate: Thu, 1 Jan 1970 00:01:00 +0000\r\n\r\nHi\r\n";
        let item = mail_item(1, 1, true, None, raw.as_bytes());
        assert_eq!(item.author.as_deref(), Some("bot@example.com"));
        assert_eq!(item.title, "");
        assert_eq!(item.occurred_at, 60);
        assert_eq!(item.state, "read");
        assert_eq!(item.meta["actions"], json!(["archive"]));
    }

    /// A scripted IMAP server: enough of the protocol to exercise the real client code.
    /// Returns its address and, once the client is gone, every command it received.
    async fn fake_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let served = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = socket.into_split();
            let mut lines = BufReader::new(reader).lines();
            let mut commands = Vec::new();
            writer.write_all(b"* OK ready\r\n").await.unwrap();
            while let Ok(Some(line)) = lines.next_line().await {
                let (tag, command) = line.split_once(' ').unwrap();
                commands.push(command.to_owned());
                let answer = match command.split(' ').take(2).collect::<Vec<_>>().as_slice() {
                    ["SELECT", ..] => "* 2 EXISTS\r\n* OK [UIDVALIDITY 7] ok\r\n".to_owned(),
                    ["UID", "SEARCH"] => "* SEARCH 12 11\r\n".to_owned(),
                    ["UID", "FETCH"] => format!(
                        "* 1 FETCH (UID 11 FLAGS (\\Seen) INTERNALDATE \"05-Jan-2026 10:00:00 +0000\" BODY[]<0> {{{0}}}\r\n{MAIL})\r\n\
                         * 2 FETCH (UID 12 FLAGS () INTERNALDATE \"06-Jan-2026 10:00:00 +0000\" BODY[]<0> {{{0}}}\r\n{MAIL})\r\n",
                        MAIL.len()
                    ),
                    ["LIST", ..] => "* LIST (\\HasNoChildren) \"/\" INBOX\r\n* LIST (\\HasNoChildren \\Archive) \"/\" Archives\r\n".to_owned(),
                    ["UID", "STORE"] => "* 2 FETCH (UID 12 FLAGS (\\Seen))\r\n".to_owned(),
                    _ => String::new(),
                };
                writer
                    .write_all(format!("{answer}{tag} OK done\r\n").as_bytes())
                    .await
                    .unwrap();
            }
            commands
        });
        (address, served)
    }

    #[tokio::test]
    async fn talks_to_a_server_reads_the_inbox_and_acts_on_a_mail() {
        let (address, served) = fake_server().await;
        let socket = TcpStream::connect(address).await.unwrap();
        let mut session = sign_in(Client::new(socket), "me@example.com", "secret")
            .await
            .unwrap();

        let items = read_inbox(&mut session, 1_767_225_600).await.unwrap();
        let summary: Vec<_> = items
            .iter()
            .map(|i| (i.external_id.as_str(), i.state.as_str(), i.title.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                ("7:11", "read", "Réunion d'équipe"),
                ("7:12", "unread", "Réunion d'équipe")
            ]
        );

        act_on(&mut session, "7:12", Action::MarkRead)
            .await
            .unwrap();
        act_on(&mut session, "7:12", Action::Archive).await.unwrap();
        // The server renumbered its mails since the item was read: nothing must be touched.
        assert!(act_on(&mut session, "6:12", Action::Archive).await.is_err());
        session.logout().await.unwrap();
        drop(session);

        let commands = served.await.unwrap();
        assert!(commands[0].starts_with("LOGIN"), "{commands:?}");
        assert!(
            commands.contains(&"UID SEARCH SINCE 1-Jan-2026".to_owned()),
            "{commands:?}"
        );
        assert!(
            commands
                .iter()
                .any(|c| c.starts_with("UID FETCH 11,12 ") && c.contains("BODY.PEEK[]<0.16384>")),
            "{commands:?}"
        );
        assert!(
            commands.contains(&"UID STORE 12 +FLAGS (\\Seen)".to_owned()),
            "{commands:?}"
        );
        assert!(
            commands
                .iter()
                .any(|c| c.starts_with("UID MOVE 12 ") && c.contains("Archives")),
            "{commands:?}"
        );
    }

    /// Opens a real encrypted connection to a public mail server with made-up credentials, so
    /// it only runs on demand: `cargo test -- --ignored real_tls`. The sign-in must be what
    /// fails, not the connection or its encryption.
    #[tokio::test]
    #[ignore]
    async fn real_tls_connection_reaches_the_sign_in() {
        let options = Options {
            host: "imap.gmail.com".into(),
            port: None,
            username: "madow-test@example.invalid".into(),
        };
        let imap = Imap::new("test".into(), options, Some("not-a-password".into()));
        let error = imap.sync().await.unwrap_err();
        assert!(error.starts_with("Sign-in refused"), "{error}");
    }
}
