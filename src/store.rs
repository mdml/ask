//! Local history, query statistics, and provider health in one SQLite file.
//!
//! `ask new` and `ask reply` open the database once per process, creating it
//! when absent; `ask thread`, `ask switch`, and `ask stats` open only an
//! existing database. A reply reads its thread in one short transaction when
//! it starts; once input is submitted, a query applies any configured history
//! expiry in its own short transaction before the request; every write for a
//! query happens in one immediate transaction after the answer finishes or
//! fails, never while it streams. The rollback journal keeps each write
//! short and leaves no WAL side files next to the database.

use std::{
    error::Error,
    fmt, fs, io,
    io::Read,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Row, Transaction, TransactionBehavior, params,
};

use crate::{
    config::Target,
    provider::{Exchange, Usage},
};

#[path = "store_recall.rs"]
mod recall;

const SCHEMA_VERSION: i64 = 4;
const DAY_MS: i64 = 86_400_000;
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
    api_key_env TEXT NOT NULL,
    max_output_tokens INTEGER CHECK (max_output_tokens > 0)
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
    last_success_source TEXT,
    last_failure_at_ms INTEGER,
    last_failure_class TEXT,
    last_failure_source TEXT,
    PRIMARY KEY (provider_kind, base_url, model)
) WITHOUT ROWID;
CREATE TABLE history_expiry (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    threads_cleared INTEGER NOT NULL CHECK (threads_cleared >= 0),
    last_cleared_at_ms INTEGER NOT NULL,
    highest_thread_id INTEGER NOT NULL CHECK (highest_thread_id >= 0)
);
PRAGMA user_version = 4;
";

// Version 2 adds the text-free count of threads removed by history expiry and
// the highest thread id it removed, so a removed id is never assigned again.
const MIGRATE_1_TO_2: &str = "
CREATE TABLE history_expiry (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    threads_cleared INTEGER NOT NULL CHECK (threads_cleared >= 0),
    last_cleared_at_ms INTEGER NOT NULL,
    highest_thread_id INTEGER NOT NULL CHECK (highest_thread_id >= 0)
);
PRAGMA user_version = 2;
";

// Version 3 captures the profile's explicit output-token limit. Threads
// created by earlier versions had none, so their snapshots keep provider defaults.
const MIGRATE_2_TO_3: &str = "
ALTER TABLE threads ADD COLUMN max_output_tokens INTEGER CHECK (max_output_tokens > 0);
PRAGMA user_version = 3;
";

// Version 4 records whether the latest success or failure came from an ordinary
// query or an explicit live check. Existing rows keep NULL sources.
const MIGRATE_3_TO_4: &str = "
ALTER TABLE provider_health ADD COLUMN last_success_source TEXT;
ALTER TABLE provider_health ADD COLUMN last_failure_source TEXT;
PRAGMA user_version = 4;
";

const EXPIRE_THREADS: &str = "DELETE FROM threads WHERE coalesce((SELECT max(created_at_ms) FROM turns WHERE turns.thread_id = threads.id), created_at_ms) < ?1";
const HIGHEST_THREAD_ID: &str = "SELECT coalesce(max(id), 0) FROM threads";
const COUNT_CLEARED: &str = "INSERT INTO history_expiry (singleton, threads_cleared, last_cleared_at_ms, highest_thread_id) VALUES (1, ?1, ?2, ?3) ON CONFLICT (singleton) DO UPDATE SET threads_cleared = threads_cleared + excluded.threads_cleared, last_cleared_at_ms = excluded.last_cleared_at_ms, highest_thread_id = max(highest_thread_id, excluded.highest_thread_id)";
const SELECT_THREAD: &str = "SELECT profile, provider_kind, base_url, model, system_prompt, timeout_ms, api_key_env, max_output_tokens FROM threads WHERE id = ?1";
const SELECT_HISTORY: &str = "SELECT prompt, answer FROM turns WHERE thread_id = ?1 AND status = 'complete' ORDER BY ordinal";
// Thread ids continue past any id history expiry removed, so a reply or
// selection that names a removed thread can never reach a newer one.
const INSERT_THREAD: &str = "INSERT INTO threads (id, created_at_ms, profile, provider_kind, base_url, model, system_prompt, timeout_ms, api_key_env, max_output_tokens) VALUES (max((SELECT coalesce(max(id), 0) FROM threads), coalesce((SELECT highest_thread_id FROM history_expiry), 0)) + 1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)";
const INSERT_TURN: &str = "INSERT INTO turns (thread_id, ordinal, created_at_ms, prompt, answer, status, reason) SELECT ?1, COALESCE(MAX(ordinal), 0) + 1, ?2, ?3, ?4, ?5, ?6 FROM turns WHERE thread_id = ?1";
const INSERT_STATISTICS: &str = "INSERT INTO query_statistics (started_at_ms, command, profile, provider_kind, base_url, model, outcome, error_class, wall_ms, api_ms, first_token_ms, input_tokens, output_tokens) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)";
const OBSERVE_SUCCESS: &str = "INSERT INTO provider_health (provider_kind, base_url, model, last_success_at_ms, last_success_source) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (provider_kind, base_url, model) DO UPDATE SET last_success_at_ms = excluded.last_success_at_ms, last_success_source = excluded.last_success_source";
const OBSERVE_FAILURE: &str = "INSERT INTO provider_health (provider_kind, base_url, model, last_failure_at_ms, last_failure_class, last_failure_source) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT (provider_kind, base_url, model) DO UPDATE SET last_failure_at_ms = excluded.last_failure_at_ms, last_failure_class = excluded.last_failure_class, last_failure_source = excluded.last_failure_source";
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

