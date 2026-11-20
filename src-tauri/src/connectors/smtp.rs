//! Sending mail, over SMTP: the protocol every mail service accepts mail through.

use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, header::ContentType},
    transport::smtp::authentication::Credentials,
};
use std::{
    hash::{BuildHasher, Hasher, RandomState},
    time::Duration,
};

/// The sending server never keeps a step of the conversation waiting longer than this.
const STEP_LIMIT: Duration = Duration::from_secs(30);

/// The user's answer to a mail they received.
pub struct Reply<'a> {
    /// The user's own address.
    pub from: &'a str,
    pub to: &'a str,
    /// Subject of the mail answered.
    pub subject: &'a str,
    /// `Message-ID` of the mail answered, without its angle brackets.
    pub message_id: Option<&'a str>,
    /// The conversation so far: the `References` of the mail answered.
    pub references: &'a [String],
    pub text: &'a str,
}

/// "Re: " in front of the subject, unless it is there already.
fn reply_subject(subject: &str) -> String {
    let subject = subject.split_whitespace().collect::<Vec<_>>().join(" ");
    if subject.to_lowercase().starts_with("re:") {
        subject
    } else {
        format!("Re: {subject}")
    }
}

/// Writes the mail. Headers are encoded by the library, so nothing in them can add a header
/// or a recipient of its own.
pub fn message(reply: &Reply<'_>) -> Result<Message, String> {
    let from: Mailbox = reply.from.trim().parse().map_err(|_| {
        "This mailbox cannot send: its user name is not an e-mail address.".to_owned()
    })?;
    let to: Mailbox = reply
        .to
        .trim()
        .parse()
        .map_err(|_| "The address to answer to is not valid.".to_owned())?;

    // An id of our own, under the sender's domain: one made on "localhost" looks like spam.
    let random = || RandomState::new().build_hasher().finish();
    let id = format!(
        "<{:016x}{:016x}@{}>",
        random(),
        random(),
        from.email.domain()
    );

    let mut builder = Message::builder()
        .from(from)
        .to(to)
        .subject(reply_subject(reply.subject))
        .message_id(Some(id))
        .header(ContentType::TEXT_PLAIN);
    // These two headers are what keeps the answer in the same conversation on the other side.
    if let Some(answered) = reply.message_id {
        let thread: Vec<String> = reply
            .references
            .iter()
            .map(String::as_str)
            .chain([answered])
            .map(|id| format!("<{id}>"))
            .collect();
        builder = builder
            .in_reply_to(format!("<{answered}>"))
            .references(thread.join(" "));
    }
    builder
        .body(
            reply
                .text
                .trim()
                .replace("\r\n", "\n")
                .replace('\n', "\r\n"),
        )
        .map_err(|error| format!("The mail could not be written: {error}"))
}

