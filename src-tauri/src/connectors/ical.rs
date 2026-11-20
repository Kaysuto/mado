//! Calendars, read from an iCalendar feed: the address every calendar service can give
//! (Google, Outlook, iCloud, Proton, Nextcloud…). Read-only.

use super::{Connector, REQUEST_LIMIT, excerpt, unreachable};
use crate::{
    http::client,
    items::{EventDraft, Kind, NewItem, now},
};
use async_trait::async_trait;
use chrono::{Local, NaiveDateTime, TimeZone};
use icalendar::{
    Calendar, CalendarDateTime, Component, DatePerhapsTime, EventLike, EventStatus, rrule::Tz,
};
use serde_json::json;
use std::collections::HashSet;

const DAY: i64 = 86_400;
/// How far back and ahead events are kept.
const PAST: i64 = 31 * DAY;
const AHEAD: i64 = 366 * DAY;
/// A repeating event never yields more occurrences than this in the window.
const MAX_OCCURRENCES: u16 = 800;

/// Services whose links are a video call to join.
const MEETING_HOSTS: [&str; 8] = [
    "meet.google.com",
    "zoom.us",
    "teams.microsoft.com",
    "teams.live.com",
    "webex.com",
    "whereby.com",
    "discord.gg",
    "discord.com",
];

pub struct Ical {
    source: String,
    /// The feed's address. It is a secret: whoever has it can read the calendar.
    address: Option<String>,
}

impl Ical {
    pub fn new(source: String, address: Option<String>) -> Self {
        Self { source, address }
    }
}

#[async_trait]
impl Connector for Ical {
    fn source(&self) -> &str {
        &self.source
    }

    async fn sync(&self) -> Result<Vec<NewItem>, String> {
        let address = self
            .address
            .as_deref()
            .ok_or("This calendar has no address yet.")?;
        // Calendar apps hand out `webcal://` links; they are plain https underneath.
        let address = address.trim().replacen("webcal://", "https://", 1);
        if !address.starts_with("https://") {
            return Err("A calendar address starts with https:// or webcal://.".into());
        }
        let response = client()
            .get(address)
            .timeout(REQUEST_LIMIT)
            .send()
            .await
            .map_err(unreachable)?;
        if !response.status().is_success() {
            return Err(format!("The calendar answered {}.", response.status()));
        }
        let feed = response.text().await.map_err(unreachable)?;
        let now = now();
        events(&feed, now - PAST, now + AHEAD)
    }

    fn event_page(&self, event: &EventDraft) -> Option<String> {
        // A feed is read-only. This service is the one known to have a page that creates an
        // event from its address; with any other, the event is kept in Madow.
        if !self.address.as_deref()?.contains("://calendar.google.com/") {
            return None;
        }
        let (start, end) = event.span().ok()?;
        // Without a zone, the page reads the times in the calendar's own one.
        let stamp = if event.all_day {
            "%Y%m%d"
        } else {
            "%Y%m%dT%H%M%S"
        };
        let dates = format!("{}/{}", start.format(stamp), end.format(stamp));
        reqwest::Url::parse_with_params(
            "https://calendar.google.com/calendar/render",
            [
                ("action", "TEMPLATE"),
                ("text", event.title.trim()),
                ("dates", &dates),
                ("location", event.location.trim()),
            ],
        )
        .ok()
        .map(String::from)
    }
}

/// The events of a feed that start in `[from, to)`, repeating ones expanded.
fn events(feed: &str, from: i64, to: i64) -> Result<Vec<NewItem>, String> {
    let calendar: Calendar = feed
        .parse()
        .map_err(|error| format!("This is not a calendar feed: {error}"))?;

    // An occurrence of a repeating event that was moved or edited comes as its own entry,
    // pointing at the occurrence it replaces. The series must then skip that occurrence.
    let replaced: HashSet<(String, i64)> = calendar
        .events()
        .filter_map(|event| {
            let original = moment(&event.get_recurrence_id()?)?.0;
            Some((event.get_uid()?.to_owned(), original))
        })
        .collect();

    let mut items = Vec::new();
    for event in calendar.calendar_events() {
        if event.get_status() == Some(EventStatus::Cancelled) {
            continue;
        }
        let Some((start, all_day)) = event.get_start().as_ref().and_then(moment) else {
            continue;
        };
        let length = event
            .get_end()
            .as_ref()
            .and_then(moment)
            .map(|(end, _)| end - start)
            .filter(|length| *length > 0);
        let uid = event.get_uid().unwrap_or_default();
        let is_replacement = event.get_recurrence_id().is_some();

        let starts: Vec<i64> = if event.property_value("RRULE").is_some() && !is_replacement {
            match event.get_recurrence() {
                Ok(series) => series
                    .after(at(from))
                    .before(at(to))
                    .all(MAX_OCCURRENCES)
                    .dates
                    .into_iter()
                    .map(|date| date.timestamp())
                    .filter(|start| !replaced.contains(&(uid.to_owned(), *start)))
                    .collect(),
                // A rule the library cannot read: better the first occurrence than nothing.
                Err(_) => vec![start],
            }
        } else {
            vec![start]
        };

        let texts = [
            event.get_location(),
            event.get_description(),
            event.get_url(),
            event.property_value("X-GOOGLE-CONFERENCE"),
        ];
        let link = texts.into_iter().flatten().find_map(meeting_link);

        for start in starts
            .into_iter()
            .filter(|start| (from..to).contains(start))
        {
            items.push(NewItem {
                external_id: format!("{uid}@{start}"),
                kind: Kind::Event,
                title: event.get_summary().unwrap_or_default().into(),
                body: excerpt(event.get_description().unwrap_or_default(), 500),
                author: None,
                url: event
                    .get_url()
                    .filter(|url| url.starts_with("https://"))
                    .map(str::to_owned),
                state: String::new(),
                occurred_at: start,
                ends_at: length.map(|length| start + length),
                meta: json!({
                    "allDay": all_day,
                    "location": event.get_location(),
                    "link": link,
                }),
            });
        }
    }
    Ok(items)
}