/// Whether a provider-health observation came from an ordinary query or an
/// explicit live check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthSource {
    Query,
    LiveCheck,
}

impl HealthSource {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::LiveCheck => "live-check",
        }
    }
}

pub enum Health {
    Success(HealthSource),
    Failure {
        class: &'static str,
        source: HealthSource,
    },
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

/// The outcome of a side-effect-free database inspection for `ask doctor`.
pub enum DatabaseState {
    Absent,
    Inaccessible(String),
    Current,
    Limited(String),
    Unusable(String),
}

/// Database state and health observations from one read-only connection and one
/// read transaction, without creating, migrating, or reopening the file.
pub struct StorageInspection {
    pub state: DatabaseState,
    pub targets: Vec<recall::TargetHealth>,
}

impl fmt::Display for DatabaseState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent => formatter.write_str("absent"),
            Self::Inaccessible(reason) => write!(formatter, "inaccessible: {reason}"),
            Self::Current => formatter.write_str("current schema, integrity ok"),
            Self::Limited(reason) => write!(formatter, "limited check: {reason}"),
            Self::Unusable(reason) => write!(formatter, "unusable: {reason}"),
        }
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
        Self::configure(Connection::open(path)?)
    }

    fn configure(mut connection: Connection) -> Result<Self, Box<dyn Error>> {
        connection.busy_timeout(BUSY_TIMEOUT)?;
        let version = supported(user_version(&connection)?)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update_and_check(None, "journal_mode", "DELETE", |row| {
            row.get::<_, String>(0)
        })?;
        if version != SCHEMA_VERSION {
            create_schema(&mut connection)?;
        }
        Ok(Self { connection })
    }

    /// Validates an existing database and reads health observations without
    /// creating, migrating, or writing to it. Header bytes and sidecar files
    /// are checked before SQLite opens the file so implicit recovery sidecars
    /// are never created.
    pub fn inspect_storage(path: &Path) -> StorageInspection {
        if let Err(state) = database_metadata(path) {
            return StorageInspection {
                state,
                targets: Vec::new(),
            };
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY;
        let opened = Connection::open_with_flags(path, flags).and_then(|connection| {
            connection.pragma_update(None, "query_only", true)?;
            Ok(connection)
        });
        let mut connection = match opened {
            Ok(connection) => connection,
            Err(error) if is_hot_journal(&error) => {
                return StorageInspection {
                    state: DatabaseState::Unusable(
                        "a hot rollback journal is present; close other ask processes and retry"
                            .into(),
                    ),
                    targets: Vec::new(),
                };
            }
            Err(error) => {
                return StorageInspection {
                    state: DatabaseState::Unusable(format!("cannot open read-only: {error}")),
                    targets: Vec::new(),
                };
            }
        };
        let transaction = match connection.transaction() {
            Ok(transaction) => transaction,
            Err(error) => {
                return StorageInspection {
                    state: DatabaseState::Unusable(format!(
                        "cannot begin read transaction: {error}"
                    )),
                    targets: Vec::new(),
                };
            }
        };
        match inspect_in_transaction(&transaction) {
            Ok(targets) => StorageInspection {
                state: DatabaseState::Current,
                targets,
            },
            Err(state) => StorageInspection {
                state,
                targets: Vec::new(),
            },
        }
    }

    /// Returns only the database state from [`Self::inspect_storage`].
    #[cfg(test)]
    pub fn inspect_database(path: &Path) -> DatabaseState {
        Self::inspect_storage(path).state
    }

    /// Records one provider-health observation without touching history.
    pub fn record_health(&mut self, target: &Target, health: Health) -> Result<(), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        observe(&transaction, target, &health)?;
        transaction.commit()?;
        Ok(())
    }

    /// Opens the database only when its file already exists, so inspection
    /// commands never create one.
    /// SQLite opens without its create flag, so a file removed after any
    /// earlier check is reported missing rather than recreated.
    pub fn open_existing(path: &Path) -> Result<Option<Self>, StoreError> {
        let flags = OpenFlags::default().difference(OpenFlags::SQLITE_OPEN_CREATE);
        let opened = Connection::open_with_flags(path, flags)
            .map_err(Box::<dyn Error>::from)
            .and_then(Self::configure);
        match opened {
            Ok(store) => Ok(Some(store)),
            Err(_) if !path.exists() => Ok(None),
            Err(error) => Err(StoreError(format!(
                "cannot open history database '{}': {error}",
                path.display()
            ))),
        }
    }

    /// Removes whole threads whose newest turn is older than `days` before
    /// `now`, and adds the number removed to the cleared-thread count.
    pub fn expire(&mut self, now: SystemTime, days: u64) -> Result<usize, StoreError> {
        let now = epoch_ms(now);
        let cutoff = now.saturating_sub(integer(days).saturating_mul(DAY_MS));
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let highest: i64 = transaction.query_row(HIGHEST_THREAD_ID, [], |row| row.get(0))?;
        let cleared = transaction.execute(EXPIRE_THREADS, [cutoff])?;
        if cleared > 0 {
            transaction.execute(
                COUNT_CLEARED,
                params![i64::try_from(cleared).unwrap_or(i64::MAX), now, highest],
            )?;
        }
        transaction.commit()?;
        Ok(cleared)
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

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let file = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    path.with_file_name(format!("{file}{suffix}"))
}

