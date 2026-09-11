//! Local history, query statistics, and provider health in one SQLite file.
//!
//! Only `ask new` and `ask reply` open the database, once per process. A reply
//! reads its thread in one short transaction before the request; every write
//! for a query happens in one immediate transaction after the answer finishes
//! or fails, never while it streams. The rollback journal keeps each write
//! short and leaves no WAL side files next to the database.

use std::{
    error::Error,
    fmt, fs, io,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, Row, Transaction, TransactionBehavior, params};

use crate::{
    config::Target,
    provider::{Exchange, Usage},
};

const SCHEMA_VERSION: i64 = 1;
const BUSY_TIMEOUT: Duration = Duration::from_millis(1_000);

// Statistics rows carry no prompt or answer text and no thread reference, so
// they can outlive history. Rows for `new` with a turn count created threads.
const SCHEMA: &str = "
CREATE TABLE threads (
    id INTEGER PRIMARY KEY,
    created_at_ms INTEGER NOT NULL,
    profile TEXT NOT NULL,
    provider_kind TEXT NOT NULL,
    base_url TEXT NOT NULL,
    model TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    timeout_ms INTEGER NOT NULL CHECK (timeout_ms > 0),
    api_key_env TEXT NOT NULL
);
CREATE TABLE turns (
    id INTEGER PRIMARY KEY,
    thread_id INTEGER NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal > 0),
    created_at_ms INTEGER NOT NULL,
    prompt TEXT NOT NULL,
    answer TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('complete', 'partial')),
    reason TEXT CHECK ((status = 'complete') = (reason IS NULL)),
    UNIQUE (thread_id, ordinal)
);
CREATE TABLE current_thread (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    thread_id INTEGER NOT NULL REFERENCES threads (id) ON DELETE CASCADE
);
CREATE TABLE query_statistics (
    id INTEGER PRIMARY KEY,
    started_at_ms INTEGER NOT NULL,
    command TEXT NOT NULL CHECK (command IN ('new', 'reply')),
    profile TEXT NOT NULL,
    provider_kind TEXT NOT NULL,
    base_url TEXT NOT NULL,
    model TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('complete', 'partial', 'failed')),
    error_class TEXT,
    wall_ms INTEGER NOT NULL,
    api_ms INTEGER NOT NULL,
    first_token_ms INTEGER,
    input_tokens INTEGER,
    output_tokens INTEGER
);
CREATE TABLE provider_health (
    provider_kind TEXT NOT NULL,
    base_url TEXT NOT NULL,
    model TEXT NOT NULL,
    last_success_at_ms INTEGER,
    last_failure_at_ms INTEGER,
    last_failure_class TEXT,
    PRIMARY KEY (provider_kind, base_url, model)
) WITHOUT ROWID;
PRAGMA user_version = 1;
";

const SELECT_THREAD: &str = "SELECT profile, provider_kind, base_url, model, system_prompt, timeout_ms, api_key_env FROM threads WHERE id = ?1";
const SELECT_HISTORY: &str = "SELECT prompt, answer FROM turns WHERE thread_id = ?1 AND status = 'complete' ORDER BY ordinal";
const INSERT_THREAD: &str = "INSERT INTO threads (created_at_ms, profile, provider_kind, base_url, model, system_prompt, timeout_ms, api_key_env) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";
const INSERT_TURN: &str = "INSERT INTO turns (thread_id, ordinal, created_at_ms, prompt, answer, status, reason) SELECT ?1, COALESCE(MAX(ordinal), 0) + 1, ?2, ?3, ?4, ?5, ?6 FROM turns WHERE thread_id = ?1";
const INSERT_STATISTICS: &str = "INSERT INTO query_statistics (started_at_ms, command, profile, provider_kind, base_url, model, outcome, error_class, wall_ms, api_ms, first_token_ms, input_tokens, output_tokens) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)";
const OBSERVE_SUCCESS: &str = "INSERT INTO provider_health (provider_kind, base_url, model, last_success_at_ms) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (provider_kind, base_url, model) DO UPDATE SET last_success_at_ms = excluded.last_success_at_ms";
const OBSERVE_FAILURE: &str = "INSERT INTO provider_health (provider_kind, base_url, model, last_failure_at_ms, last_failure_class) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (provider_kind, base_url, model) DO UPDATE SET last_failure_at_ms = excluded.last_failure_at_ms, last_failure_class = excluded.last_failure_class";
const MAKE_CURRENT: &str = "INSERT INTO current_thread (singleton, thread_id) VALUES (1, ?1) ON CONFLICT (singleton) DO UPDATE SET thread_id = excluded.thread_id";

