//! Read-only views of history and statistics, and current-thread selection.

use rusqlite::{OptionalExtension, Row, Transaction, TransactionBehavior};

use super::{MAKE_CURRENT, Store, StoreError};

const SELECT_CURRENT_ID: &str = "SELECT thread_id FROM current_thread";
const SELECT_HEADER: &str = "SELECT id, profile, model FROM threads WHERE id = ?1";
const SELECT_TURNS: &str =
    "SELECT prompt, answer, reason FROM turns WHERE thread_id = ?1 ORDER BY ordinal";
const SELECT_RECENT: &str = "
SELECT threads.id, max(turns.created_at_ms) AS updated, threads.profile, threads.model,
       count(turns.id),
       (SELECT prompt FROM turns AS first WHERE first.thread_id = threads.id ORDER BY ordinal LIMIT 1),
       threads.id IS (SELECT thread_id FROM current_thread)
FROM threads JOIN turns ON turns.thread_id = threads.id
GROUP BY threads.id
ORDER BY updated DESC, threads.id DESC
LIMIT ?1";
const THREAD_EXISTS: &str = "SELECT count(*) FROM threads WHERE id = ?1";
const SELECT_OUTCOMES: &str = "
SELECT count(*), count(*) FILTER (WHERE outcome = 'complete'),
       count(*) FILTER (WHERE outcome = 'partial'), count(*) FILTER (WHERE outcome = 'failed'),
       coalesce(sum(input_tokens), 0), coalesce(sum(output_tokens), 0), count(input_tokens)
FROM query_statistics";
const SELECT_HISTORY_COUNTS: &str = "
SELECT (SELECT count(*) FROM threads), (SELECT count(*) FROM turns),
       coalesce((SELECT threads_cleared FROM history_expiry), 0)";
const SELECT_TARGETS: &str = "
SELECT targets.provider_kind, targets.base_url, targets.model,
       (SELECT count(*) FROM query_statistics AS s WHERE s.provider_kind = targets.provider_kind
            AND s.base_url = targets.base_url AND s.model = targets.model),
       health.last_success_at_ms, health.last_failure_at_ms, health.last_failure_class
FROM (SELECT provider_kind, base_url, model FROM query_statistics
      UNION SELECT provider_kind, base_url, model FROM provider_health) AS targets
LEFT JOIN provider_health AS health USING (provider_kind, base_url, model)
ORDER BY targets.provider_kind, targets.base_url, targets.model";

/// A complete thread as `ask thread` shows it, including partial turns.
pub struct ThreadView {
    pub id: i64,
    pub profile: String,
    pub model: String,
    pub turns: Vec<StoredTurn>,
}

/// One recorded turn. `reason` is present exactly when the turn is partial.
pub struct StoredTurn {
    pub prompt: String,
    pub answer: String,
    pub reason: Option<String>,
}

/// One entry in the recent-thread list.
pub struct ThreadSummary {
    pub id: i64,
    pub updated_at_ms: i64,
    pub profile: String,
    pub model: String,
    pub turns: i64,
    pub opening: String,
    pub current: bool,
}

/// Text-free totals for `ask stats`.
#[derive(Default)]
pub struct Summary {
    pub queries: i64,
    pub complete: i64,
    pub partial: i64,
    pub failed: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub with_usage: i64,
    pub median_wall_ms: Option<i64>,
    pub median_first_token_ms: Option<i64>,
    pub threads: i64,
    pub turns: i64,
    pub threads_cleared: i64,
    pub targets: Vec<TargetHealth>,
}

/// Query count and the latest health observations for one provider target.
pub struct TargetHealth {
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub queries: i64,
    pub last_success_at_ms: Option<i64>,
    pub last_failure_at_ms: Option<i64>,
    pub last_failure_class: Option<String>,
}

impl Store {
    /// The current thread with every turn, complete or partial.
    pub fn current_view(&mut self) -> Result<Option<ThreadView>, StoreError> {
        let transaction = self.connection.transaction()?;
        let id: Option<i64> = transaction
            .query_row(SELECT_CURRENT_ID, [], |row| row.get(0))
            .optional()?;
        let view = match id {
            Some(id) => Some(view(&transaction, id)?),
            None => None,
        };
        transaction.commit()?;
        Ok(view)
    }

