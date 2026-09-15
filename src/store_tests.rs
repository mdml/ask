use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

use rusqlite::types::Value;

use super::*;

const TABLES: [&str; 5] = [
    "threads",
    "turns",
    "current_thread",
    "query_statistics",
    "provider_health",
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

#[test]
fn creates_the_schema_with_owner_only_permissions() {
    let path = scratch();
    let store = Store::open(&path).unwrap();
    assert_eq!(scalar::<i64>(&store, "PRAGMA user_version"), 1);
    assert_eq!(counts(&store), vec![0; 5]);
    assert_eq!(scalar::<String>(&store, "PRAGMA journal_mode"), "delete");
    assert_eq!(scalar::<i64>(&store, "PRAGMA synchronous"), 2);
    assert_eq!(scalar::<i64>(&store, "PRAGMA foreign_keys"), 1);
    assert_eq!(scalar::<i64>(&store, "PRAGMA busy_timeout"), 1_000);
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(path.parent().unwrap()), 0o700);
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
    assert_eq!(mode(&path), 0o640);
    assert_eq!(mode(directory), 0o750);
}

#[test]
fn newer_and_foreign_databases_are_refused_without_writing() {
    for (setup, expected) in [
        ("PRAGMA user_version = 2;", "schema version 2 is newer"),
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
    assert_eq!(counts(&store), vec![0, 0, 0, 1, 1]);
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
