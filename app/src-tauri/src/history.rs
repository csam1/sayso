//! Every dictation, kept in one local SQLite file (`<app_data_dir>/history.db`) for the dashboard.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

const DASHBOARD_LABEL: &str = "dashboard";
/// Emitted whenever entries are added or removed, so an open dashboard refreshes.
pub const HISTORY_CHANGED: &str = "history-changed";

/// Settings key; `"false"` turns history off, anything else (or unset) keeps it on.
pub const SAVE_HISTORY: &str = "save_history";
const WEEK_MS: i64 = 7 * 86_400_000;
/// Typing speed used for "minutes saved".
const TYPING_WPM: f64 = 40.0;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS entries (
        id INTEGER PRIMARY KEY,
        text TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        duration_ms INTEGER NOT NULL,
        word_count INTEGER NOT NULL,
        app_name TEXT
    );
    CREATE INDEX IF NOT EXISTS entries_created_at ON entries (created_at);
    CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
";

/// A dictation about to be saved. Times are Unix milliseconds.
pub struct NewEntry {
    pub text: String,
    pub created_at: i64,
    pub duration_ms: i64,
    pub app_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Entry {
    pub id: i64,
    pub text: String,
    pub created_at: i64,
    pub duration_ms: i64,
    pub word_count: i64,
    pub app_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub total_entries: i64,
    pub total_words: i64,
    pub words_this_week: i64,
    pub minutes_saved: i64,
}

pub struct HistoryStore {
    conn: Connection,
}

impl HistoryStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("failed to create history dir: {e}"))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("failed to open history: {e}"))?;
        Self::with_schema(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| format!("failed to open history: {e}"))?;
        Self::with_schema(conn)
    }

    fn with_schema(conn: Connection) -> Result<Self, String> {
        conn.execute_batch(SCHEMA).map_err(|e| format!("failed to set up history: {e}"))?;
        Ok(Self { conn })
    }

    pub fn insert(&self, entry: &NewEntry) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO entries (text, created_at, duration_ms, word_count, app_name) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![entry.text, entry.created_at, entry.duration_ms, word_count(&entry.text), entry.app_name],
            )
            .map_err(|e| format!("failed to save to history: {e}"))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Newest first. An empty `query` lists everything; otherwise a case-insensitive substring match.
    pub fn list(&self, query: &str, limit: i64, offset: i64) -> Result<Vec<Entry>, String> {
        let pattern = format!("%{}%", escape_like(query));
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, text, created_at, duration_ms, word_count, app_name FROM entries
                 WHERE text LIKE ?1 ESCAPE '\\'
                 ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3",
            )
            .map_err(|e| format!("failed to read history: {e}"))?;
        let rows = stmt
            .query_map(params![pattern, limit, offset], |row| {
                Ok(Entry {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    created_at: row.get(2)?,
                    duration_ms: row.get(3)?,
                    word_count: row.get(4)?,
                    app_name: row.get(5)?,
                })
            })
            .map_err(|e| format!("failed to read history: {e}"))?;
        rows.collect::<Result<_, _>>().map_err(|e| format!("failed to read history: {e}"))
    }

    pub fn delete(&self, id: i64) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM entries WHERE id = ?1", [id])
            .map(|_| ())
            .map_err(|e| format!("failed to delete from history: {e}"))
    }

    pub fn clear(&self) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM entries", [])
            .map(|_| ())
            .map_err(|e| format!("failed to clear history: {e}"))
    }

    pub fn stats(&self, now_ms: i64) -> Result<Stats, String> {
        let (total_entries, total_words, words_this_week, total_duration_ms): (i64, i64, i64, i64) = self
            .conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(word_count), 0),
                        COALESCE(SUM(CASE WHEN created_at >= ?1 THEN word_count ELSE 0 END), 0),
                        COALESCE(SUM(duration_ms), 0)
                 FROM entries",
                [now_ms - WEEK_MS],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(|e| format!("failed to read history stats: {e}"))?;
        let saved = total_words as f64 / TYPING_WPM - total_duration_ms as f64 / 60_000.0;
        Ok(Stats { total_entries, total_words, words_this_week, minutes_saved: saved.max(0.0).floor() as i64 })
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| row.get(0))
            .optional()
            .map_err(|e| format!("failed to read setting: {e}"))
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [key, value],
            )
            .map(|_| ())
            .map_err(|e| format!("failed to save setting: {e}"))
    }
}