pub struct Store {
    connection: Connection,
}

/// The current thread as a reply sees it: the profile snapshot captured at
/// creation and its complete turns in order.
pub struct Thread {
    pub id: i64,
    pub target: Target,
    pub history: Vec<Exchange>,
}

pub enum TurnStatus {
    Complete,
    Partial(String),
}

pub struct Turn<'a> {
    pub prompt: &'a str,
    pub answer: &'a str,
    pub status: TurnStatus,
}

pub enum Health {
    Success,
    Failure(&'static str),
}

/// Text-free measurements of one query.
pub struct Measurement {
    pub command: &'static str,
    pub outcome: &'static str,
    pub error_class: Option<&'static str>,
    pub wall: Duration,
    pub api: Duration,
    pub first_token: Option<Duration>,
    pub usage: Option<Usage>,
}

/// Everything one query writes. `thread` is `None` for `new`; `turn` is
/// `None` when the provider failed before any answer text.
pub struct Record<'a> {
    pub started_at: SystemTime,
    pub target: &'a Target,
    pub thread: Option<i64>,
    pub turn: Option<Turn<'a>>,
    pub measurement: Measurement,
    pub health: Option<Health>,
}

#[derive(Debug)]
pub struct StoreError(String);

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self(error.to_string())
    }
}

impl Store {
    /// Opens the database, creating it and its schema when absent. A newer
    /// schema version is refused before anything is written.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Self::connect(path).map_err(|error| {
            StoreError(format!(
                "cannot open history database '{}': {error}",
                path.display()
            ))
        })
    }

    fn connect(path: &Path) -> Result<Self, Box<dyn Error>> {
        prepare_file(path)?;
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        let version = supported(user_version(&connection)?)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update_and_check(None, "journal_mode", "DELETE", |row| {
            row.get::<_, String>(0)
        })?;
        if version == 0 {
            create_schema(&mut connection)?;
        }
        Ok(Self { connection })
    }

    /// Reads the current thread, its snapshot, and its complete turns in one
    /// read transaction.
    pub fn current(&mut self) -> Result<Option<Thread>, StoreError> {
        let transaction = self.connection.transaction()?;
        let thread = read_current(&transaction)?;
        transaction.commit()?;
        Ok(thread)
    }

    /// Writes the thread (for `new`), the turn, statistics, provider health,
    /// and the current-thread change in one transaction.
    pub fn record(&mut self, record: &Record<'_>) -> Result<(), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let thread = match &record.turn {
            Some(turn) => Some(append(&transaction, record, turn)?),
            None => None,
        };
        insert_statistics(&transaction, record)?;
        if let Some(health) = &record.health {
            observe(&transaction, record.target, health)?;
        }
        if let Some(id) = thread {
            transaction.execute(MAKE_CURRENT, [id])?;
        }
        transaction.commit()?;
        Ok(())
    }
}

/// Creates missing directories as 0700 and a missing file as 0600 so SQLite
/// never creates them with broader permissions. Existing modes are kept.
fn prepare_file(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }
    let created = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path);
    match created {
        Err(error) if error.kind() != io::ErrorKind::AlreadyExists => Err(error),
        _ => Ok(()),
    }
}

fn user_version(connection: &Connection) -> rusqlite::Result<i64> {
    connection.pragma_query_value(None, "user_version", |row| row.get(0))
}

fn supported(version: i64) -> Result<i64, String> {
    match version {
        0 | SCHEMA_VERSION => Ok(version),
        newer if newer > SCHEMA_VERSION => Err(format!(
            "schema version {newer} is newer than this ask supports ({SCHEMA_VERSION})"
        )),
        other => Err(format!("unrecognized schema version {other}")),
    }
}

