//! What is playing on the machine, for the status bar, and the three buttons that go with it.
//! Windows asks the system's media session, Linux the players that speak MPRIS. macOS tells
//! nothing: it keeps what is playing to its own applications.

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize)]
pub struct Track {
    pub title: String,
    /// Empty when the player does not tell.
    pub artist: String,
    /// False while paused or stopped.
    pub playing: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Toggle,
    Next,
    Previous,
}

/// What the player in front is on; `None` when nothing is, or when the system does not tell.
pub fn track() -> Option<Track> {
    player::track()
}

/// Asks the player in front; a player that cannot do it (no next track, say) does nothing.
pub fn act(action: Action) {
    player::act(action);
}

#[cfg(windows)]
mod player {
    use super::{Action, Track};
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    use windows::core::Result;

    /// The session Windows itself puts in front, the one its own volume panel shows.
    fn session() -> Result<Session> {
        Manager::RequestAsync()?.join()?.GetCurrentSession()
    }

    fn read(session: &Session) -> Result<Track> {
        let properties = session.TryGetMediaPropertiesAsync()?.join()?;
        Ok(Track {
            title: properties.Title()?.to_string(),
            artist: properties.Artist()?.to_string(),
            playing: session.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing,
        })
    }

    pub fn track() -> Option<Track> {
        let track = read(&session().ok()?).ok()?;
        (!track.title.is_empty()).then_some(track)
    }

    pub fn act(action: Action) {
        let Ok(session) = session() else { return };
        let asked = match action {
            Action::Toggle => session.TryTogglePlayPauseAsync(),
            Action::Next => session.TrySkipNextAsync(),
            Action::Previous => session.TrySkipPreviousAsync(),
        };
        // Waited for, so that the reading that follows the click sees the change.
        let _ = asked.and_then(|asked| asked.join());
    }
}

#[cfg(target_os = "linux")]
mod player {
    use super::{Action, Track};
    use std::collections::HashMap;
    use zbus::{
        blocking::{Connection, Proxy, fdo::DBusProxy},
        zvariant::OwnedValue,
    };

    type Player = Proxy<'static>;

    fn read(player: &Player) -> Option<Track> {
        let status: String = player.get_property("PlaybackStatus").ok()?;
        let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata").ok()?;
        translate(&status, metadata)
    }

    /// A track from what MPRIS tells: `xesam:title` is a text, `xesam:artist` a list of them.
    pub(super) fn translate(
        status: &str,
        mut metadata: HashMap<String, OwnedValue>,
    ) -> Option<Track> {
        let title = String::try_from(metadata.remove("xesam:title")?).ok()?;
        let artists = metadata
            .remove("xesam:artist")
            .and_then(|artists| Vec::<String>::try_from(artists).ok())
            .unwrap_or_default();
        (!title.is_empty()).then(|| Track {
            title,
            artist: artists.join(", "),
            playing: status == "Playing",
        })
    }

    /// The player in front: the one that plays, else the first that has a track.
    fn front() -> Option<(Player, Track)> {
        let connection = Connection::session().ok()?;
        let names = DBusProxy::new(&connection).ok()?.list_names().ok()?;
        let mut players = names
            .iter()
            .filter(|name| name.as_str().starts_with("org.mpris.MediaPlayer2."))
            .filter_map(|name| {
                let player = Proxy::new(
                    &connection,
                    name.to_owned(),
                    "/org/mpris/MediaPlayer2",
                    "org.mpris.MediaPlayer2.Player",
                )
                .ok()?;
                let track = read(&player)?;
                Some((player, track))
            })
            .collect::<Vec<_>>();
        let front = players.iter().position(|(_, track)| track.playing);
        (!players.is_empty()).then(|| players.swap_remove(front.unwrap_or(0)))
    }

    pub fn track() -> Option<Track> {
        front().map(|(_, track)| track)
    }

    pub fn act(action: Action) {
        let Some((player, _)) = front() else { return };
        let method = match action {
            Action::Toggle => "PlayPause",
            Action::Next => "Next",
            Action::Previous => "Previous",
        };
        let _ = player.call_method(method, &());
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod player {
    use super::{Action, Track};

    pub fn track() -> Option<Track> {
        None
    }

    pub fn act(_: Action) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asks the machine the tests run on. What is playing is nobody else's business: only
    /// whether something is gets printed.
    #[test]
    fn asking_the_machine_never_fails() {
        let track = track();
        if let Some(track) = &track {
            assert!(!track.title.is_empty());
        }
        println!("something playing: {}", track.is_some());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_track_is_read_from_what_mpris_tells() {
        use std::collections::HashMap;
        use zbus::zvariant::{OwnedValue, Value};

        let metadata = |title: &str, artists: Vec<&str>| {
            let own = |value: Value| OwnedValue::try_from(value).unwrap();
            HashMap::from([
                ("xesam:title".to_owned(), own(Value::from(title))),
                ("xesam:artist".to_owned(), own(Value::from(artists))),
            ])
        };
        let track = player::translate("Playing", metadata("Nocturne", vec!["Ada", "Lin"]));
        let expected = Track {
            title: "Nocturne".into(),
            artist: "Ada, Lin".into(),
            playing: true,
        };
        assert_eq!(track, Some(expected));

        let paused = player::translate("Paused", metadata("Nocturne", vec![])).unwrap();
        assert_eq!((paused.artist.as_str(), paused.playing), ("", false));
        assert_eq!(player::translate("Playing", metadata("", vec![])), None);
        assert_eq!(player::translate("Playing", HashMap::new()), None);
    }
}