/// The store, or `None` when the database couldn't be opened (dictation keeps working without it).
pub struct HistoryState(pub Mutex<Option<HistoryStore>>);

/// Saves a finished dictation. Returns `Ok(false)` without writing when the text is blank
/// or the user turned history off.
pub fn record(
    store: &HistoryStore,
    text: &str,
    created_at: i64,
    duration_ms: i64,
    app_name: Option<String>,
) -> Result<bool, String> {
    if text.trim().is_empty() || !save_history_enabled(store)? {
        return Ok(false);
    }
    store.insert(&NewEntry { text: text.to_string(), created_at, duration_ms, app_name })?;
    Ok(true)
}

pub fn save_history_enabled(store: &HistoryStore) -> Result<bool, String> {
    Ok(store.get_setting(SAVE_HISTORY)?.as_deref() != Some("false"))
}

/// Runs `f` against the open store, or fails with a message the dashboard can show.
pub fn with_store<T>(
    state: &HistoryState,
    f: impl FnOnce(&HistoryStore) -> Result<T, String>,
) -> Result<T, String> {
    let guard = state.0.lock().unwrap();
    f(guard.as_ref().ok_or("History is unavailable")?)
}

pub fn show_dashboard(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(DASHBOARD_LABEL) {
        window.show().ok();
        window.set_focus().ok();
    }
}

#[tauri::command]
pub fn history_list(
    state: State<HistoryState>,
    query: String,
    limit: i64,
    offset: i64,
) -> Result<Vec<Entry>, String> {
    with_store(&state, |s| s.list(&query, limit, offset))
}

#[tauri::command]
pub fn history_delete(app: AppHandle, state: State<HistoryState>, id: i64) -> Result<(), String> {
    with_store(&state, |s| s.delete(id))?;
    app.emit(HISTORY_CHANGED, ()).ok();
    Ok(())
}

#[tauri::command]
pub fn history_clear(app: AppHandle, state: State<HistoryState>) -> Result<(), String> {
    with_store(&state, |s| s.clear())?;
    app.emit(HISTORY_CHANGED, ()).ok();
    Ok(())
}

#[tauri::command]
pub fn history_stats(state: State<HistoryState>) -> Result<Stats, String> {
    with_store(&state, |s| s.stats(chrono::Utc::now().timestamp_millis()))
}

#[tauri::command]
pub fn get_save_history(state: State<HistoryState>) -> Result<bool, String> {
    with_store(&state, save_history_enabled)
}

#[tauri::command]
pub fn set_save_history(state: State<HistoryState>, enabled: bool) -> Result<(), String> {
    with_store(&state, |s| s.set_setting(SAVE_HISTORY, if enabled { "true" } else { "false" }))
}

/// Words separated by whitespace, not counting the `-` that starts a bullet.
pub fn word_count(text: &str) -> i64 {
    text.split_whitespace().filter(|w| !w.chars().all(|c| c == '-')).count() as i64
}