fn sidecar_present(path: &Path, suffix: &str) -> bool {
    sidecar(path, suffix).is_file()
}

fn blocking_sidecar(path: &Path) -> Option<String> {
    for suffix in ["-wal", "-shm", "-journal"] {
        if sidecar_present(path, suffix) {
            return Some(format!(
                "companion file '{suffix}' is present; deeper validation was not attempted"
            ));
        }
    }
    None
}

fn is_hot_journal(error: &rusqlite::Error) -> bool {
    error.sqlite_error_code() == Some(rusqlite::ErrorCode::ReadOnly)
        && error.to_string().contains("rollback")
}

const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

fn database_metadata(path: &Path) -> Result<(), DatabaseState> {
    match fs::metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(DatabaseState::Absent),
        Err(error) => Err(DatabaseState::Inaccessible(format!("cannot stat: {error}"))),
        Ok(metadata) if metadata.is_dir() => Err(DatabaseState::Unusable(
            "path is a directory, not a database file".into(),
        )),
        Ok(metadata) if !metadata.is_file() => {
            Err(DatabaseState::Unusable("path is not a regular file".into()))
        }
        Ok(_) => Ok(()),
    }?;
    if let Some(reason) = blocking_sidecar(path) {
        return Err(DatabaseState::Limited(reason));
    }
    if let Some(reason) = wal_header_issue(path) {
        return Err(DatabaseState::Limited(reason));
    }
    Ok(())
}

fn wal_header_issue(path: &Path) -> Option<String> {
    let mut header = [0u8; 20];
    let mut file = fs::File::open(path).ok()?;
    if file.read_exact(&mut header).is_err() {
        return Some("file is too small to be a SQLite database".into());
    }
    if header.get(..16) != Some(SQLITE_MAGIC) {
        return Some("file is not a SQLite database".into());
    }
    if header[18] == 2 || header[19] == 2 {
        return Some(
            "database header indicates WAL journal mode; deeper validation was not attempted"
                .into(),
        );
    }
    None
}

fn inspect_in_transaction(
    transaction: &Transaction<'_>,
) -> Result<Vec<recall::TargetHealth>, DatabaseState> {
    let version = match user_version(transaction) {
        Ok(version) => version,
        Err(error) => {
            return Err(DatabaseState::Unusable(format!(
                "cannot read schema version: {error}"
            )));
        }
    };
    match supported(version) {
        Ok(SCHEMA_VERSION) => {}
        Ok(older) => {
            return Err(DatabaseState::Limited(format!(
                "schema version {older} is older than this ask supports ({SCHEMA_VERSION}); migration was not attempted"
            )));
        }
        Err(message) => return Err(DatabaseState::Unusable(message)),
    }
    if !expected_tables(transaction) {
        return Err(DatabaseState::Unusable(
            "required tables are missing from the history database".into(),
        ));
    }
    if let Err(message) = required_columns(transaction) {
        return Err(DatabaseState::Unusable(message));
    }
    if let Err(message) = integrity(transaction) {
        return Err(DatabaseState::Unusable(message));
    }
    recall::read_target_health(transaction).map_err(|message| {
        DatabaseState::Limited(format!("health observations could not be read: {message}"))
    })
}

