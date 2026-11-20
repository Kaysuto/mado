//! The shortcut that calls Madow and its launcher from anywhere on the machine.

use crate::{
    error::{Error, Result},
    settings,
};
use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// The setting that keeps the shortcut. An empty text means the user wants none.
pub const SHORTCUT: &str = "launcher.shortcut";
const DEFAULT: &str = "Alt+Space";
/// Sent to the interface when the shortcut is pressed.
const CALLED: &str = "launcher-called";

/// The shortcut the user chose: the usual one until they choose, none once they emptied it.
pub fn chosen(conn: &Connection) -> Result<String> {
    Ok(settings::get(conn, SHORTCUT)?
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| DEFAULT.into()))
}

/// Makes `shortcut` the only one Madow listens to. Fails when it cannot be read, or when
/// another application already holds it.
pub fn bind(app: &AppHandle, shortcut: &str) -> Result<()> {
    let shortcuts = app.global_shortcut();
    shortcuts.unregister_all().map_err(refused)?;
    if shortcut.is_empty() {
        return Ok(());
    }
    shortcuts
        .on_shortcut(shortcut, |app, _, event| {
            if event.state == ShortcutState::Pressed {
                bring_forward(app);
                let _ = app.emit(CALLED, ());
            }
        })
        .map_err(refused)
}

/// Whether Madow holds `shortcut` right now.
pub fn bound(app: &AppHandle, shortcut: &str) -> bool {
    !shortcut.is_empty() && app.global_shortcut().is_registered(shortcut)
}

/// Puts Madow's window in front of the others, wherever it was.
pub fn bring_forward(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn refused(cause: tauri_plugin_global_shortcut::Error) -> Error {
    Error::Message(format!("This shortcut cannot be used: {cause}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use serde_json::json;

    #[test]
    fn the_shortcut_is_the_usual_one_until_the_user_chooses_or_empties_it() {
        let conn = open_in_memory();
        assert_eq!(chosen(&conn).unwrap(), "Alt+Space");
        settings::set(&conn, SHORTCUT, &json!("Ctrl+Shift+K")).unwrap();
        assert_eq!(chosen(&conn).unwrap(), "Ctrl+Shift+K");
        settings::set(&conn, SHORTCUT, &json!("")).unwrap();
        assert_eq!(chosen(&conn).unwrap(), "");
    }
}
