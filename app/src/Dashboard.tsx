import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./Dashboard.css";

type Entry = {
  id: number;
  text: string;
  created_at: number;
  duration_ms: number;
  word_count: number;
  app_name: string | null;
};

type Stats = {
  total_entries: number;
  total_words: number;
  words_this_week: number;
  minutes_saved: number;
};

type Section = "home" | "history" | "settings";

const PAGE_SIZE = 50;
// The webview's window.confirm() returns false without showing a dialog, so Clear all asks twice.
const CLEAR_CONFIRM_MS = 3000;
const SEARCH_DEBOUNCE_MS = 250;

const timeFormat = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const dateFormat = new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric" });

function dayLabel(ms: number): string {
  const day = new Date(ms).toDateString();
  const today = new Date();
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (day === today.toDateString()) return "Today";
  if (day === yesterday.toDateString()) return "Yesterday";
  return dateFormat.format(ms);
}

function groupByDay(entries: Entry[]): [string, Entry[]][] {
  const groups = new Map<string, Entry[]>();
  for (const entry of entries) {
    const label = dayLabel(entry.created_at);
    groups.set(label, [...(groups.get(label) ?? []), entry]);
  }
  return [...groups];
}

function message(error: unknown): string {
  return typeof error === "string" ? error : String(error);
}

type EntryRowProps = { entry: Entry; onDelete?: (id: number) => void };

function EntryRow({ entry, onDelete }: EntryRowProps) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    await navigator.clipboard.writeText(entry.text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <li className="entry">
      <div className="entry-meta">
        <span>{timeFormat.format(entry.created_at)}</span>
        {entry.app_name && <span>· {entry.app_name}</span>}
      </div>
      <p className="entry-text">{entry.text}</p>
      <div className="entry-actions">
        <button type="button" onClick={copy}>
          {copied ? "Copied" : "Copy"}
        </button>
        {onDelete && (
          <button type="button" className="danger" onClick={() => onDelete(entry.id)}>
            Delete
          </button>
        )}
      </div>
    </li>
  );
}

const EMPTY_HISTORY = "Your dictations will appear here. Press ⌘⇧Space to start.";

function Home({ stats, recent }: { stats: Stats | null; recent: Entry[] }) {
  const tiles: [string, number | undefined][] = [
    ["Dictations", stats?.total_entries],
    ["Words", stats?.total_words],
    ["Words this week", stats?.words_this_week],
    ["Minutes saved", stats?.minutes_saved],
  ];
  return (
    <>
      <h1>Home</h1>
      <div className="tiles">
        {tiles.map(([label, value]) => (
          <div key={label} className="tile">
            <div className="tile-value">{value?.toLocaleString() ?? "–"}</div>
            <div className="tile-label">{label}</div>
          </div>
        ))}
      </div>
      <p className="hint">Minutes saved compares your speaking time with typing at 40 words per minute.</p>
      <h2>Recent</h2>
      {recent.length === 0 ? (
        <p className="empty">{EMPTY_HISTORY}</p>
      ) : (
        <ul className="entries">
          {recent.map((entry) => (
            <EntryRow key={entry.id} entry={entry} />
          ))}
        </ul>
      )}
    </>
  );
}

type HistoryProps = {
  entries: Entry[];
  query: string;
  hasMore: boolean;
  onQuery: (query: string) => void;
  onLoadMore: () => void;
  onDelete: (id: number) => void;
  onClear: () => void;
};

function History({ entries, query, hasMore, onQuery, onLoadMore, onDelete, onClear }: HistoryProps) {
  const [confirmingClear, setConfirmingClear] = useState(false);

  useEffect(() => {
    if (!confirmingClear) return;
    const timer = setTimeout(() => setConfirmingClear(false), CLEAR_CONFIRM_MS);
    return () => clearTimeout(timer);
  }, [confirmingClear]);

  const clear = () => {
    if (confirmingClear) {
      setConfirmingClear(false);
      onClear();
    } else {
      setConfirmingClear(true);
    }
  };

  return (
    <>
      <div className="section-header">
        <h1>History</h1>
        <button type="button" className="danger" onClick={clear} disabled={entries.length === 0 && !query}>
          {confirmingClear ? "Click again to delete everything" : "Clear all"}
        </button>
      </div>
      <input
        className="search"
        type="search"
        placeholder="Search dictations"
        value={query}
        onChange={(event) => onQuery(event.target.value)}
      />
      {entries.length === 0 ? (
        <p className="empty">{query ? `No matches for “${query}”` : EMPTY_HISTORY}</p>
      ) : (
        groupByDay(entries).map(([day, dayEntries]) => (
          <section key={day}>
            <h2>{day}</h2>
            <ul className="entries">
              {dayEntries.map((entry) => (
                <EntryRow key={entry.id} entry={entry} onDelete={onDelete} />
              ))}
            </ul>
          </section>
        ))
      )}
      {hasMore && (
        <button type="button" className="load-more" onClick={onLoadMore}>
          Load more
        </button>
      )}
    </>
  );
}