/// Unix seconds of a date from the feed, and whether it is a whole day rather than a time.
/// Times without a usable time zone are read as this computer's local time.
fn moment(date: &DatePerhapsTime) -> Option<(i64, bool)> {
    let local =
        |naive: &NaiveDateTime| Some(Local.from_local_datetime(naive).earliest()?.timestamp());
    match date {
        DatePerhapsTime::Date(day) => Some((local(&day.and_hms_opt(0, 0, 0)?)?, true)),
        DatePerhapsTime::DateTime(time) => {
            let seconds = match time {
                CalendarDateTime::Utc(utc) => utc.timestamp(),
                CalendarDateTime::Floating(naive) => local(naive)?,
                // Outlook names zones its own way ("Romance Standard Time"), which the zone
                // database does not know; the user's own zone is the best guess then.
                CalendarDateTime::WithTimezone { date_time, .. } => match time.try_into_utc() {
                    Some(utc) => utc.timestamp(),
                    None => local(date_time)?,
                },
            };
            Some((seconds, false))
        }
    }
}

fn at(seconds: i64) -> chrono::DateTime<Tz> {
    chrono::DateTime::from_timestamp(seconds, 0)
        .unwrap_or_default()
        .with_timezone(&Tz::UTC)
}

/// The first video-call address found in a text.
fn meeting_link(text: &str) -> Option<String> {
    text.match_indices("https://").find_map(|(start, _)| {
        let rest = &text[start..];
        let end = rest
            .find(|c: char| c.is_whitespace() || "<>\"')\\".contains(c))
            .unwrap_or(rest.len());
        let link = &rest[..end];
        let host = link["https://".len()..].split('/').next()?;
        MEETING_HOSTS
            .iter()
            .any(|known| host == *known || host.ends_with(&format!(".{known}")))
            .then(|| link.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600;

    #[test]
    fn only_a_calendar_known_to_have_a_creation_page_gives_one() {
        let lunch = EventDraft {
            title: " Lunch & talk ".into(),
            start: "2026-11-26T12:30".into(),
            end: "2026-11-26T13:30".into(),
            all_day: false,
            location: "Le Bistro".into(),
        };
        let calendar = |address: &str| Ical::new("a".into(), Some(address.into()));
        let known = calendar("webcal://calendar.google.com/calendar/ical/someone/basic.ics");
        assert_eq!(
            known.event_page(&lunch).as_deref(),
            Some(
                "https://calendar.google.com/calendar/render?action=TEMPLATE&text=Lunch+%26+talk&dates=20261126T123000%2F20261126T133000&location=Le+Bistro"
            )
        );
        // A whole-day event ends the day after its last day.
        let trip = EventDraft {
            start: "2026-11-26".into(),
            end: "2026-11-27".into(),
            all_day: true,
            ..lunch.clone()
        };
        assert!(
            known
                .event_page(&trip)
                .unwrap()
                .contains("&dates=20261126%2F20261128&")
        );
        assert_eq!(
            calendar("https://example.com/feed.ics").event_page(&lunch),
            None
        );
        assert_eq!(Ical::new("b".into(), None).event_page(&lunch), None);
    }

    fn feed(events: &str) -> String {
        format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:test\r\n{events}END:VCALENDAR\r\n")
    }

    fn event(lines: &str) -> String {
        format!("BEGIN:VEVENT\r\n{lines}END:VEVENT\r\n")
    }

    /// 2026-01-05 is a Monday. All test times are UTC so they read the same on any machine.
    const JAN_1: i64 = 1_767_225_600;
    const JAN_5_10H: i64 = JAN_1 + 4 * DAY + 10 * HOUR;

    #[test]
    fn reads_a_timed_event_with_its_place_and_call_link() {
        let feed = feed(&event(
            "UID:one\r\nSUMMARY:Weekly sync\r\nDTSTART:20260105T100000Z\r\nDTEND:20260105T103000Z\r\n\
             LOCATION:Room 4\r\nDESCRIPTION:Join at https://meet.google.com/abc-defg-hij now\r\n",
        ));
        let items = events(&feed, JAN_1, JAN_1 + 31 * DAY).unwrap();
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.kind, Kind::Event);
        assert_eq!(item.title, "Weekly sync");
        assert_eq!(item.occurred_at, JAN_5_10H);
        assert_eq!(item.ends_at, Some(JAN_5_10H + HOUR / 2));
        assert_eq!(item.meta["allDay"], false);
        assert_eq!(item.meta["location"], "Room 4");
        assert_eq!(item.meta["link"], "https://meet.google.com/abc-defg-hij");
    }

    #[test]
    fn reads_time_zones_and_whole_days() {
        let feed = feed(&format!(
            "{}{}",
            event(
                "UID:paris\r\nSUMMARY:Lunch\r\nDTSTART;TZID=Europe/Paris:20260105T120000\r\nDTEND;TZID=Europe/Paris:20260105T130000\r\n"
            ),
            event(
                "UID:day\r\nSUMMARY:Holiday\r\nDTSTART;VALUE=DATE:20260106\r\nDTEND;VALUE=DATE:20260107\r\n"
            ),
        ));
        let items = events(&feed, JAN_1 - DAY, JAN_1 + 31 * DAY).unwrap();
        let lunch = items.iter().find(|i| i.title == "Lunch").unwrap();
        // Noon in Paris is 11:00 UTC in winter.
        assert_eq!(lunch.occurred_at, JAN_1 + 4 * DAY + 11 * HOUR);
        let holiday = items.iter().find(|i| i.title == "Holiday").unwrap();
        assert_eq!(holiday.meta["allDay"], true);
        assert_eq!(holiday.ends_at, Some(holiday.occurred_at + DAY));
    }

    #[test]
    fn expands_a_series_minus_what_was_removed_moved_or_cancelled() {
        let feed = feed(&format!(
            "{}{}{}",
            // Every Monday at 10:00, four times; the second one is removed.
            event(
                "UID:series\r\nSUMMARY:Standup\r\nDTSTART:20260105T100000Z\r\nDTEND:20260105T101500Z\r\nRRULE:FREQ=WEEKLY;COUNT=4\r\nEXDATE:20260112T100000Z\r\n"
            ),
            // The third one is moved to 15:00.
            event(
                "UID:series\r\nSUMMARY:Standup (moved)\r\nRECURRENCE-ID:20260119T100000Z\r\nDTSTART:20260119T150000Z\r\nDTEND:20260119T151500Z\r\n"
            ),
            event(
                "UID:gone\r\nSUMMARY:Cancelled\r\nSTATUS:CANCELLED\r\nDTSTART:20260107T100000Z\r\n"
            ),
        ));
        let mut found: Vec<_> = events(&feed, JAN_1, JAN_1 + 60 * DAY)
            .unwrap()
            .into_iter()
            .map(|i| (i.occurred_at, i.title))
            .collect();
        found.sort();
        assert_eq!(
            found,
            [
                (JAN_5_10H, "Standup".to_owned()),
                (
                    JAN_5_10H + 14 * DAY + 5 * HOUR,
                    "Standup (moved)".to_owned()
                ),
                (JAN_5_10H + 21 * DAY, "Standup".to_owned()),
            ]
        );
    }

    #[test]
    fn keeps_only_what_starts_in_the_window_and_gives_each_occurrence_its_own_id() {
        let feed = feed(&event(
            "UID:daily\r\nSUMMARY:Daily\r\nDTSTART:20260101T090000Z\r\nRRULE:FREQ=DAILY\r\n",
        ));
        let items = events(&feed, JAN_1 + 2 * DAY, JAN_1 + 5 * DAY).unwrap();
        assert_eq!(items.len(), 3);
        let ids: HashSet<_> = items.iter().map(|i| i.external_id.clone()).collect();
        assert_eq!(ids.len(), 3);
        assert!(items.iter().all(|i| i.ends_at.is_none()));
    }

    #[test]
    fn only_known_video_call_services_are_join_links() {
        assert_eq!(
            meeting_link(
                "Agenda: https://docs.example.com/a then <https://acme.zoom.us/j/123?pwd=x>"
            ),
            Some("https://acme.zoom.us/j/123?pwd=x".into())
        );
        assert_eq!(meeting_link("https://notzoom.us.example.com/j/1"), None);
        assert_eq!(meeting_link("no link here"), None);
    }

    #[test]
    fn something_that_is_not_a_feed_is_an_error() {
        assert!(events("<html>Sign in</html>", 0, DAY).is_err());
    }

    /// Reads a real feed, whose address is given in `MADOW_TEST_ICAL`, so it only runs on
    /// demand: `cargo test -- --ignored real_feed`. Prints a count, never contents.
    #[tokio::test]
    #[ignore]
    async fn real_feed_syncs() {
        let address = std::env::var("MADOW_TEST_ICAL").expect("MADOW_TEST_ICAL");
        let items = Ical::new("real".into(), Some(address))
            .sync()
            .await
            .unwrap();
        let ids: HashSet<_> = items.iter().map(|i| &i.external_id).collect();
        println!("events in the window: {}", items.len());
        assert_eq!(ids.len(), items.len(), "every occurrence has its own id");
    }
}