const REQUIRED_COLUMNS: [(&str, &str); 3] = [
    ("threads", "max_output_tokens"),
    ("provider_health", "last_success_source"),
    ("provider_health", "last_failure_source"),
];

fn required_columns(transaction: &Transaction<'_>) -> Result<(), String> {
    for (table, column) in REQUIRED_COLUMNS {
        let exists =
            column_exists(transaction, table, column).map_err(|error| error.to_string())?;
        if !exists {
            return Err(format!(
                "required column '{column}' is missing from table '{table}'"
            ));
        }
    }
    Ok(())
}

const EXPECTED_TABLES: [&str; 6] = [
    "threads",
    "turns",
    "current_thread",
    "query_statistics",
    "provider_health",
    "history_expiry",
];

fn expected_tables(connection: &Connection) -> bool {
    EXPECTED_TABLES.iter().all(|name| {
        connection
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name = ?1",
                [*name],
                |row| row.get::<_, i64>(0),
            )
            .is_ok_and(|count| count > 0)
    })
}

fn integrity(connection: &Connection) -> Result<(), String> {
    let message: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if message == "ok" {
        Ok(())
    } else {
        Err(message)
    }
}

fn supported(version: i64) -> Result<i64, String> {
    match version {
        0..=SCHEMA_VERSION => Ok(version),
        newer if newer > SCHEMA_VERSION => Err(format!(
            "schema version {newer} is newer than this ask supports ({SCHEMA_VERSION})"
        )),
        other => Err(format!("unrecognized schema version {other}")),
    }
}

fn table_exists(transaction: &Transaction<'_>, name: &str) -> rusqlite::Result<bool> {
    transaction.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get(0),
    )
}

fn column_exists(
    transaction: &Transaction<'_>,
    table: &str,
    column: &str,
) -> rusqlite::Result<bool> {
    transaction.query_row(
        "SELECT count(*) FROM pragma_table_info(?1) WHERE name = ?2",
        params![table, column],
        |row| row.get(0),
    )
}

/// Version 0 is accepted only for an empty database; older versions migrate in
/// place. The check repeats under the write lock so concurrent first runs
/// create or upgrade the schema once.
fn create_schema(connection: &mut Connection) -> Result<(), Box<dyn Error>> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    match supported(user_version(&transaction)?)? {
        0 => {
            let objects: i64 =
                transaction
                    .query_row("SELECT count(*) FROM sqlite_schema", [], |row| row.get(0))?;
            if objects != 0 {
                return Err("the file is not empty and has no ask schema version".into());
            }
            transaction.execute_batch(SCHEMA)?;
        }
        1 => {
            transaction.execute_batch(MIGRATE_1_TO_2)?;
            transaction.execute_batch(MIGRATE_2_TO_3)?;
            transaction.execute_batch(MIGRATE_3_TO_4)?;
        }
        2 => {
            if !table_exists(&transaction, "history_expiry")? {
                return Err(
                    "schema version 2 without history expiry is not supported by this ask".into(),
                );
            }
            if column_exists(&transaction, "threads", "max_output_tokens")? {
                return Err(
                    "schema version 2 with an output-token snapshot is not supported by this ask"
                        .into(),
                );
            }
            transaction.execute_batch(MIGRATE_2_TO_3)?;
            transaction.execute_batch(MIGRATE_3_TO_4)?;
        }
        3 => {
            transaction.execute_batch(MIGRATE_3_TO_4)?;
        }
        _ => (),
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
        max_output_tokens: row.get::<_, Option<i64>>(7)?.map(i64::unsigned_abs),
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
            target.api_key_env,
            target.max_output_tokens.map(integer)
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
        Health::Success(source) => transaction.execute(
            OBSERVE_SUCCESS,
            params![identity.0, identity.1, identity.2, now, source.as_str()],
        ),
        Health::Failure { class, source } => transaction.execute(
            OBSERVE_FAILURE,
            params![
                identity.0,
                identity.1,
                identity.2,
                now,
                class,
                source.as_str()
            ],
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

pub use recall::{Summary, TargetHealth, ThreadSummary, ThreadView};

#[cfg(test)]
pub use recall::StoredTurn;

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