function Settings({ saveHistory, onToggle }: { saveHistory: boolean | null; onToggle: (on: boolean) => void }) {
  return (
    <>
      <h1>Settings</h1>
      <ul className="settings">
        <li>
          <label className="setting-row">
            <div>
              <div className="setting-name">Save dictation history</div>
              <div className="setting-note">Stored only on this Mac.</div>
            </div>
            <input
              type="checkbox"
              checked={saveHistory ?? false}
              disabled={saveHistory === null}
              onChange={(event) => onToggle(event.target.checked)}
            />
          </label>
        </li>
        <li className="setting-row">
          <div className="setting-name">Shortcut</div>
          <kbd>⌘⇧Space</kbd>
        </li>
        <li className="setting-row">
          <div className="setting-name">Speech model</div>
          <span className="setting-note">Whisper base (English)</span>
        </li>
        <li className="setting-row">
          <div className="setting-name">Welcome tour</div>
          <button type="button" onClick={() => invoke("open_welcome")}>
            Show again
          </button>
        </li>
      </ul>
    </>
  );
}

function Dashboard() {
  const [section, setSection] = useState<Section>("home");
  const [stats, setStats] = useState<Stats | null>(null);
  const [recent, setRecent] = useState<Entry[]>([]);
  const [entries, setEntries] = useState<Entry[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [saveHistory, setSaveHistory] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(async (task: () => Promise<void>) => {
    try {
      await task();
      setError(null);
    } catch (err) {
      setError(message(err));
    }
  }, []);

  // Reloads everything currently shown, keeping as many history rows as are loaded.
  const refresh = useCallback(
    (rows = PAGE_SIZE) =>
      run(async () => {
        const [nextStats, nextRecent, nextEntries, nextSave] = await Promise.all([
          invoke<Stats>("history_stats"),
          invoke<Entry[]>("history_list", { query: "", limit: 5, offset: 0 }),
          invoke<Entry[]>("history_list", { query: debouncedQuery, limit: rows + 1, offset: 0 }),
          invoke<boolean>("get_save_history"),
        ]);
        setStats(nextStats);
        setRecent(nextRecent);
        setEntries(nextEntries.slice(0, rows));
        setHasMore(nextEntries.length > rows);
        setSaveHistory(nextSave);
      }),
    [run, debouncedQuery],
  );

  useEffect(() => {
    const timer = setTimeout(() => setDebouncedQuery(query), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [query]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const loaded = Math.max(entries.length, PAGE_SIZE);
  useEffect(() => {
    const unlisten = listen("history-changed", () => refresh(loaded));
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refresh, loaded]);

  const deleteEntry = (id: number) => run(() => invoke("history_delete", { id }));
  const clearAll = () => run(() => invoke("history_clear"));
  const toggleSave = (enabled: boolean) =>
    run(async () => {
      await invoke("set_save_history", { enabled });
      setSaveHistory(enabled);
    });

  const nav: [Section, string][] = [
    ["home", "Home"],
    ["history", "History"],
    ["settings", "Settings"],
  ];

  return (
    <div className="dashboard">
      <nav className="sidebar">
        <div className="brand">Sayso</div>
        {nav.map(([id, label]) => (
          <button
            key={id}
            type="button"
            className={section === id ? "nav-item active" : "nav-item"}
            onClick={() => setSection(id)}
          >
            {label}
          </button>
        ))}
      </nav>
      <main className="content">
        {error && <div className="error-banner">{error}</div>}
        {section === "home" && <Home stats={stats} recent={recent} />}
        {section === "history" && (
          <History
            entries={entries}
            query={query}
            hasMore={hasMore}
            onQuery={setQuery}
            onLoadMore={() => refresh(entries.length + PAGE_SIZE)}
            onDelete={deleteEntry}
            onClear={clearAll}
          />
        )}
        {section === "settings" && <Settings saveHistory={saveHistory} onToggle={toggleSave} />}
      </main>
    </div>
  );
}

export default Dashboard;