fn escape_like(query: &str) -> String {
    query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    const DAY_MS: i64 = 86_400_000;

    fn entry(text: &str, created_at: i64) -> NewEntry {
        NewEntry { text: text.into(), created_at, duration_ms: 3_000, app_name: Some("Notes".into()) }
    }

    fn texts(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.text.as_str()).collect()
    }

    fn fresh_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sayso-history-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn insert_then_list_returns_newest_first() {
        let s = HistoryStore::open_in_memory().unwrap();
        s.insert(&entry("first", 1_000)).unwrap();
        s.insert(&entry("second", 2_000)).unwrap();
        let all = s.list("", 50, 0).unwrap();
        assert_eq!(texts(&all), ["second", "first"]);
        assert_eq!(all[0].app_name.as_deref(), Some("Notes"));
        assert_eq!(all[0].duration_ms, 3_000);
    }

    #[test]
    fn list_paginates() {
        let s = HistoryStore::open_in_memory().unwrap();
        for i in 0..3 {
            s.insert(&entry(&format!("n{i}"), i)).unwrap();
        }
        assert_eq!(texts(&s.list("", 2, 0).unwrap()), ["n2", "n1"]);
        assert_eq!(texts(&s.list("", 2, 2).unwrap()), ["n0"]);
    }

    #[test]
    fn search_is_case_insensitive_substring() {
        let s = HistoryStore::open_in_memory().unwrap();
        s.insert(&entry("Call the bank", 1)).unwrap();
        s.insert(&entry("Buy milk", 2)).unwrap();
        assert_eq!(texts(&s.list("BANK", 50, 0).unwrap()), ["Call the bank"]);
    }

    #[test]
    fn search_treats_wildcards_and_quotes_literally() {
        let s = HistoryStore::open_in_memory().unwrap();
        for (i, text) in ["100% done", "a_b", "it's fine", "back\\slash", "plain"].iter().enumerate() {
            s.insert(&entry(text, i as i64)).unwrap();
        }
        assert_eq!(texts(&s.list("%", 50, 0).unwrap()), ["100% done"]);
        assert_eq!(texts(&s.list("_", 50, 0).unwrap()), ["a_b"]);
        assert_eq!(texts(&s.list("'", 50, 0).unwrap()), ["it's fine"]);
        assert_eq!(texts(&s.list("\\", 50, 0).unwrap()), ["back\\slash"]);
    }

    #[test]
    fn multiline_and_unicode_text_round_trips() {
        let s = HistoryStore::open_in_memory().unwrap();
        s.insert(&entry("- Buy milk\n- Call the bank", 1)).unwrap();
        s.insert(&entry("नमस्ते, café", 2)).unwrap();
        assert_eq!(texts(&s.list("", 50, 0).unwrap()), ["नमस्ते, café", "- Buy milk\n- Call the bank"]);
    }

    #[test]
    fn word_count_ignores_bullet_dashes() {
        assert_eq!(word_count("- Buy milk\n- Call the bank"), 5);
        assert_eq!(word_count("नमस्ते दुनिया"), 2);
        assert_eq!(word_count("  "), 0);
    }

    #[test]
    fn insert_stores_word_count() {
        let s = HistoryStore::open_in_memory().unwrap();
        s.insert(&entry("- Buy milk\n- Call the bank", 1)).unwrap();
        assert_eq!(s.list("", 1, 0).unwrap()[0].word_count, 5);
    }

    #[test]
    fn delete_removes_one_and_clear_keeps_settings() {
        let s = HistoryStore::open_in_memory().unwrap();
        let first = s.insert(&entry("first", 1)).unwrap();
        s.insert(&entry("second", 2)).unwrap();
        s.insert(&entry("third", 3)).unwrap();
        s.set_setting("save_history", "false").unwrap();

        s.delete(first).unwrap();
        assert_eq!(texts(&s.list("", 50, 0).unwrap()), ["third", "second"]);

        s.clear().unwrap();
        assert!(s.list("", 50, 0).unwrap().is_empty());
        assert_eq!(s.get_setting("save_history").unwrap().as_deref(), Some("false"));
    }

    #[test]
    fn stats_counts_words_this_week_and_time_saved() {
        let s = HistoryStore::open_in_memory().unwrap();
        let now = 30 * DAY_MS;
        let words = |n: usize| "word ".repeat(n);
        s.insert(&NewEntry { text: words(400), created_at: now - DAY_MS, duration_ms: 120_000, app_name: None })
            .unwrap();
        s.insert(&NewEntry { text: words(40), created_at: now - 10 * DAY_MS, duration_ms: 30_000, app_name: None })
            .unwrap();

        let stats = s.stats(now).unwrap();
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.total_words, 440);
        assert_eq!(stats.words_this_week, 400);
        assert_eq!(stats.minutes_saved, 8); // (10 − 2) + (1 − 0.5) = 8.5
    }

    #[test]
    fn stats_on_empty_history_are_zero_and_never_negative() {
        let s = HistoryStore::open_in_memory().unwrap();
        let stats = s.stats(DAY_MS).unwrap();
        assert_eq!((stats.total_entries, stats.total_words, stats.words_this_week, stats.minutes_saved), (0, 0, 0, 0));

        // Slow speech: 4 words over 10 minutes would be negative time saved.
        s.insert(&NewEntry { text: "one two three four".into(), created_at: 1, duration_ms: 600_000, app_name: None })
            .unwrap();
        assert_eq!(s.stats(DAY_MS).unwrap().minutes_saved, 0);
    }

    #[test]
    fn settings_default_to_none_and_overwrite() {
        let s = HistoryStore::open_in_memory().unwrap();
        assert_eq!(s.get_setting("save_history").unwrap(), None);
        s.set_setting("save_history", "false").unwrap();
        s.set_setting("save_history", "true").unwrap();
        assert_eq!(s.get_setting("save_history").unwrap().as_deref(), Some("true"));
    }

    #[test]
    fn open_creates_parent_dirs_and_reopens_existing_data() {
        let dir = fresh_dir("reopen");
        let path = dir.join("nested").join("history.db");
        {
            let s = HistoryStore::open(&path).unwrap();
            s.insert(&entry("kept", 1)).unwrap();
        }
        let s = HistoryStore::open(&path).unwrap();
        assert_eq!(texts(&s.list("", 50, 0).unwrap()), ["kept"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn record_skips_blank_text() {
        let s = HistoryStore::open_in_memory().unwrap();
        assert!(!record(&s, "  \n", 1, 1, None).unwrap());
        assert!(s.list("", 10, 0).unwrap().is_empty());
    }

    #[test]
    fn record_respects_save_history_off_and_keeps_old_entries() {
        let s = HistoryStore::open_in_memory().unwrap();
        record(&s, "kept", 1, 1, None).unwrap();
        s.set_setting(SAVE_HISTORY, "false").unwrap();
        assert!(!record(&s, "dropped", 2, 1, None).unwrap());
        assert_eq!(texts(&s.list("", 10, 0).unwrap()), ["kept"]);
    }

    #[test]
    fn record_stores_when_setting_missing_or_true() {
        let s = HistoryStore::open_in_memory().unwrap();
        assert!(record(&s, "first", 1, 2_500, Some("Slack".into())).unwrap());
        s.set_setting(SAVE_HISTORY, "true").unwrap();
        assert!(record(&s, "second", 2, 1, None).unwrap());
        let all = s.list("", 10, 0).unwrap();
        assert_eq!(texts(&all), ["second", "first"]);
        assert_eq!((all[1].duration_ms, all[1].app_name.as_deref()), (2_500, Some("Slack")));
    }

    #[test]
    fn with_store_reports_unavailable_history() {
        let state = HistoryState(Mutex::new(None));
        assert_eq!(with_store(&state, |s| s.stats(0)).err().as_deref(), Some("History is unavailable"));
    }

    #[test]
    fn with_store_runs_against_the_open_store() {
        let state = HistoryState(Mutex::new(Some(HistoryStore::open_in_memory().unwrap())));
        with_store(&state, |s| s.insert(&entry("hi", 1))).unwrap();
        assert_eq!(with_store(&state, |s| s.stats(0)).unwrap().total_entries, 1);
    }

    #[test]
    fn save_history_defaults_on_and_reads_back() {
        let s = HistoryStore::open_in_memory().unwrap();
        assert!(save_history_enabled(&s).unwrap());
        s.set_setting(SAVE_HISTORY, "false").unwrap();
        assert!(!save_history_enabled(&s).unwrap());
    }

    #[test]
    fn open_on_a_directory_path_errors() {
        let dir = fresh_dir("is-dir");
        fs::create_dir_all(&dir).unwrap();
        assert!(HistoryStore::open(&dir).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
