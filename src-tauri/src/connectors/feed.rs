//! News feeds: the address any blog, forum, newspaper or release page can give, as RSS, Atom
//! or JSON Feed. Read-only.

use super::{Connector, REQUEST_LIMIT, excerpt, unreachable};
use crate::{
    http::client,
    items::{Kind, NewItem},
};
use async_trait::async_trait;
use mail_parser::decoders::html::html_to_text;
use serde_json::json;

/// A feed lists its latest entries; older ones than these are not kept.
const KEPT: usize = 30;

pub struct Feed {
    source: String,
    /// Kept with the secrets: some feeds are private, and their address is their key.
    address: Option<String>,
}

impl Feed {
    pub fn new(source: String, address: Option<String>) -> Self {
        Self { source, address }
    }
}

#[async_trait]
impl Connector for Feed {
    fn source(&self) -> &str {
        &self.source
    }

    async fn sync(&self) -> Result<Vec<NewItem>, String> {
        let address = self.address.as_deref().map(str::trim).unwrap_or_default();
        if !address.starts_with("https://") {
            return Err("A feed address starts with https://.".into());
        }
        let response = client()
            .get(address)
            .timeout(REQUEST_LIMIT)
            .send()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(format!("The feed answered {}.", response.status()));
        }
        articles(&response.bytes().await.map_err(unreachable)?)
    }
}

/// The latest entries of a feed, as articles signed with the feed's name.
fn articles(feed: &[u8]) -> Result<Vec<NewItem>, String> {
    let feed = feed_rs::parser::parse(feed)
        .map_err(|_| "This address does not give a feed (RSS or Atom).".to_owned())?;
    let name = feed.title.map(|title| plain(&title.content));
    let articles = feed.entries.into_iter().take(KEPT).map(|entry| {
        let text = entry
            .summary
            .map(|summary| summary.content)
            .or(entry.content.and_then(|content| content.body))
            .unwrap_or_default();
        let links = entry.links.into_iter().map(|link| link.href);
        NewItem {
            external_id: entry.id,
            kind: Kind::Article,
            title: plain(&entry.title.map(|title| title.content).unwrap_or_default()),
            body: excerpt(&plain(&text), 300),
            author: name.clone(),
            url: links.into_iter().find(|link| link.starts_with("https://")),
            state: "unread".into(),
            occurred_at: entry
                .published
                .or(entry.updated)
                .map_or(0, |at| at.timestamp()),
            ends_at: None,
            meta: json!({}),
        }
    });
    Ok(articles.collect())
}

/// Feeds carry HTML: only its text is kept, on one line.
fn plain(html: &str) -> String {
    let text = html_to_text(html);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::parse_time;

    #[test]
    fn reads_a_rss_feed() {
        let feed = r#"<?xml version="1.0"?>
            <rss version="2.0"><channel>
              <title>Async &amp; Rust</title>
              <link>https://blog.example.com</link>
              <item>
                <title>Cancellation safety, explained</title>
                <link>https://blog.example.com/cancellation</link>
                <guid>post-12</guid>
                <pubDate>Thu, 19 Nov 2026 08:30:00 GMT</pubDate>
                <description><![CDATA[<p>What happens when a <em>future</em> is dropped&nbsp;early.</p>
                  <img src="https://blog.example.com/a.png">]]></description>
              </item>
              <item><title>No date, no link</title><guid>post-11</guid></item>
            </channel></rss>"#;
        let articles = articles(feed.as_bytes()).unwrap();
        assert_eq!(articles.len(), 2);
        let first = &articles[0];
        assert_eq!(
            (first.kind, first.external_id.as_str()),
            (Kind::Article, "post-12")
        );
        assert_eq!(first.title, "Cancellation safety, explained");
        assert_eq!(first.body, "What happens when a future is dropped early.");
        assert_eq!(first.author.as_deref(), Some("Async & Rust"));
        assert_eq!(
            first.url.as_deref(),
            Some("https://blog.example.com/cancellation")
        );
        assert_eq!(
            first.occurred_at,
            parse_time("2026-11-19T08:30:00Z").unwrap()
        );
        assert_eq!(
            (articles[1].url.as_deref(), articles[1].occurred_at),
            (None, 0)
        );
    }

    #[test]
    fn reads_an_atom_feed_and_keeps_only_the_latest_entries() {
        let entry = |number: usize| {
            format!(
                "<entry><id>urn:post:{number}</id><title>Post {number}</title>\
                 <link href=\"https://example.com/{number}\"/>\
                 <updated>2026-11-18T10:00:00Z</updated>\
                 <content type=\"html\">&lt;p&gt;Body {number}&lt;/p&gt;</content></entry>"
            )
        };
        let entries: String = (1..=40).map(entry).collect();
        let feed = format!(
            "<feed xmlns=\"http://www.w3.org/2005/Atom\"><title>Releases</title>\
             <id>urn:feed</id><updated>2026-11-18T10:00:00Z</updated>{entries}</feed>"
        );
        let articles = articles(feed.as_bytes()).unwrap();
        assert_eq!(articles.len(), KEPT);
        assert_eq!(
            (articles[0].title.as_str(), articles[0].body.as_str()),
            ("Post 1", "Body 1")
        );
        assert_eq!(articles[0].url.as_deref(), Some("https://example.com/1"));
        assert_eq!(
            articles[0].occurred_at,
            parse_time("2026-11-18T10:00:00Z").unwrap()
        );
    }

    #[test]
    fn a_page_that_is_not_a_feed_is_an_error() {
        let error = articles(b"<html><body>Not found</body></html>").unwrap_err();
        assert_eq!(error, "This address does not give a feed (RSS or Atom).");
    }

    #[tokio::test]
    async fn an_address_that_is_not_https_is_refused() {
        for address in [
            None,
            Some("http://example.com/feed"),
            Some("file:///etc/passwd"),
        ] {
            let feed = Feed::new("news".into(), address.map(str::to_owned));
            assert_eq!(
                feed.sync().await.unwrap_err(),
                "A feed address starts with https://."
            );
        }
    }

    /// Reads a public feed over the network, so it only runs on demand:
    /// `cargo test -- --ignored real_news_feed --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn real_news_feed_syncs() {
        let address = "https://blog.rust-lang.org/feed.xml";
        let articles = Feed::new("news".into(), Some(address.into()))
            .sync()
            .await
            .unwrap();
        println!(
            "{} articles, first: {:?}",
            articles.len(),
            articles.first().map(|a| &a.title)
        );
        assert!(
            articles
                .iter()
                .all(|a| !a.title.is_empty() && a.url.is_some() && a.occurred_at > 0)
        );
        assert!(!articles.is_empty());
    }
}
