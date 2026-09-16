use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Instant,
};

use rusqlite::types::Value;

use super::*;

const TABLES: [&str; 6] = [
    "threads",
    "turns",
    "current_thread",
    "query_statistics",
    "provider_health",
    "history_expiry",
];

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A database path whose `data` directory does not exist yet.
fn scratch() -> PathBuf {
    let id = NEXT.fetch_add(1, Ordering::SeqCst);
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/store-tests")
        .join(format!("{}-{id}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    directory.join("data").join("ask.sqlite3")
}

fn target(model: &str) -> Target {
    Target {
        profile: "default".to_string(),
        kind: "openai-compatible".to_string(),
        base_url: "http://127.0.0.1:1/v1".to_string(),
        api_key_env: "LOCAL_API_KEY".to_string(),
        timeout_ms: 41,
        model: model.to_string(),
        system_prompt: "Be brief.".to_string(),
        max_output_tokens: None,
    }
}

fn complete<'a>(prompt: &'a str, answer: &'a str) -> Option<Turn<'a>> {
    Some(Turn {
        prompt,
        answer,
        status: TurnStatus::Complete,
    })
}

fn partial<'a>(prompt: &'a str, answer: &'a str) -> Option<Turn<'a>> {
    Some(Turn {
        prompt,
        answer,
        status: TurnStatus::Partial("provider request failed: cut".to_string()),
    })
}

fn record<'a>(target: &'a Target, thread: Option<i64>, turn: Option<Turn<'a>>) -> Record<'a> {
    let (outcome, health) = match &turn {
        Some(Turn {
            status: TurnStatus::Complete,
            ..
        }) => ("complete", Health::Success),
        Some(_) => ("partial", Health::Failure("provider")),
        None => ("failed", Health::Failure("provider")),
    };
    Record {
        started_at: SystemTime::now(),
        target,
        thread,
        turn,
        measurement: Measurement {
            command: if thread.is_some() { "reply" } else { "new" },
            outcome,
            error_class: (outcome != "complete").then_some("provider"),
            wall: Duration::from_millis(30),
            api: Duration::from_millis(20),
            first_token: Some(Duration::from_millis(10)),
            usage: Some(Usage {
                input: 12,
                output: 3,
            }),
        },
        health: Some(health),
    }
}

fn scalar<T: rusqlite::types::FromSql>(store: &Store, sql: &str) -> T {
    store
        .connection
        .query_row(sql, [], |row| row.get(0))
        .unwrap()
}

fn counts(store: &Store) -> Vec<i64> {
    TABLES
        .iter()
        .map(|table| scalar(store, &format!("SELECT count(*) FROM {table}")))
        .collect()
}

fn current_id(store: &mut Store) -> Option<i64> {
    store.current().unwrap().map(|thread| thread.id)
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// The permission bits of the database file and its directory.
fn modes(path: &Path) -> (u32, u32) {
    (mode(path), mode(path.parent().unwrap()))
}

/// The connection settings `Store::open` applies: user_version, journal_mode,
/// synchronous, foreign_keys, and busy_timeout.
fn pragmas(store: &Store) -> (i64, String, i64, i64, i64) {
    (
        scalar(store, "PRAGMA user_version"),
        scalar(store, "PRAGMA journal_mode"),
        scalar(store, "PRAGMA synchronous"),
        scalar(store, "PRAGMA foreign_keys"),
        scalar(store, "PRAGMA busy_timeout"),
    )
}

#[test]
fn creates_the_schema_with_owner_only_permissions() {
    let path = scratch();
    let store = Store::open(&path).unwrap();
    assert_eq!(pragmas(&store), (3, "delete".to_string(), 2, 1, 1_000));
    assert_eq!(counts(&store), vec![0; 6]);
    assert_eq!(modes(&path), (0o600, 0o700));
}

#[test]
fn existing_permissions_are_left_alone() {
    let path = scratch();
    let directory = path.parent().unwrap();
    fs::create_dir(directory).unwrap();
    fs::set_permissions(directory, fs::Permissions::from_mode(0o750)).unwrap();
    fs::write(&path, b"").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    Store::open(&path).unwrap();
    assert_eq!(modes(&path), (0o640, 0o750));
}

#[test]
fn newer_and_foreign_databases_are_refused_without_writing() {
    for (setup, expected) in [
        ("PRAGMA user_version = 4;", "schema version 4 is newer"),
        (
            "PRAGMA user_version = -1;",
            "unrecognized schema version -1",
        ),
        ("CREATE TABLE other (x);", "not empty and has no ask schema"),
    ] {
        let path = scratch();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        Connection::open(&path)
            .unwrap()
            .execute_batch(&format!("CREATE TABLE marker (x); {setup}"))
            .unwrap();
        let before = fs::read(&path).unwrap();
        let message = Store::open(&path).err().unwrap().to_string();
        assert!(message.contains(expected), "{message}");
        assert!(message.starts_with("cannot open history database '"));
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn simultaneous_first_runs_create_the_schema_once() {
    for _ in 0..8 {
        let path = scratch();
        let barrier = Arc::new(Barrier::new(4));
        let openers: Vec<_> = (0..4)
            .map(|_| {
                let (path, barrier) = (path.clone(), Arc::clone(&barrier));
                thread::spawn(move || {
                    barrier.wait();
                    Store::open(&path).map(|_| ())
                })
            })
            .collect();
        for opener in openers {
            opener.join().unwrap().unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(scalar::<i64>(&store, "PRAGMA user_version"), 3);
        assert_eq!(counts(&store), vec![0; 6]);
    }
}

#[test]
fn a_first_run_that_loses_the_race_keeps_the_winners_schema() {
    let path = scratch();
    prepare_file(&path).unwrap();
    let mut loser = Connection::open(&path).unwrap();
    assert_eq!(user_version(&loser).unwrap(), 0);
    let mut winner = Store::open(&path).unwrap();
    let snapshot = target("model");
    winner
        .record(&record(&snapshot, None, complete("q", "a")))
        .unwrap();
    create_schema(&mut loser).unwrap();
    assert_eq!(counts(&winner), vec![1, 1, 1, 1, 1, 0]);
}

/// The version 1 schema and one thread with a complete and a partial turn,
/// as `ask` wrote them before profiles could set `max_output_tokens`.
const VERSION_1: &str = "
CREATE TABLE threads (id INTEGER PRIMARY KEY, created_at_ms INTEGER NOT NULL, profile TEXT NOT NULL, provider_kind TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL, system_prompt TEXT NOT NULL, timeout_ms INTEGER NOT NULL CHECK (timeout_ms > 0), api_key_env TEXT NOT NULL);
CREATE TABLE turns (id INTEGER PRIMARY KEY, thread_id INTEGER NOT NULL REFERENCES threads (id) ON DELETE CASCADE, ordinal INTEGER NOT NULL CHECK (ordinal > 0), created_at_ms INTEGER NOT NULL, prompt TEXT NOT NULL, answer TEXT NOT NULL, status TEXT NOT NULL CHECK (status IN ('complete', 'partial')), reason TEXT CHECK ((status = 'complete') = (reason IS NULL)), UNIQUE (thread_id, ordinal));
CREATE TABLE current_thread (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), thread_id INTEGER NOT NULL REFERENCES threads (id) ON DELETE CASCADE);
CREATE TABLE query_statistics (id INTEGER PRIMARY KEY, started_at_ms INTEGER NOT NULL, command TEXT NOT NULL CHECK (command IN ('new', 'reply')), profile TEXT NOT NULL, provider_kind TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL, outcome TEXT NOT NULL CHECK (outcome IN ('complete', 'partial', 'failed')), error_class TEXT, wall_ms INTEGER NOT NULL, api_ms INTEGER NOT NULL, first_token_ms INTEGER, input_tokens INTEGER, output_tokens INTEGER);
CREATE TABLE provider_health (provider_kind TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL, last_success_at_ms INTEGER, last_failure_at_ms INTEGER, last_failure_class TEXT, PRIMARY KEY (provider_kind, base_url, model)) WITHOUT ROWID;
INSERT INTO threads VALUES (7, 1, 'default', 'openai-compatible', 'http://127.0.0.1:1/v1', 'old-model', 'Be brief.', 41, 'LOCAL_API_KEY');
INSERT INTO turns VALUES (1, 7, 1, 1, 'q1', 'a1', 'complete', NULL);
INSERT INTO turns VALUES (2, 7, 2, 1, 'q2', 'cut', 'partial', 'provider request failed: cut');
INSERT INTO current_thread VALUES (1, 7);
PRAGMA user_version = 1;
";

#[test]
fn version_1_snapshots_migrate_without_an_output_limit() {
    let path = scratch();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    Connection::open(&path)
        .unwrap()
        .execute_batch(VERSION_1)
        .unwrap();
    let barrier = Arc::new(Barrier::new(4));
    let openers: Vec<_> = (0..4)
        .map(|_| {
            let (path, barrier) = (path.clone(), Arc::clone(&barrier));
            thread::spawn(move || {
                barrier.wait();
                Store::open(&path).map(|_| ())
            })
        })
        .collect();
    for opener in openers {
        opener.join().unwrap().unwrap();
    }
    let mut store = Store::open(&path).unwrap();
    assert_eq!(scalar::<i64>(&store, "PRAGMA user_version"), 3);
    let thread = store.current().unwrap().unwrap();
    assert_eq!(thread.id, 7);
    assert_eq!(thread.target, target("old-model"));
    assert_eq!(
        thread.history,
        vec![Exchange {
            prompt: "q1".to_string(),
            answer: "a1".to_string()
        }]
    );
    store
        .record(&record(&thread.target, Some(7), complete("q3", "a3")))
        .unwrap();
    assert_eq!(counts(&store), vec![1, 3, 1, 1, 1, 0]);
}

#[test]
fn an_explicit_output_limit_is_part_of_the_snapshot() {
    let path = scratch();
    let mut store = Store::open(&path).unwrap();
    let snapshot = Target {
        max_output_tokens: Some(512),
        ..target("model")
    };
    store
        .record(&record(&snapshot, None, complete("q", "a")))
        .unwrap();
    let mut reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.current().unwrap().unwrap().target, snapshot);
}

#[test]
fn a_new_database_has_no_current_thread() {
    let mut store = Store::open(&scratch()).unwrap();
    assert!(store.current().unwrap().is_none());
}

#[test]
fn reopening_returns_the_snapshot_and_history() {
    let path = scratch();
    let snapshot = target("model");
    let mut store = Store::open(&path).unwrap();
    store
        .record(&record(&snapshot, None, complete("q1", "a1\n\n")))
        .unwrap();
    drop(store);
    let thread = Store::open(&path).unwrap().current().unwrap().unwrap();
    assert_eq!(thread.target, snapshot);
    assert_eq!(
        thread.history,
        vec![Exchange {
            prompt: "q1".to_string(),
            answer: "a1\n\n".to_string()
        }]
    );
}

#[test]
fn replies_append_in_order_and_history_skips_partial_turns() {
    let mut store = Store::open(&scratch()).unwrap();
    let snapshot = target("model");
    store
        .record(&record(&snapshot, None, complete("q1", "a1")))
        .unwrap();
    let id = current_id(&mut store);
    store
        .record(&record(&snapshot, id, partial("q2", "cut")))
        .unwrap();
    store
        .record(&record(&snapshot, id, complete("q3", "")))
        .unwrap();
    let thread = store.current().unwrap().unwrap();
    let prompts: Vec<_> = thread.history.iter().map(|e| e.prompt.as_str()).collect();
    assert_eq!(prompts, ["q1", "q3"]);
    assert_eq!(thread.history[1].answer, "");
    assert_eq!(
        scalar::<String>(
            &store,
            "SELECT group_concat(ordinal || status || coalesce(reason, ''), ',') FROM turns"
        ),
        "1complete,2partialprovider request failed: cut,3complete"
    );
}

#[test]
fn failure_before_text_records_statistics_but_no_thread() {
    let mut store = Store::open(&scratch()).unwrap();
    let snapshot = target("model");
    store.record(&record(&snapshot, None, None)).unwrap();
    assert_eq!(counts(&store), vec![0, 0, 0, 1, 1, 0]);
    assert_eq!(
        scalar::<String>(
            &store,
            "SELECT outcome || ':' || error_class FROM query_statistics"
        ),
        "failed:provider"
    );
}

#[test]
fn the_last_query_to_finish_owns_the_current_thread() {
    let mut store = Store::open(&scratch()).unwrap();
    let snapshot = target("model");
    store
        .record(&record(&snapshot, None, complete("a", "a")))
        .unwrap();
    let first = current_id(&mut store);
    store
        .record(&record(&snapshot, None, complete("b", "b")))
        .unwrap();
    let second = current_id(&mut store);
    assert_ne!(first, second);
    store.record(&record(&snapshot, first, None)).unwrap();
    assert_eq!(current_id(&mut store), second);
    store
        .record(&record(&snapshot, first, complete("c", "c")))
        .unwrap();
    assert_eq!(current_id(&mut store), first);
}

#[test]
fn an_injected_failure_rolls_back_every_table() {
    for table in [
        "turns",
        "query_statistics",
        "provider_health",
        "current_thread",
    ] {
        for existing in [false, true] {
            let mut store = Store::open(&scratch()).unwrap();
            let snapshot = target("model");
            if existing {
                store
                    .record(&record(&snapshot, None, complete("q", "a")))
                    .unwrap();
            }
            let thread = current_id(&mut store);
            let before = counts(&store);
            store
                .connection
                .execute_batch(&format!(
                    "CREATE TRIGGER fail BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT, 'injected'); END;"
                ))
                .unwrap();
            let error = store
                .record(&record(&snapshot, thread, complete("q2", "a2")))
                .unwrap_err();
            assert!(error.to_string().contains("injected"), "{error}");
            assert_eq!(counts(&store), before, "{table}");
        }
    }
}

/// A held write blocks a second writer for the configured busy timeout while
/// readers still see the committed state; after rollback the writer succeeds.
#[test]
fn a_held_write_delays_a_second_writer_and_hides_nothing_from_readers() {
    let path = scratch();
    let snapshot = target("model");
    let mut holder = Store::open(&path).unwrap();
    let mut contender = Store::open(&path).unwrap();
    holder
        .record(&record(&snapshot, None, complete("q1", "a1")))
        .unwrap();
    let thread = current_id(&mut holder);
    let held = holder
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    held.execute("DELETE FROM current_thread", []).unwrap();
    assert_eq!(current_id(&mut contender), thread);
    let start = Instant::now();
    let error = contender
        .record(&record(&snapshot, thread, complete("q2", "a2")))
        .unwrap_err();
    let waited = start.elapsed();
    assert!(waited >= BUSY_TIMEOUT.mul_f32(0.9), "{waited:?}");
    assert!(error.to_string().contains("database is locked"), "{error}");
    held.rollback().unwrap();
    contender
        .record(&record(&snapshot, thread, complete("q2", "a2")))
        .unwrap();
    assert_eq!(counts(&holder), vec![1, 2, 1, 2, 1, 0]);
    assert_eq!(current_id(&mut holder), thread);
    assert_eq!(scalar::<String>(&holder, "PRAGMA integrity_check"), "ok");
}

#[test]
fn health_is_kept_per_provider_target() {
    let mut store = Store::open(&scratch()).unwrap();
    let first = target("one");
    let second = target("two");
    let mut elsewhere = target("one");
    elsewhere.base_url = "http://127.0.0.1:2/v1".to_string();
    store
        .record(&record(&first, None, complete("q", "a")))
        .unwrap();
    store.record(&record(&first, None, None)).unwrap();
    store
        .record(&record(&first, None, complete("q", "a")))
        .unwrap();
    store.record(&record(&second, None, None)).unwrap();
    store.record(&record(&elsewhere, None, None)).unwrap();
    let rows = scalar::<String>(
        &store,
        "SELECT group_concat(base_url || ' ' || model || ' ' || (last_success_at_ms IS NOT NULL) || (last_failure_at_ms IS NOT NULL) || coalesce(last_failure_class, '-'), ',') FROM (SELECT * FROM provider_health ORDER BY base_url, model)",
    );
    assert_eq!(
        rows,
        "http://127.0.0.1:1/v1 one 11provider,http://127.0.0.1:1/v1 two 01provider,http://127.0.0.1:2/v1 one 01provider"
    );
}

#[test]
fn statistics_and_health_carry_no_query_text() {
    let mut store = Store::open(&scratch()).unwrap();
    let snapshot = target("model");
    store
        .record(&record(
            &snapshot,
            None,
            partial("PROMPT_SENTINEL", "ANSWER_SENTINEL"),
        ))
        .unwrap();
    for table in ["query_statistics", "provider_health"] {
        let mut statement = store
            .connection
            .prepare(&format!("SELECT * FROM {table}"))
            .unwrap();
        let width = statement.column_count();
        let values: Vec<Value> = statement
            .query_map([], |row| {
                (0..width)
                    .map(|index| row.get(index))
                    .collect::<Result<Vec<Value>, _>>()
            })
            .unwrap()
            .flat_map(Result::unwrap)
            .collect();
        let text = format!("{values:?}");
        assert!(!text.contains("SENTINEL"), "{text}");
    }
}

const DAY: i64 = 86_400_000;

/// Sets every turn of `thread` to `ms` since the epoch.
fn age(store: &Store, thread: i64, ms: i64) {
    store
        .connection
        .execute(
            "UPDATE turns SET created_at_ms = ?1 WHERE thread_id = ?2",
            [ms, thread],
        )
        .unwrap();
}

fn at(ms: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms.unsigned_abs())
}

fn thread_ids(store: &Store) -> String {
    scalar(
        store,
        "SELECT coalesce(group_concat(id, ','), '') FROM (SELECT id FROM threads ORDER BY id)",
    )
}

#[test]
fn a_version_one_database_is_upgraded_in_place() {
    let path = scratch();
    let mut store = Store::open(&path).unwrap();
    let snapshot = target("model");
    store
        .record(&record(&snapshot, None, complete("q", "a")))
        .unwrap();
    store
        .connection
        .execute_batch(
            "DROP TABLE history_expiry; ALTER TABLE threads DROP COLUMN max_output_tokens; PRAGMA user_version = 1;",
        )
        .unwrap();
    drop(store);
    let mut store = Store::open(&path).unwrap();
    assert_eq!(scalar::<i64>(&store, "PRAGMA user_version"), 3);
    assert_eq!(counts(&store), vec![1, 1, 1, 1, 1, 0]);
    assert_eq!(store.current().unwrap().unwrap().history.len(), 1);
    assert_eq!(
        store.current().unwrap().unwrap().target.max_output_tokens,
        None
    );
}

/// Recall schema version 2 with expiry highwater preserved through the upgrade.
const VERSION_2: &str = "
CREATE TABLE threads (id INTEGER PRIMARY KEY, created_at_ms INTEGER NOT NULL, profile TEXT NOT NULL, provider_kind TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL, system_prompt TEXT NOT NULL, timeout_ms INTEGER NOT NULL CHECK (timeout_ms > 0), api_key_env TEXT NOT NULL);
CREATE TABLE turns (id INTEGER PRIMARY KEY, thread_id INTEGER NOT NULL REFERENCES threads (id) ON DELETE CASCADE, ordinal INTEGER NOT NULL CHECK (ordinal > 0), created_at_ms INTEGER NOT NULL, prompt TEXT NOT NULL, answer TEXT NOT NULL, status TEXT NOT NULL CHECK (status IN ('complete', 'partial')), reason TEXT CHECK ((status = 'complete') = (reason IS NULL)), UNIQUE (thread_id, ordinal));
CREATE TABLE current_thread (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), thread_id INTEGER NOT NULL REFERENCES threads (id) ON DELETE CASCADE);
CREATE TABLE query_statistics (id INTEGER PRIMARY KEY, started_at_ms INTEGER NOT NULL, command TEXT NOT NULL CHECK (command IN ('new', 'reply')), profile TEXT NOT NULL, provider_kind TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL, outcome TEXT NOT NULL CHECK (outcome IN ('complete', 'partial', 'failed')), error_class TEXT, wall_ms INTEGER NOT NULL, api_ms INTEGER NOT NULL, first_token_ms INTEGER, input_tokens INTEGER, output_tokens INTEGER);
CREATE TABLE provider_health (provider_kind TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL, last_success_at_ms INTEGER, last_failure_at_ms INTEGER, last_failure_class TEXT, PRIMARY KEY (provider_kind, base_url, model)) WITHOUT ROWID;
CREATE TABLE history_expiry (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), threads_cleared INTEGER NOT NULL CHECK (threads_cleared >= 0), last_cleared_at_ms INTEGER NOT NULL, highest_thread_id INTEGER NOT NULL CHECK (highest_thread_id >= 0));
INSERT INTO threads VALUES (9, 1, 'default', 'openai-compatible', 'http://127.0.0.1:1/v1', 'kept-model', 'Be brief.', 41, 'LOCAL_API_KEY');
INSERT INTO turns VALUES (1, 9, 1, 1, 'kept', 'answer', 'complete', NULL);
INSERT INTO current_thread VALUES (1, 9);
INSERT INTO history_expiry VALUES (1, 2, 500, 8);
PRAGMA user_version = 2;
";

#[test]
fn recall_version_two_preserves_highwater_and_migrates() {
    let path = scratch();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    Connection::open(&path)
        .unwrap()
        .execute_batch(VERSION_2)
        .unwrap();
    let mut store = Store::open(&path).unwrap();
    assert_eq!(scalar::<i64>(&store, "PRAGMA user_version"), 3);
    let thread = store.current().unwrap().unwrap();
    assert_eq!(thread.id, 9);
    assert_eq!(thread.target.model, "kept-model");
    assert_eq!(thread.target.max_output_tokens, None);
    assert_eq!(
        scalar::<i64>(&store, "SELECT highest_thread_id FROM history_expiry"),
        8
    );
    store
        .record(&record(&thread.target, None, complete("after", "a")))
        .unwrap();
    assert_eq!(thread_ids(&store), "9,10");
}

#[test]
fn prototype_version_two_layouts_are_refused() {
    for (batch, expected) in [
        (
            "ALTER TABLE threads ADD COLUMN max_output_tokens INTEGER CHECK (max_output_tokens > 0); PRAGMA user_version = 2;",
            "schema version 2 without history expiry is not supported",
        ),
        (
            "CREATE TABLE history_expiry (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), threads_cleared INTEGER NOT NULL CHECK (threads_cleared >= 0), last_cleared_at_ms INTEGER NOT NULL, highest_thread_id INTEGER NOT NULL CHECK (highest_thread_id >= 0)); ALTER TABLE threads ADD COLUMN max_output_tokens INTEGER CHECK (max_output_tokens > 0); PRAGMA user_version = 2;",
            "schema version 2 with an output-token snapshot is not supported",
        ),
    ] {
        let path = scratch();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        Connection::open(&path)
            .unwrap()
            .execute_batch(&format!("{VERSION_1}{batch}"))
            .unwrap();
        let before = fs::read(&path).unwrap();
        let message = Store::open(&path).err().unwrap().to_string();
        assert!(message.contains(expected), "{message}");
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn open_existing_never_creates_a_database() {
    let path = scratch();
    assert!(Store::open_existing(&path).unwrap().is_none());
    assert!(!path.exists() && !path.parent().unwrap().exists());
    drop(Store::open(&path).unwrap());
    assert!(Store::open_existing(&path).unwrap().is_some());
}

/// The present used by the expiry fixtures, in milliseconds since the epoch.
const NOW: i64 = 1_000 * DAY;

/// Three threads against a 90-day period ending at `NOW`: thread 1 is one
/// millisecond past the boundary, thread 2 is exactly on it, and thread 3 has
/// an ancient first turn but a partial second turn from yesterday.
fn aged_threads() -> Store {
    let mut store = Store::open(&scratch()).unwrap();
    let snapshot = target("model");
    for prompt in ["old", "boundary", "mixed"] {
        store
            .record(&record(&snapshot, None, complete(prompt, "a")))
            .unwrap();
    }
    store
        .record(&record(&snapshot, Some(3), partial("recent", "cut")))
        .unwrap();
    age(&store, 1, NOW - 90 * DAY - 1);
    age(&store, 2, NOW - 90 * DAY);
    age(&store, 3, NOW - 400 * DAY);
    store
        .connection
        .execute(
            "UPDATE turns SET created_at_ms = ?1 WHERE ordinal = 2",
            [NOW - DAY],
        )
        .unwrap();
    store
}

#[test]
fn expiry_removes_whole_threads_by_their_newest_turn() {
    let mut store = aged_threads();
    assert_eq!(store.expire(at(NOW), 90).unwrap(), 1);
    assert_eq!(
        (thread_ids(&store), counts(&store)),
        ("2,3".to_string(), vec![2, 3, 1, 4, 1, 1])
    );
}

#[test]
fn repeated_expiry_accumulates_the_cleared_count_and_last_time() {
    let mut store = aged_threads();
    let cleared: Vec<usize> = [NOW, NOW + 1, NOW + 1]
        .into_iter()
        .map(|present| store.expire(at(present), 90).unwrap())
        .collect();
    assert_eq!(cleared, [1, 1, 0]);
    let row: String = scalar(
        &store,
        "SELECT threads_cleared || ':' || last_cleared_at_ms FROM history_expiry",
    );
    assert_eq!(
        (row, thread_ids(&store)),
        (format!("2:{}", NOW + 1), "3".to_string())
    );
}

#[test]
fn an_expired_current_thread_leaves_no_current_thread() {
    let mut store = Store::open(&scratch()).unwrap();
    let snapshot = target("model");
    store
        .record(&record(&snapshot, None, complete("q", "a")))
        .unwrap();
    age(&store, 1, 0);
    assert_eq!(store.expire(at(2 * DAY), 1).unwrap(), 1);
    assert!(store.current().unwrap().is_none());
    assert_eq!(counts(&store), vec![0, 0, 0, 1, 1, 1]);
}

#[test]
fn an_enormous_retention_expires_nothing() {
    let mut store = Store::open(&scratch()).unwrap();
    let snapshot = target("model");
    store
        .record(&record(&snapshot, None, complete("q", "a")))
        .unwrap();
    age(&store, 1, 0);
    assert_eq!(store.expire(SystemTime::now(), u64::MAX).unwrap(), 0);
}

#[test]
fn expired_thread_ids_are_never_reused() {
    let mut store = Store::open(&scratch()).unwrap();
    let snapshot = target("model");
    for prompt in ["first", "second"] {
        store
            .record(&record(&snapshot, None, complete(prompt, "a")))
            .unwrap();
    }
    age(&store, 1, 0);
    age(&store, 2, 0);
    assert_eq!(store.expire(at(2 * DAY), 1).unwrap(), 2);
    store
        .record(&record(&snapshot, None, complete("third", "a")))
        .unwrap();
    assert_eq!(thread_ids(&store), "3");
    assert!(
        store
            .record(&record(&snapshot, Some(2), complete("late", "a")))
            .is_err()
    );
    assert_eq!(thread_ids(&store), "3");
}

#[test]
fn open_existing_never_creates_a_file_in_an_existing_directory() {
    let path = scratch();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    assert!(Store::open_existing(&path).unwrap().is_none());
    assert!(!path.exists());
}

#[test]
fn the_current_view_includes_partial_turns_with_reasons() {
    let mut store = Store::open(&scratch()).unwrap();
    assert!(store.current_view().unwrap().is_none());
    let snapshot = target("model");
    store
        .record(&record(&snapshot, None, complete("q1", "a1")))
        .unwrap();
    store
        .record(&record(&snapshot, Some(1), partial("q2", "cut")))
        .unwrap();
    let view = store.current_view().unwrap().unwrap();
    assert_eq!(
        (view.id, view.profile.as_str(), view.model.as_str()),
        (1, "default", "model")
    );
    let turns: Vec<_> = view
        .turns
        .iter()
        .map(|turn| {
            (
                turn.prompt.as_str(),
                turn.answer.as_str(),
                turn.reason.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        turns,
        [
            ("q1", "a1", None),
            ("q2", "cut", Some("provider request failed: cut"))
        ]
    );
}

/// Threads 1 to 3, where thread 1 (current) was continued most recently and
/// thread 2 least recently.
fn three_threads() -> Store {
    let mut store = Store::open(&scratch()).unwrap();
    let snapshot = target("model");
    for prompt in ["first", "second", "third"] {
        store
            .record(&record(&snapshot, None, complete(prompt, "a")))
            .unwrap();
    }
    store
        .record(&record(&snapshot, Some(1), complete("again", "a")))
        .unwrap();
    for (thread, ms) in [(1, 5_000), (2, 3_000), (3, 4_000)] {
        age(&store, thread, ms);
    }
    store
}

/// Whether `select` found thread `id`, and the current thread afterwards.
fn select_then_current(store: &mut Store, id: i64) -> (bool, Option<i64>) {
    let found = store.select(id).unwrap();
    (found, current_id(store))
}

#[test]
fn recent_threads_are_newest_first_with_turn_counts_and_openings() {
    let store = three_threads();
    let recent = store.recent(2).unwrap();
    let rows: Vec<_> = recent
        .iter()
        .map(|thread| {
            (
                thread.id,
                thread.updated_at_ms,
                thread.turns,
                thread.opening.as_str(),
                thread.current,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [(1, 5_000, 2, "first", true), (3, 4_000, 1, "third", false)]
    );
}

#[test]
fn selection_changes_current_only_for_an_existing_thread() {
    let mut store = three_threads();
    assert_eq!(select_then_current(&mut store, 2), (true, Some(2)));
    assert_eq!(select_then_current(&mut store, 9), (false, Some(2)));
}

#[test]
fn an_empty_store_summarizes_to_nothing() {
    let mut store = Store::open(&scratch()).unwrap();
    let empty = store.summary().unwrap();
    assert_eq!(
        (empty.queries, empty.median_wall_ms, empty.targets.len()),
        (0, None, 0)
    );
}

/// Four queries: a complete one and a partial reply on thread 1 (recorded
/// with usage), a failure against another model, and a usage-free complete
/// query with a 70 ms wall time inserted directly. The partial reply's wall
/// time is raised to 900 ms so the medians have a clear middle.
fn summarized_store() -> Summary {
    let mut store = Store::open(&scratch()).unwrap();
    let snapshot = target("model");
    store
        .record(&record(&snapshot, None, complete("q", "a")))
        .unwrap();
    store
        .record(&record(&snapshot, Some(1), partial("q", "a")))
        .unwrap();
    store.record(&record(&target("other"), None, None)).unwrap();
    store
        .connection
        .execute_batch("UPDATE query_statistics SET wall_ms = 900 WHERE id = 2; INSERT INTO query_statistics (started_at_ms, command, profile, provider_kind, base_url, model, outcome, wall_ms, api_ms) VALUES (0, 'new', 'default', 'openai-compatible', 'http://127.0.0.1:1/v1', 'model', 'complete', 70, 60);")
        .unwrap();
    store.summary().unwrap()
}

#[test]
fn the_summary_counts_outcomes_tokens_and_medians() {
    let summary = summarized_store();
    assert_eq!(
        (
            summary.queries,
            summary.complete,
            summary.partial,
            summary.failed
        ),
        (4, 2, 1, 1)
    );
    assert_eq!(
        (
            summary.input_tokens,
            summary.output_tokens,
            summary.with_usage
        ),
        (36, 9, 3)
    );
    assert_eq!(
        (summary.median_wall_ms, summary.median_first_token_ms),
        (Some(30), Some(10))
    );
}

#[test]
fn the_summary_counts_history_and_lists_targets_with_latest_health() {
    let summary = summarized_store();
    assert_eq!(
        (summary.threads, summary.turns, summary.threads_cleared),
        (1, 2, 0)
    );
    let targets: Vec<_> = summary
        .targets
        .iter()
        .map(|health| {
            (
                health.model.as_str(),
                health.queries,
                health.last_success_at_ms.is_some(),
                health.last_failure_class.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        targets,
        [
            ("model", 3, true, Some("provider")),
            ("other", 1, false, Some("provider"))
        ]
    );
}

#[test]
#[ignore = "measurement: cargo test --release --lib store::tests::startup -- --ignored --nocapture"]
fn startup_cost() {
    let first = sample(|| {
        let path = scratch();
        let start = Instant::now();
        drop(Store::open(&path).unwrap());
        start.elapsed()
    });
    let path = scratch();
    drop(Store::open(&path).unwrap());
    let warm = sample(|| {
        let start = Instant::now();
        drop(Store::open(&path).unwrap());
        start.elapsed()
    });
    println!("first creation: {}", summary(first));
    println!("warm reopen: {}", summary(warm));
}

fn sample(measure: impl FnMut() -> Duration) -> Vec<Duration> {
    let mut measure = measure;
    let mut samples: Vec<_> = (0..200).map(|_| measure()).collect();
    samples.sort_unstable();
    samples
}

fn summary(samples: Vec<Duration>) -> String {
    let median = samples[samples.len() / 2];
    let p95 = samples[samples.len() * 95 / 100];
    format!("median {median:?}, p95 {p95:?}, n {}", samples.len())
}