/// An encrypted connection to the sending server: encrypted from the first byte on port 465,
/// upgraded before signing in on any other port. Never in clear.
pub fn transport(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, String> {
    let builder = if port == 465 {
        AsyncSmtpTransport::<Tokio1Executor>::relay(host)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
    }
    .map_err(|error| format!("Could not secure the connection to {host}: {error}"))?;
    Ok(builder
        .port(port)
        .credentials(Credentials::new(user.to_owned(), password.to_owned()))
        .timeout(Some(STEP_LIMIT))
        .build())
}

pub async fn deliver(
    transport: &AsyncSmtpTransport<Tokio1Executor>,
    message: Message,
) -> Result<(), String> {
    transport
        .send(message)
        .await
        .map(|_| ())
        .map_err(|error| format!("The mail was not sent. The sending server said: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mail_parser::MessageParser;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
    };

    fn reply<'a>(subject: &'a str, references: &'a [String]) -> Reply<'a> {
        Reply {
            from: "me@example.com",
            to: "amelie@example.org",
            subject,
            message_id: Some("abc@example.org"),
            references,
            text: "Bonjour Amélie,\n\nC'est noté pour 15 h.\n.\nÀ jeudi",
        }
    }

    #[test]
    fn writes_a_reply_that_stays_in_its_conversation() {
        let earlier = ["first@example.org".to_owned()];
        let raw = message(&reply("Réunion d'équipe", &earlier))
            .unwrap()
            .formatted();
        let text = String::from_utf8_lossy(&raw);
        assert!(
            text.contains("In-Reply-To: <abc@example.org>\r\n"),
            "{text}"
        );
        assert!(
            text.contains("References: <first@example.org> <abc@example.org>\r\n"),
            "{text}"
        );

        let parsed = MessageParser::default().parse(&raw).unwrap();
        let address = |list: Option<&mail_parser::Address<'_>>| {
            list.and_then(|list| list.first())
                .and_then(|one| one.address())
                .map(str::to_owned)
        };
        assert_eq!(address(parsed.from()).as_deref(), Some("me@example.com"));
        assert_eq!(address(parsed.to()).as_deref(), Some("amelie@example.org"));
        assert_eq!(parsed.subject(), Some("Re: Réunion d'équipe"));
        assert!(parsed.message_id().unwrap().ends_with("@example.com"));
        assert_eq!(
            parsed.body_text(0).unwrap().replace("\r\n", "\n").trim(),
            "Bonjour Amélie,\n\nC'est noté pour 15 h.\n.\nÀ jeudi"
        );
    }

    #[test]
    fn a_subject_cannot_add_a_header_and_is_not_prefixed_twice() {
        let raw = message(&reply("RE: Budget\r\nBcc: thief@example.net", &[]))
            .unwrap()
            .formatted();
        let parsed = MessageParser::default().parse(&raw).unwrap();
        assert_eq!(parsed.subject(), Some("RE: Budget Bcc: thief@example.net"));
        assert!(parsed.bcc().is_none());
        assert_eq!(reply_subject("Budget"), "Re: Budget");
    }

    #[test]
    fn refuses_addresses_that_are_not_addresses() {
        let mut bad = reply("Hi", &[]);
        bad.from = "just-a-login";
        assert!(message(&bad).unwrap_err().contains("user name"));
        let mut bad = reply("Hi", &[]);
        bad.to = "a@example.org, b@example.org";
        assert!(message(&bad).is_err());
    }

    /// A scripted sending server. Returns its port and, once the client is gone, every line
    /// it received.
    async fn fake_server() -> (u16, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = socket.into_split();
            let mut lines = BufReader::new(reader).lines();
            let mut received = Vec::new();
            let mut in_mail = false;
            writer.write_all(b"220 ready\r\n").await.unwrap();
            while let Ok(Some(line)) = lines.next_line().await {
                let answer = if in_mail {
                    in_mail = line != ".";
                    if in_mail { "" } else { "250 queued\r\n" }
                } else {
                    match line.split(' ').next().unwrap_or_default() {
                        "EHLO" => "250-fake\r\n250 AUTH PLAIN\r\n",
                        "AUTH" => "235 welcome\r\n",
                        "DATA" => {
                            in_mail = true;
                            "354 go ahead\r\n"
                        }
                        "QUIT" => "221 bye\r\n",
                        _ => "250 ok\r\n",
                    }
                };
                received.push(line);
                writer.write_all(answer.as_bytes()).await.unwrap();
            }
            received
        });
        (port, served)
    }

    #[tokio::test]
    async fn signs_in_and_hands_the_mail_to_the_server() {
        let (port, served) = fake_server().await;
        // The real transport refuses to talk in clear; only this test does, to a local socket.
        let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous("127.0.0.1")
            .port(port)
            .credentials(Credentials::new("me@example.com".into(), "secret".into()))
            .build();
        deliver(&transport, message(&reply("Réunion", &[])).unwrap())
            .await
            .unwrap();
        drop(transport);

        let received = served.await.unwrap();
        let has = |start: &str| received.iter().any(|line| line.starts_with(start));
        assert!(has("AUTH PLAIN "), "{received:?}");
        assert!(has("MAIL FROM:<me@example.com>"), "{received:?}");
        assert!(has("RCPT TO:<amelie@example.org>"), "{received:?}");
        assert!(has("In-Reply-To: <abc@example.org>"), "{received:?}");
        // A line holding a single dot would end the mail early if it were sent as it is.
        assert!(received.contains(&"..".to_owned()), "{received:?}");
    }

    /// Opens a real encrypted connection to a public sending server with made-up credentials,
    /// so it only runs on demand: `cargo test -- --ignored real_smtp`. The sign-in must be
    /// what fails, before any mail is handed over.
    #[tokio::test]
    #[ignore]
    async fn real_smtp_connection_reaches_the_sign_in() {
        for port in [465, 587] {
            let transport = transport(
                "smtp.gmail.com",
                port,
                "madow-test@example.invalid",
                "not-a-password",
            )
            .unwrap();
            let error = deliver(&transport, message(&reply("Test", &[])).unwrap())
                .await
                .unwrap_err();
            assert!(error.contains("535"), "port {port}: {error}");
        }
    }
}