    /// Up to `limit` threads, most recently continued first.
    pub fn recent(&self, limit: i64) -> Result<Vec<ThreadSummary>, StoreError> {
        let mut statement = self.connection.prepare(SELECT_RECENT)?;
        let rows = statement.query_map([limit], thread_summary)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn has_thread(&self, id: i64) -> Result<bool, StoreError> {
        let count: i64 = self
            .connection
            .query_row(THREAD_EXISTS, [id], |row| row.get(0))?;
        Ok(count > 0)
    }

    /// Makes `id` the current thread. Returns `false` when no such thread exists.
    pub fn select(&mut self, id: i64) -> Result<bool, StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exists: i64 = transaction.query_row(THREAD_EXISTS, [id], |row| row.get(0))?;
        if exists == 0 {
            return Ok(false);
        }
        transaction.execute(MAKE_CURRENT, [id])?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn summary(&mut self) -> Result<Summary, StoreError> {
        let transaction = self.connection.transaction()?;
        let mut summary = transaction.query_row(SELECT_OUTCOMES, [], outcomes)?;
        summary.median_wall_ms = median(&transaction, "wall_ms")?;
        summary.median_first_token_ms = median(&transaction, "first_token_ms")?;
        (summary.threads, summary.turns, summary.threads_cleared) =
            transaction.query_row(SELECT_HISTORY_COUNTS, [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
        summary.targets = transaction
            .prepare(SELECT_TARGETS)?
            .query_map([], target_health)?
            .collect::<rusqlite::Result<_>>()?;
        transaction.commit()?;
        Ok(summary)
    }
}

fn view(transaction: &Transaction<'_>, id: i64) -> rusqlite::Result<ThreadView> {
    let (profile, model) =
        transaction.query_row(SELECT_HEADER, [id], |row| Ok((row.get(1)?, row.get(2)?)))?;
    let turns = transaction
        .prepare(SELECT_TURNS)?
        .query_map([id], |row| {
            Ok(StoredTurn {
                prompt: row.get(0)?,
                answer: row.get(1)?,
                reason: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ThreadView {
        id,
        profile,
        model,
        turns,
    })
}

fn thread_summary(row: &Row<'_>) -> rusqlite::Result<ThreadSummary> {
    Ok(ThreadSummary {
        id: row.get(0)?,
        updated_at_ms: row.get(1)?,
        profile: row.get(2)?,
        model: row.get(3)?,
        turns: row.get(4)?,
        opening: row.get(5)?,
        current: row.get(6)?,
    })
}

fn outcomes(row: &Row<'_>) -> rusqlite::Result<Summary> {
    Ok(Summary {
        queries: row.get(0)?,
        complete: row.get(1)?,
        partial: row.get(2)?,
        failed: row.get(3)?,
        input_tokens: row.get(4)?,
        output_tokens: row.get(5)?,
        with_usage: row.get(6)?,
        ..Summary::default()
    })
}

/// The lower median of `column` over complete queries that recorded it.
fn median(transaction: &Transaction<'_>, column: &str) -> rusqlite::Result<Option<i64>> {
    let filter = format!("outcome = 'complete' AND {column} IS NOT NULL");
    let sql = format!(
        "SELECT {column} FROM query_statistics WHERE {filter} ORDER BY {column} LIMIT 1 OFFSET (SELECT (count(*) - 1) / 2 FROM query_statistics WHERE {filter})"
    );
    transaction.query_row(&sql, [], |row| row.get(0)).optional()
}

fn target_health(row: &Row<'_>) -> rusqlite::Result<TargetHealth> {
    Ok(TargetHealth {
        kind: row.get(0)?,
        base_url: row.get(1)?,
        model: row.get(2)?,
        queries: row.get(3)?,
        last_success_at_ms: row.get(4)?,
        last_failure_at_ms: row.get(5)?,
        last_failure_class: row.get(6)?,
    })
}