/// Version 0 is accepted only for an empty database. The check repeats under
/// the write lock so concurrent first runs create the schema once.
fn create_schema(connection: &mut Connection) -> Result<(), Box<dyn Error>> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if supported(user_version(&transaction)?)? == 0 {
        let objects: i64 =
            transaction.query_row("SELECT count(*) FROM sqlite_schema", [], |row| row.get(0))?;
        if objects != 0 {
            return Err("the file is not empty and has no ask schema version".into());
        }
        transaction.execute_batch(SCHEMA)?;
    }
    transaction.commit()?;
    Ok(())
}

fn read_current(transaction: &Transaction<'_>) -> rusqlite::Result<Option<Thread>> {
    let id = transaction
        .query_row("SELECT thread_id FROM current_thread", [], |row| row.get(0))
        .optional()?;
    let Some(id) = id else {
        return Ok(None);
    };
    let target = transaction.query_row(SELECT_THREAD, [id], snapshot)?;
    let history = transaction
        .prepare(SELECT_HISTORY)?
        .query_map([id], exchange)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Some(Thread {
        id,
        target,
        history,
    }))
}

fn snapshot(row: &Row<'_>) -> rusqlite::Result<Target> {
    Ok(Target {
        profile: row.get(0)?,
        kind: row.get(1)?,
        base_url: row.get(2)?,
        model: row.get(3)?,
        system_prompt: row.get(4)?,
        timeout_ms: row.get::<_, i64>(5)?.unsigned_abs(),
        api_key_env: row.get(6)?,
    })
}

fn exchange(row: &Row<'_>) -> rusqlite::Result<Exchange> {
    Ok(Exchange {
        prompt: row.get(0)?,
        answer: row.get(1)?,
    })
}

fn append(
    transaction: &Transaction<'_>,
    record: &Record<'_>,
    turn: &Turn<'_>,
) -> rusqlite::Result<i64> {
    let thread = match record.thread {
        Some(id) => id,
        None => insert_thread(transaction, record)?,
    };
    let (status, reason) = match &turn.status {
        TurnStatus::Complete => ("complete", None),
        TurnStatus::Partial(reason) => ("partial", Some(reason.as_str())),
    };
    transaction.execute(
        INSERT_TURN,
        params![
            thread,
            epoch_ms(SystemTime::now()),
            turn.prompt,
            turn.answer,
            status,
            reason
        ],
    )?;
    Ok(thread)
}

fn insert_thread(transaction: &Transaction<'_>, record: &Record<'_>) -> rusqlite::Result<i64> {
    let target = record.target;
    transaction.execute(
        INSERT_THREAD,
        params![
            epoch_ms(record.started_at),
            target.profile,
            target.kind,
            target.base_url,
            target.model,
            target.system_prompt,
            integer(target.timeout_ms),
            target.api_key_env
        ],
    )?;
    Ok(transaction.last_insert_rowid())
}

fn insert_statistics(transaction: &Transaction<'_>, record: &Record<'_>) -> rusqlite::Result<()> {
    let (target, measurement) = (record.target, &record.measurement);
    let usage = measurement.usage;
    transaction.execute(
        INSERT_STATISTICS,
        params![
            epoch_ms(record.started_at),
            measurement.command,
            target.profile,
            target.kind,
            target.base_url,
            target.model,
            measurement.outcome,
            measurement.error_class,
            millis(measurement.wall),
            millis(measurement.api),
            measurement.first_token.map(millis),
            usage.map(|usage| integer(usage.input)),
            usage.map(|usage| integer(usage.output))
        ],
    )?;
    Ok(())
}

/// Keeps only the latest success and latest failure per provider target.
fn observe(
    transaction: &Transaction<'_>,
    target: &Target,
    health: &Health,
) -> rusqlite::Result<()> {
    let identity = (&target.kind, &target.base_url, &target.model);
    let now = epoch_ms(SystemTime::now());
    match health {
        Health::Success => transaction.execute(
            OBSERVE_SUCCESS,
            params![identity.0, identity.1, identity.2, now],
        ),
        Health::Failure(class) => transaction.execute(
            OBSERVE_FAILURE,
            params![identity.0, identity.1, identity.2, now, class],
        ),
    }?;
    Ok(())
}

fn epoch_ms(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH).map_or(0, millis)
}

fn millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn integer(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
