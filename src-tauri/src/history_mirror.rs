//! Coalescing, crash-safe JSONL history snapshots.
//!
//! Production refreshes carry only a generation signal. After debounce, a
//! worker-owned read-only SQLite connection loads the latest committed rows,
//! then performs event decoding, serialization, flushing, syncing, and atomic
//! replacement outside the application's database mutex. Owned-row scheduling
//! remains available for isolated tests.

use crate::event::decode_event_blob;
use crate::private_fs::{
    create_private_temp_file, harden_private_file_if_exists, prepare_private_output_path,
    resolve_private_path, PrivateFsError, PrivateFsErrorKind,
};
use crate::store::Database;
use copy_event_listener::event::Event;
use serde::Serialize;
use std::fmt;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const DEFAULT_HISTORY_MIRROR_DEBOUNCE: Duration = Duration::from_millis(200);

/// One owned row read by the background mirror worker.
///
/// Keep `event_data` in its compact persisted form. The mirror worker decodes
/// it after the database lock has been released.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistorySnapshotRow {
    pub content_hash: String,
    pub event_data: Vec<u8>,
    pub data_type: String,
    pub display: Vec<u8>,
    pub timestamp: i64,
    pub source_bundle_id: Option<String>,
    pub is_remote_clipboard: bool,
}

impl HistorySnapshotRow {
    pub fn new(
        content_hash: String,
        event_data: Vec<u8>,
        data_type: String,
        display: Vec<u8>,
        timestamp: i64,
    ) -> Self {
        Self {
            content_hash,
            event_data,
            data_type,
            display,
            timestamp,
            source_bundle_id: None,
            is_remote_clipboard: false,
        }
    }

    pub fn with_pasteboard_metadata(
        mut self,
        source_bundle_id: Option<String>,
        is_remote_clipboard: bool,
    ) -> Self {
        self.source_bundle_id = source_bundle_id;
        self.is_remote_clipboard = is_remote_clipboard;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryMirrorConfig {
    pub path: PathBuf,
    pub max_data_bytes: usize,
    pub debounce: Duration,
}

impl HistoryMirrorConfig {
    pub fn new(path: PathBuf, max_data_bytes: usize) -> Self {
        Self {
            path,
            max_data_bytes,
            debounce: DEFAULT_HISTORY_MIRROR_DEBOUNCE,
        }
    }

    pub fn with_debounce(mut self, debounce: Duration) -> Self {
        self.debounce = debounce;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryMirrorErrorKind {
    Path(PrivateFsErrorKind),
    EventDecode,
    Serialize,
    SnapshotRead,
    Write,
    WorkerStopped,
    GenerationExhausted,
    TimedOut,
    InjectedFailure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryMirrorError {
    kind: HistoryMirrorErrorKind,
    stage: &'static str,
}

impl HistoryMirrorError {
    pub fn kind(&self) -> HistoryMirrorErrorKind {
        self.kind
    }

    pub fn stage(&self) -> &'static str {
        self.stage
    }

    fn new(kind: HistoryMirrorErrorKind, stage: &'static str) -> Self {
        Self { kind, stage }
    }

    fn path(error: PrivateFsError, stage: &'static str) -> Self {
        Self::new(HistoryMirrorErrorKind::Path(error.kind()), stage)
    }
}

impl fmt::Display for HistoryMirrorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self.kind {
            HistoryMirrorErrorKind::Path(_) => "private path validation failed",
            HistoryMirrorErrorKind::EventDecode => "a persisted event could not be decoded",
            HistoryMirrorErrorKind::Serialize => "a snapshot row could not be serialized",
            HistoryMirrorErrorKind::SnapshotRead => {
                "the committed history snapshot could not be read"
            }
            HistoryMirrorErrorKind::Write => "the snapshot could not be written",
            HistoryMirrorErrorKind::WorkerStopped => "the mirror worker stopped",
            HistoryMirrorErrorKind::GenerationExhausted => {
                "the snapshot generation counter is exhausted"
            }
            HistoryMirrorErrorKind::TimedOut => "the bounded wait timed out",
            HistoryMirrorErrorKind::InjectedFailure => "a test failure was injected",
        };
        write!(
            formatter,
            "history mirror failed during {}: {}",
            self.stage, reason
        )
    }
}

impl std::error::Error for HistoryMirrorError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryMirrorStatus {
    pub requested_generation: u64,
    pub settled_generation: u64,
    pub written_generation: u64,
    pub last_error: Option<HistoryMirrorError>,
    pub stopped: bool,
}

/// A single-writer background JSONL mirror.
pub struct HistoryMirror {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl fmt::Debug for HistoryMirror {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HistoryMirror")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl HistoryMirror {
    pub fn start(config: HistoryMirrorConfig) -> Result<Self, HistoryMirrorError> {
        Self::start_with_source(config, SnapshotSource::ScheduledRows, Arc::new(NoopHook))
    }

    /// Starts a production mirror whose worker reads the latest committed
    /// history through its own read-only SQLite connection.
    ///
    /// Refresh requests carry no clipboard BLOBs, so mutations never clone the
    /// full history while holding the application's database mutex. Because
    /// the worker reads current committed state, an older request scheduled
    /// after a newer mutation cannot overwrite the mirror with stale rows.
    pub fn start_database(
        config: HistoryMirrorConfig,
        database_path: PathBuf,
    ) -> Result<Self, HistoryMirrorError> {
        let database_path = resolve_private_path(&database_path)
            .map_err(|error| HistoryMirrorError::path(error, "resolve database source"))?;
        harden_private_file_if_exists(&database_path)
            .map_err(|error| HistoryMirrorError::path(error, "validate database source"))?;
        Self::start_with_source(
            config,
            SnapshotSource::Database(database_path),
            Arc::new(NoopHook),
        )
    }

    /// Replaces the pending snapshot and returns its monotonic generation.
    ///
    /// This method performs no event decoding, serialization, or file I/O.
    /// Call it only after the associated database mutation has committed.
    pub fn schedule(&self, rows: Vec<HistorySnapshotRow>) -> Result<u64, HistoryMirrorError> {
        if !matches!(&self.shared.source, SnapshotSource::ScheduledRows) {
            return Err(HistoryMirrorError::new(
                HistoryMirrorErrorKind::SnapshotRead,
                "schedule owned rows for database mirror",
            ));
        }
        self.schedule_pending(Some(rows))
    }

    /// Requests a refresh from the worker-owned database connection.
    pub fn schedule_refresh(&self) -> Result<u64, HistoryMirrorError> {
        if !matches!(&self.shared.source, SnapshotSource::Database(_)) {
            return Err(HistoryMirrorError::new(
                HistoryMirrorErrorKind::SnapshotRead,
                "schedule database refresh for owned-row mirror",
            ));
        }
        self.schedule_pending(None)
    }

    fn schedule_pending(
        &self,
        rows: Option<Vec<HistorySnapshotRow>>,
    ) -> Result<u64, HistoryMirrorError> {
        let replaced = {
            let mut state = lock_recover(&self.shared.state);
            if state.shutdown || state.exited {
                return Err(HistoryMirrorError::new(
                    HistoryMirrorErrorKind::WorkerStopped,
                    "schedule",
                ));
            }

            let generation = self
                .shared
                .latest_requested
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    current.checked_add(1)
                })
                .map(|previous| previous + 1)
                .map_err(|_| {
                    HistoryMirrorError::new(HistoryMirrorErrorKind::GenerationExhausted, "schedule")
                })?;

            let deadline = Instant::now()
                .checked_add(self.shared.config.debounce)
                .unwrap_or_else(Instant::now);
            let replaced = state.pending.replace(PendingSnapshot {
                generation,
                rows,
                deadline,
            });
            self.shared.wake.notify_one();
            (generation, replaced)
        };

        // Potentially large superseded rows are dropped outside the worker
        // state mutex so a concurrent flush/status call is not held up.
        drop(replaced.1);
        Ok(replaced.0)
    }

    /// Forces the latest requested generation past debounce and waits at most
    /// `timeout` for it to be written or to fail.
    pub fn flush(&self, timeout: Duration) -> Result<(), HistoryMirrorError> {
        let target = self.shared.latest_requested.load(Ordering::Acquire);
        if target == 0 {
            return Ok(());
        }

        let started = Instant::now();
        let mut state = lock_recover(&self.shared.state);
        state.force_through = state.force_through.max(target);
        self.shared.wake.notify_one();

        loop {
            let latest = self.shared.latest_requested.load(Ordering::Acquire);
            if let Some(result) = settled_result(&state, target, latest) {
                return result;
            }
            if state.exited {
                return Err(HistoryMirrorError::new(
                    HistoryMirrorErrorKind::WorkerStopped,
                    "flush",
                ));
            }

            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(HistoryMirrorError::new(
                    HistoryMirrorErrorKind::TimedOut,
                    "flush",
                ));
            }
            let (next, timed_out) = wait_timeout_recover(&self.shared.wake, state, remaining);
            state = next;
            let latest = self.shared.latest_requested.load(Ordering::Acquire);
            if timed_out && settled_result(&state, target, latest).is_none() {
                return Err(HistoryMirrorError::new(
                    HistoryMirrorErrorKind::TimedOut,
                    "flush",
                ));
            }
        }
    }

    /// Flushes the latest generation and stops the worker within `timeout`.
    ///
    /// A timeout detaches nothing immediately: callers may release a stalled
    /// test hook or slow filesystem and call `shutdown` again. Dropping the
    /// mirror remains non-blocking.
    pub fn shutdown(&self, timeout: Duration) -> Result<(), HistoryMirrorError> {
        let target = self.shared.latest_requested.load(Ordering::Acquire);
        let started = Instant::now();
        let mut state = lock_recover(&self.shared.state);
        state.shutdown = true;
        state.force_through = state.force_through.max(target);
        self.shared.wake.notify_one();

        while !state.exited {
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(HistoryMirrorError::new(
                    HistoryMirrorErrorKind::TimedOut,
                    "shutdown",
                ));
            }
            let (next, timed_out) = wait_timeout_recover(&self.shared.wake, state, remaining);
            state = next;
            if timed_out && !state.exited {
                return Err(HistoryMirrorError::new(
                    HistoryMirrorErrorKind::TimedOut,
                    "shutdown",
                ));
            }
        }

        let result = if target == 0 {
            Ok(())
        } else {
            settled_result(&state, target, target).unwrap_or_else(|| {
                Err(HistoryMirrorError::new(
                    HistoryMirrorErrorKind::WorkerStopped,
                    "shutdown",
                ))
            })
        };
        drop(state);

        if let Some(worker) = lock_recover(&self.worker).take() {
            if worker.join().is_err() {
                return Err(HistoryMirrorError::new(
                    HistoryMirrorErrorKind::WorkerStopped,
                    "join",
                ));
            }
        }
        result
    }

    pub fn status(&self) -> HistoryMirrorStatus {
        let state = lock_recover(&self.shared.state);
        HistoryMirrorStatus {
            requested_generation: self.shared.latest_requested.load(Ordering::Acquire),
            settled_generation: state.settled_generation,
            written_generation: state.written_generation,
            last_error: state.last_failure.as_ref().map(|(_, error)| error.clone()),
            stopped: state.exited,
        }
    }

    fn start_with_source(
        mut config: HistoryMirrorConfig,
        source: SnapshotSource,
        hook: Arc<dyn MirrorHook>,
    ) -> Result<Self, HistoryMirrorError> {
        config.path = prepare_private_output_path(&config.path)
            .map_err(|error| HistoryMirrorError::path(error, "start path validation"))?;
        if matches!(&source, SnapshotSource::Database(path) if path == &config.path) {
            return Err(HistoryMirrorError::new(
                HistoryMirrorErrorKind::SnapshotRead,
                "reject database as mirror destination",
            ));
        }

        let shared = Arc::new(Shared {
            config,
            source,
            latest_requested: AtomicU64::new(0),
            state: Mutex::new(WorkerState::default()),
            wake: Condvar::new(),
            hook,
        });
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("copy-stack-history-mirror".to_string())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    worker_loop(&worker_shared);
                }));
                if result.is_err() {
                    let generation = worker_shared.latest_requested.load(Ordering::Acquire);
                    let mut state = lock_recover(&worker_shared.state);
                    state.settled_generation = state.settled_generation.max(generation);
                    state.last_failure = Some((
                        generation,
                        HistoryMirrorError::new(
                            HistoryMirrorErrorKind::WorkerStopped,
                            "worker panic containment",
                        ),
                    ));
                }
                let mut state = lock_recover(&worker_shared.state);
                state.exited = true;
                worker_shared.wake.notify_all();
            })
            .map_err(|_| {
                HistoryMirrorError::new(HistoryMirrorErrorKind::WorkerStopped, "start worker")
            })?;

        Ok(Self {
            shared,
            worker: Mutex::new(Some(worker)),
        })
    }
}

impl Drop for HistoryMirror {
    fn drop(&mut self) {
        {
            let mut state = lock_recover(&self.shared.state);
            state.shutdown = true;
            state.force_through = u64::MAX;
            self.shared.wake.notify_one();
        }
        // Dropping a JoinHandle detaches it. Destruction must never turn an
        // unexpectedly slow disk into an unbounded application-exit stall.
        let _ = lock_recover(&self.worker).take();
    }
}

struct Shared {
    config: HistoryMirrorConfig,
    source: SnapshotSource,
    latest_requested: AtomicU64,
    state: Mutex<WorkerState>,
    wake: Condvar,
    hook: Arc<dyn MirrorHook>,
}

enum SnapshotSource {
    ScheduledRows,
    Database(PathBuf),
}

#[derive(Default)]
struct WorkerState {
    pending: Option<PendingSnapshot>,
    force_through: u64,
    shutdown: bool,
    exited: bool,
    settled_generation: u64,
    written_generation: u64,
    last_failure: Option<(u64, HistoryMirrorError)>,
}

struct PendingSnapshot {
    generation: u64,
    rows: Option<Vec<HistorySnapshotRow>>,
    deadline: Instant,
}

enum WriteOutcome {
    Written,
    Superseded,
    Failed(HistoryMirrorError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WriteStage {
    BeforeTempCreate,
    AfterTempCreate,
    AfterSerialize,
    AfterFlush,
    AfterSync,
    BeforeCommit,
    AfterCommit,
}

trait MirrorHook: Send + Sync + 'static {
    fn reach(&self, stage: WriteStage, generation: u64) -> Result<(), HistoryMirrorError>;
}

struct NoopHook;

impl MirrorHook for NoopHook {
    fn reach(&self, _stage: WriteStage, _generation: u64) -> Result<(), HistoryMirrorError> {
        Ok(())
    }
}

fn worker_loop(shared: &Arc<Shared>) {
    loop {
        let pending = {
            let mut state = lock_recover(&shared.state);
            loop {
                let Some(pending) = state.pending.as_ref() else {
                    if state.shutdown {
                        return;
                    }
                    state = wait_recover(&shared.wake, state);
                    continue;
                };

                let now = Instant::now();
                let forced = state.shutdown || state.force_through >= pending.generation;
                if forced || now >= pending.deadline {
                    if let Some(pending) = state.pending.take() {
                        break pending;
                    }
                    continue;
                }

                let wait = pending.deadline.saturating_duration_since(now);
                let (next, _) = wait_timeout_recover(&shared.wake, state, wait);
                state = next;
            }
        };

        let generation = pending.generation;
        let outcome = write_snapshot(shared, pending);
        let mut state = lock_recover(&shared.state);
        state.settled_generation = state.settled_generation.max(generation);
        match outcome {
            WriteOutcome::Written => {
                state.written_generation = state.written_generation.max(generation);
                if state
                    .last_failure
                    .as_ref()
                    .is_some_and(|(failed_generation, _)| *failed_generation <= generation)
                {
                    state.last_failure = None;
                }
            }
            WriteOutcome::Superseded => {}
            WriteOutcome::Failed(error) => {
                state.last_failure = Some((generation, error));
            }
        }
        shared.wake.notify_all();
    }
}

fn write_snapshot(shared: &Shared, mut pending: PendingSnapshot) -> WriteOutcome {
    match write_snapshot_inner(shared, &mut pending) {
        Ok(true) => WriteOutcome::Written,
        Ok(false) => WriteOutcome::Superseded,
        Err(error) => WriteOutcome::Failed(error),
    }
}

fn write_snapshot_inner(
    shared: &Shared,
    pending: &mut PendingSnapshot,
) -> Result<bool, HistoryMirrorError> {
    let generation = pending.generation;
    let mut scheduled_rows = pending.rows.take();
    let output_path = prepare_private_output_path(&shared.config.path)
        .map_err(|error| HistoryMirrorError::path(error, "write path validation"))?;

    shared
        .hook
        .reach(WriteStage::BeforeTempCreate, generation)?;
    let mut temp = create_private_temp_file(&output_path, generation)
        .map_err(|error| HistoryMirrorError::path(error, "create temporary file"))?;
    shared.hook.reach(WriteStage::AfterTempCreate, generation)?;

    {
        let mut writer = BufWriter::new(temp.file_mut());
        if let Some(rows) = scheduled_rows.as_mut() {
            rows.sort_by(|left, right| {
                right
                    .timestamp
                    .cmp(&left.timestamp)
                    .then_with(|| left.content_hash.cmp(&right.content_hash))
            });
            for row in rows {
                write_jsonl_row(&mut writer, row, shared.config.max_data_bytes)?;
            }
        } else {
            match &shared.source {
                SnapshotSource::Database(path) => {
                    let mut write_error = None;
                    Database::visit_history_snapshot_rows_from_path(path, |row| {
                        match write_jsonl_row(&mut writer, &row, shared.config.max_data_bytes) {
                            Ok(()) => true,
                            Err(error) => {
                                write_error = Some(error);
                                false
                            }
                        }
                    })
                    .map_err(|_| {
                        HistoryMirrorError::new(
                            HistoryMirrorErrorKind::SnapshotRead,
                            "read committed database snapshot",
                        )
                    })?;
                    if let Some(error) = write_error {
                        return Err(error);
                    }
                }
                SnapshotSource::ScheduledRows => {
                    return Err(HistoryMirrorError::new(
                        HistoryMirrorErrorKind::SnapshotRead,
                        "load missing owned snapshot rows",
                    ));
                }
            }
        }
        shared.hook.reach(WriteStage::AfterSerialize, generation)?;
        writer.flush().map_err(|_| {
            HistoryMirrorError::new(HistoryMirrorErrorKind::Write, "flush temporary file")
        })?;
    }
    shared.hook.reach(WriteStage::AfterFlush, generation)?;

    temp.sync_all()
        .map_err(|error| HistoryMirrorError::path(error, "sync temporary file"))?;
    shared.hook.reach(WriteStage::AfterSync, generation)?;

    if shared.latest_requested.load(Ordering::Acquire) != generation {
        return Ok(false);
    }

    shared.hook.reach(WriteStage::BeforeCommit, generation)?;
    if shared.latest_requested.load(Ordering::Acquire) != generation {
        return Ok(false);
    }

    temp.commit(&output_path)
        .map_err(|error| HistoryMirrorError::path(error, "atomic replace"))?;
    shared.hook.reach(WriteStage::AfterCommit, generation)?;
    Ok(true)
}

fn write_jsonl_row<W: Write>(
    writer: &mut W,
    row: &HistorySnapshotRow,
    max_data_bytes: usize,
) -> Result<(), HistoryMirrorError> {
    let event = decode_event_blob(&row.event_data).map_err(|_| {
        HistoryMirrorError::new(
            HistoryMirrorErrorKind::EventDecode,
            "decode persisted event",
        )
    })?;
    let record = history_jsonl_record(row, &event, max_data_bytes);
    serde_json::to_writer(&mut *writer, &record).map_err(|_| {
        HistoryMirrorError::new(HistoryMirrorErrorKind::Serialize, "serialize JSONL row")
    })?;
    writer
        .write_all(b"\n")
        .map_err(|_| HistoryMirrorError::new(HistoryMirrorErrorKind::Write, "write JSONL newline"))
}

fn settled_result(
    state: &WorkerState,
    target: u64,
    latest_requested: u64,
) -> Option<Result<(), HistoryMirrorError>> {
    if state.written_generation >= target {
        return Some(Ok(()));
    }
    if state.settled_generation < target {
        return None;
    }
    // A target can settle as superseded (or fail) while a newer coalesced
    // generation is still pending. The newer successful write also satisfies
    // this flush, so do not report a false worker failure early.
    if latest_requested > target {
        return None;
    }
    if let Some((generation, error)) = &state.last_failure {
        if *generation == target {
            return Some(Err(error.clone()));
        }
    }
    Some(Err(HistoryMirrorError::new(
        HistoryMirrorErrorKind::WorkerStopped,
        "settle latest generation",
    )))
}

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wait_recover<'a, T>(condition: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    condition
        .wait(guard)
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wait_timeout_recover<'a, T>(
    condition: &Condvar,
    guard: MutexGuard<'a, T>,
    timeout: Duration,
) -> (MutexGuard<'a, T>, bool) {
    match condition.wait_timeout(guard, timeout) {
        Ok((guard, result)) => (guard, result.timed_out()),
        Err(poisoned) => {
            let (guard, result) = poisoned.into_inner();
            (guard, result.timed_out())
        }
    }
}

#[derive(Serialize)]
struct HistoryJsonlRecord<'a> {
    content_hash: &'a str,
    data_type: &'a str,
    timestamp: i64,
    display: HistoryJsonlBytes,
    event_data: HistoryJsonlEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_bundle_id: Option<&'a str>,
    #[serde(skip_serializing_if = "is_false")]
    is_remote_clipboard: bool,
}

#[derive(Serialize)]
struct HistoryJsonlEvent {
    items: Vec<HistoryJsonlItem>,
}

#[derive(Serialize)]
struct HistoryJsonlItem {
    data_list: Vec<HistoryJsonlData>,
}

#[derive(Serialize)]
struct HistoryJsonlData {
    #[serde(rename = "type")]
    data_type: String,
    data: HistoryJsonlBytes,
}

#[derive(Serialize)]
struct HistoryJsonlBytes {
    byte_len: usize,
    truncated: bool,
    encoding: &'static str,
    value: String,
}

impl HistoryJsonlBytes {
    fn new(bytes: &[u8], max_data_bytes: usize) -> Self {
        let byte_len = bytes.len();
        let truncated = byte_len > max_data_bytes;

        if let Some(value) = Self::utf8_value(bytes, max_data_bytes) {
            return Self {
                byte_len,
                truncated,
                encoding: "utf8",
                value,
            };
        }

        let visible_len = byte_len.min(max_data_bytes);
        Self {
            byte_len,
            truncated,
            encoding: "hex",
            value: hex_bytes(&bytes[..visible_len]),
        }
    }

    fn utf8_value(bytes: &[u8], max_data_bytes: usize) -> Option<String> {
        let text = std::str::from_utf8(bytes).ok()?;
        if bytes.len() <= max_data_bytes {
            return Some(text.to_string());
        }

        let mut end = max_data_bytes;
        while !text.is_char_boundary(end) {
            end = end.checked_sub(1)?;
        }
        Some(text[..end].to_string())
    }
}

fn history_jsonl_record<'a>(
    row: &'a HistorySnapshotRow,
    event: &Event,
    max_data_bytes: usize,
) -> HistoryJsonlRecord<'a> {
    HistoryJsonlRecord {
        content_hash: &row.content_hash,
        data_type: &row.data_type,
        timestamp: row.timestamp,
        display: HistoryJsonlBytes::new(&row.display, max_data_bytes),
        event_data: HistoryJsonlEvent {
            items: event
                .items
                .iter()
                .map(|item| HistoryJsonlItem {
                    data_list: item
                        .data_list
                        .iter()
                        .map(|data| HistoryJsonlData {
                            data_type: data.r#type.clone(),
                            data: HistoryJsonlBytes::new(&data.data, max_data_bytes),
                        })
                        .collect(),
                })
                .collect(),
        },
        source_bundle_id: row.source_bundle_id.as_deref(),
        is_remote_clipboard: row.is_remote_clipboard,
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::event::encode_event_blob;
    use copy_event_listener::event::{Data, Item};
    use serde_json::Value;
    use std::fs::Permissions;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    use std::sync::mpsc;

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "copy-stack-history-mirror-test-{}-{}-{}",
                std::process::id(),
                sequence,
                label
            ));
            std::fs::create_dir(&path).expect("test directory should be created");
            std::fs::set_permissions(&path, Permissions::from_mode(0o700))
                .expect("test directory should be private");
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    struct ClosureHook<F>(F);

    impl<F> MirrorHook for ClosureHook<F>
    where
        F: Fn(WriteStage, u64) -> Result<(), HistoryMirrorError> + Send + Sync + 'static,
    {
        fn reach(&self, stage: WriteStage, generation: u64) -> Result<(), HistoryMirrorError> {
            (self.0)(stage, generation)
        }
    }

    fn start_with<F>(
        config: HistoryMirrorConfig,
        hook: F,
    ) -> Result<HistoryMirror, HistoryMirrorError>
    where
        F: Fn(WriteStage, u64) -> Result<(), HistoryMirrorError> + Send + Sync + 'static,
    {
        HistoryMirror::start_with_source(
            config,
            SnapshotSource::ScheduledRows,
            Arc::new(ClosureHook(hook)),
        )
    }

    fn start_database_with<F>(
        config: HistoryMirrorConfig,
        database_path: PathBuf,
        hook: F,
    ) -> Result<HistoryMirror, HistoryMirrorError>
    where
        F: Fn(WriteStage, u64) -> Result<(), HistoryMirrorError> + Send + Sync + 'static,
    {
        let database_path = resolve_private_path(&database_path)
            .map_err(|error| HistoryMirrorError::path(error, "resolve test database source"))?;
        harden_private_file_if_exists(&database_path)
            .map_err(|error| HistoryMirrorError::path(error, "validate test database source"))?;
        HistoryMirror::start_with_source(
            config,
            SnapshotSource::Database(database_path),
            Arc::new(ClosureHook(hook)),
        )
    }

    fn text_row(hash: &str, text: &[u8], display: &[u8], timestamp: i64) -> HistorySnapshotRow {
        let event = Event {
            items: vec![Item {
                data_list: vec![Data {
                    r#type: "public.utf8-plain-text".to_string(),
                    data: text.to_vec(),
                }],
            }],
        };
        HistorySnapshotRow::new(
            hash.to_string(),
            encode_event_blob(&event).expect("event should encode"),
            "text".to_string(),
            display.to_vec(),
            timestamp,
        )
    }

    fn config(path: PathBuf, debounce: Duration) -> HistoryMirrorConfig {
        HistoryMirrorConfig::new(path, 4).with_debounce(debounce)
    }

    fn jsonl_values(path: &Path) -> Vec<Value> {
        let text = std::fs::read_to_string(path).expect("JSONL should be readable");
        text.lines()
            .map(|line| serde_json::from_str(line).expect("line should be valid JSON"))
            .collect()
    }

    fn assert_no_temporary_files(directory: &Path) {
        let leftovers: Vec<_> = std::fs::read_dir(directory)
            .expect("directory should list")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains(".copy-stack-jsonl.")
            })
            .collect();
        assert!(leftovers.is_empty(), "temporary files remained");
    }

    #[test]
    fn writes_compatible_jsonl_with_utf8_and_binary_truncation() {
        let root = TestDirectory::new("format");
        let output = root.path.join("history.jsonl");
        let mirror = HistoryMirror::start(config(output.clone(), Duration::ZERO))
            .expect("mirror should start");

        let binary_event = Event {
            items: vec![Item {
                data_list: vec![Data {
                    r#type: "public.png".to_string(),
                    data: vec![0, 1, 2, 255, 4],
                }],
            }],
        };
        let binary = HistorySnapshotRow::new(
            "b".repeat(64),
            encode_event_blob(&binary_event).expect("event should encode"),
            "png".to_string(),
            vec![0, 1, 2, 255, 4],
            10,
        );
        let utf8 = text_row(
            &"a".repeat(64),
            "éclair".as_bytes(),
            "éclair".as_bytes(),
            20,
        )
        .with_pasteboard_metadata(Some("com.example.source".to_string()), true);

        mirror
            .schedule(vec![binary, utf8])
            .expect("snapshot should schedule");
        mirror
            .flush(Duration::from_secs(2))
            .expect("snapshot should flush");

        let values = jsonl_values(&output);
        assert_eq!(values.len(), 2);
        assert_eq!(values[0]["content_hash"], "a".repeat(64));
        assert_eq!(values[0]["display"]["byte_len"], 7);
        assert_eq!(values[0]["display"]["truncated"], true);
        assert_eq!(values[0]["display"]["encoding"], "utf8");
        assert_eq!(values[0]["display"]["value"], "écl");
        assert_eq!(values[0]["source_bundle_id"], "com.example.source");
        assert_eq!(values[0]["is_remote_clipboard"], true);
        assert_eq!(values[1]["display"]["encoding"], "hex");
        assert_eq!(values[1]["display"]["value"], "000102ff");
        assert!(values[1].get("source_bundle_id").is_none());
        assert!(values[1].get("is_remote_clipboard").is_none());
        assert_eq!(
            std::fs::symlink_metadata(&output)
                .expect("output metadata should exist")
                .mode()
                & 0o7777,
            0o600
        );

        mirror
            .shutdown(Duration::from_secs(2))
            .expect("mirror should stop");
    }

    #[test]
    fn debounce_coalesces_pending_snapshots_to_the_latest_generation() {
        let root = TestDirectory::new("coalesce");
        let output = root.path.join("history.jsonl");
        let commits = Arc::new(AtomicUsize::new(0));
        let hook_commits = Arc::clone(&commits);
        let mirror = start_with(
            config(output.clone(), Duration::from_secs(1)),
            move |stage, _| {
                if stage == WriteStage::AfterCommit {
                    hook_commits.fetch_add(1, Ordering::Relaxed);
                }
                Ok(())
            },
        )
        .expect("mirror should start");

        mirror
            .schedule(vec![text_row("one", b"one", b"one", 1)])
            .expect("first snapshot should schedule");
        mirror
            .schedule(vec![text_row("two", b"two", b"two", 2)])
            .expect("second snapshot should schedule");
        let latest = mirror
            .schedule(vec![text_row("three", b"three", b"three", 3)])
            .expect("third snapshot should schedule");
        mirror
            .flush(Duration::from_secs(2))
            .expect("latest snapshot should flush");

        assert_eq!(latest, 3);
        assert_eq!(commits.load(Ordering::Relaxed), 1);
        let values = jsonl_values(&output);
        assert_eq!(values.len(), 1);
        assert_eq!(values[0]["content_hash"], "three");
        mirror
            .shutdown(Duration::from_secs(2))
            .expect("mirror should stop");
    }

    #[test]
    fn empty_snapshot_atomically_clears_the_mirror() {
        let root = TestDirectory::new("empty");
        let output = root.path.join("history.jsonl");
        let mirror = HistoryMirror::start(config(output.clone(), Duration::ZERO))
            .expect("mirror should start");
        mirror
            .schedule(vec![text_row("one", b"one", b"one", 1)])
            .expect("non-empty snapshot should schedule");
        mirror
            .flush(Duration::from_secs(2))
            .expect("non-empty snapshot should flush");
        assert!(!std::fs::read(&output)
            .expect("snapshot should be readable")
            .is_empty());

        mirror
            .schedule(Vec::new())
            .expect("empty snapshot should schedule");
        mirror
            .flush(Duration::from_secs(2))
            .expect("empty snapshot should flush");
        assert!(
            std::fs::read(&output)
                .expect("empty snapshot should be readable")
                .is_empty(),
            "clear-all must produce a complete empty snapshot"
        );
        mirror
            .shutdown(Duration::from_secs(2))
            .expect("mirror should stop");
    }

    #[test]
    fn a_superseded_settlement_waits_for_the_newer_generation() {
        let mut state = WorkerState {
            settled_generation: 1,
            ..WorkerState::default()
        };
        assert_eq!(settled_result(&state, 1, 2), None);

        state.written_generation = 2;
        state.settled_generation = 2;
        assert_eq!(settled_result(&state, 1, 2), Some(Ok(())));
    }

    #[test]
    fn stale_in_flight_generation_never_commits_over_the_latest() {
        let root = TestDirectory::new("ordering");
        let output = root.path.join("history.jsonl");
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let entered_tx = Mutex::new(Some(entered_tx));
        let release_rx = Mutex::new(release_rx);
        let committed = Arc::new(Mutex::new(Vec::new()));
        let hook_committed = Arc::clone(&committed);
        let mirror = start_with(
            config(output.clone(), Duration::ZERO),
            move |stage, generation| {
                if stage == WriteStage::AfterSync && generation == 1 {
                    if let Some(sender) = lock_recover(&entered_tx).take() {
                        sender.send(()).map_err(|_| {
                            HistoryMirrorError::new(
                                HistoryMirrorErrorKind::InjectedFailure,
                                "signal blocked generation",
                            )
                        })?;
                    }
                    lock_recover(&release_rx).recv().map_err(|_| {
                        HistoryMirrorError::new(
                            HistoryMirrorErrorKind::InjectedFailure,
                            "release blocked generation",
                        )
                    })?;
                }
                if stage == WriteStage::AfterCommit {
                    lock_recover(&hook_committed).push(generation);
                }
                Ok(())
            },
        )
        .expect("mirror should start");

        mirror
            .schedule(vec![text_row("old", b"old", b"old", 1)])
            .expect("old snapshot should schedule");
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("old generation should reach the sync barrier");
        mirror
            .schedule(vec![text_row("new", b"new", b"new", 2)])
            .expect("new snapshot should schedule");
        release_tx
            .send(())
            .expect("old generation should be released");
        mirror
            .flush(Duration::from_secs(2))
            .expect("new generation should flush");

        assert_eq!(*lock_recover(&committed), vec![2]);
        assert_eq!(jsonl_values(&output)[0]["content_hash"], "new");
        mirror
            .shutdown(Duration::from_secs(2))
            .expect("mirror should stop");
    }

    #[test]
    fn database_refresh_forced_interleaving_commits_latest_database_state() {
        let root = TestDirectory::new("database-ordering");
        let database_path = root.path.join("copy_stack.db");
        let output = root.path.join("history.jsonl");
        let db = Database::open_path(&database_path).expect("database should open");
        db.insert_event(&Event {
            items: vec![Item {
                data_list: vec![Data {
                    r#type: "public.utf8-plain-text".to_string(),
                    data: b"first committed row".to_vec(),
                }],
            }],
        })
        .expect("first row should commit");

        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let entered_tx = Mutex::new(Some(entered_tx));
        let release_rx = Mutex::new(release_rx);
        let mirror = start_database_with(
            HistoryMirrorConfig::new(output.clone(), 4_096).with_debounce(Duration::ZERO),
            database_path,
            move |stage, generation| {
                if stage == WriteStage::AfterSync && generation == 1 {
                    if let Some(sender) = lock_recover(&entered_tx).take() {
                        sender.send(()).map_err(|_| {
                            HistoryMirrorError::new(
                                HistoryMirrorErrorKind::InjectedFailure,
                                "signal database snapshot barrier",
                            )
                        })?;
                    }
                    lock_recover(&release_rx).recv().map_err(|_| {
                        HistoryMirrorError::new(
                            HistoryMirrorErrorKind::InjectedFailure,
                            "release database snapshot barrier",
                        )
                    })?;
                }
                Ok(())
            },
        )
        .expect("database mirror should start");

        mirror
            .schedule_refresh()
            .expect("first database refresh should schedule");
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("first refresh should reach the forced barrier");

        db.insert_event(&Event {
            items: vec![Item {
                data_list: vec![Data {
                    r#type: "public.utf8-plain-text".to_string(),
                    data: b"second committed row".to_vec(),
                }],
            }],
        })
        .expect("second row should commit");
        mirror
            .schedule_refresh()
            .expect("second database refresh should schedule");
        release_tx
            .send(())
            .expect("first database refresh should be released");
        mirror
            .flush(Duration::from_secs(2))
            .expect("latest database refresh should flush");

        let values = jsonl_values(&output);
        assert_eq!(values.len(), 2);
        let exported_text = values
            .iter()
            .map(|value| {
                value["event_data"]["items"][0]["data_list"][0]["data"]["value"]
                    .as_str()
                    .expect("text value should exist")
            })
            .collect::<Vec<_>>();
        assert!(exported_text.contains(&"first committed row"));
        assert!(exported_text.contains(&"second committed row"));
        assert_eq!(mirror.status().written_generation, 2);
        mirror
            .shutdown(Duration::from_secs(2))
            .expect("database mirror should stop");
    }

    #[test]
    fn database_refresh_matches_compact_history_projection() {
        let root = TestDirectory::new("database-compact");
        let database_path = root.path.join("copy_stack.db");
        let output = root.path.join("history.jsonl");
        let db = Database::open_path(&database_path).expect("database should open");
        let event = Event {
            items: vec![Item {
                data_list: vec![
                    Data {
                        r#type: "public.html".to_string(),
                        data: b"<strong>compact mirror</strong>".to_vec(),
                    },
                    Data {
                        r#type: "public.utf8-plain-text".to_string(),
                        data: b"compact mirror".to_vec(),
                    },
                    Data {
                        r#type: crate::pasteboard_protocol::SOURCE_TYPE.to_string(),
                        data: b"com.example.compact".to_vec(),
                    },
                    Data {
                        r#type: crate::pasteboard_protocol::REMOTE_CLIPBOARD_TYPE.to_string(),
                        data: Vec::new(),
                    },
                ],
            }],
        };
        db.insert_event(&event)
            .expect("formatted source row should commit");
        db.set_compact_mode(true)
            .expect("compact mode should enable");

        let mirror = HistoryMirror::start_database(
            HistoryMirrorConfig::new(output.clone(), 4_096).with_debounce(Duration::ZERO),
            database_path,
        )
        .expect("database mirror should start");
        mirror
            .schedule_refresh()
            .expect("compact refresh should schedule");
        mirror
            .flush(Duration::from_secs(2))
            .expect("compact refresh should flush");

        let values = jsonl_values(&output);
        assert_eq!(values.len(), 1);
        assert_eq!(values[0]["data_type"], "text");
        assert_eq!(values[0]["display"]["value"], "compact mirror");
        assert_eq!(values[0]["source_bundle_id"], "com.example.compact");
        assert_eq!(values[0]["is_remote_clipboard"], true);
        let data_types = values[0]["event_data"]["items"][0]["data_list"]
            .as_array()
            .expect("data list should be present")
            .iter()
            .map(|data| data["type"].as_str().expect("type should be text"))
            .collect::<Vec<_>>();
        assert_eq!(
            data_types,
            vec![
                "public.utf8-plain-text",
                crate::pasteboard_protocol::SOURCE_TYPE,
                crate::pasteboard_protocol::REMOTE_CLIPBOARD_TYPE,
            ]
        );
        mirror
            .shutdown(Duration::from_secs(2))
            .expect("database mirror should stop");
    }

    #[test]
    fn injected_precommit_failures_preserve_final_and_cleanup_temp() {
        for failure_stage in [
            WriteStage::BeforeTempCreate,
            WriteStage::AfterTempCreate,
            WriteStage::AfterSerialize,
            WriteStage::AfterFlush,
            WriteStage::AfterSync,
            WriteStage::BeforeCommit,
        ] {
            let root = TestDirectory::new("failure");
            let output = root.path.join("history.jsonl");
            std::fs::write(&output, b"{\"old\":true}\n").expect("old snapshot should be created");
            std::fs::set_permissions(&output, Permissions::from_mode(0o600))
                .expect("old snapshot should be private");

            let should_fail = Arc::new(AtomicBool::new(true));
            let hook_should_fail = Arc::clone(&should_fail);
            let mirror = start_with(config(output.clone(), Duration::ZERO), move |stage, _| {
                if stage == failure_stage && hook_should_fail.load(Ordering::Acquire) {
                    return Err(HistoryMirrorError::new(
                        HistoryMirrorErrorKind::InjectedFailure,
                        "deterministic write failure",
                    ));
                }
                Ok(())
            })
            .expect("mirror should start");

            mirror
                .schedule(vec![text_row("failed", b"failed", b"failed", 1)])
                .expect("failing snapshot should schedule");
            let error = mirror
                .flush(Duration::from_secs(2))
                .expect_err("injected write should fail");
            assert_eq!(error.kind(), HistoryMirrorErrorKind::InjectedFailure);
            assert_eq!(
                std::fs::read(&output).expect("old snapshot should remain"),
                b"{\"old\":true}\n"
            );
            assert_no_temporary_files(&root.path);

            should_fail.store(false, Ordering::Release);
            mirror
                .schedule(vec![text_row("recovered", b"ok", b"ok", 2)])
                .expect("recovery snapshot should schedule");
            mirror
                .flush(Duration::from_secs(2))
                .expect("worker should recover after a failure");
            assert_eq!(jsonl_values(&output)[0]["content_hash"], "recovered");
            mirror
                .shutdown(Duration::from_secs(2))
                .expect("mirror should stop");
        }
    }

    #[test]
    fn postcommit_failure_leaves_a_complete_new_snapshot() {
        let root = TestDirectory::new("postcommit");
        let output = root.path.join("history.jsonl");
        std::fs::write(&output, b"{\"old\":true}\n").expect("old snapshot should exist");
        std::fs::set_permissions(&output, Permissions::from_mode(0o600))
            .expect("old snapshot should be private");

        let mirror = start_with(config(output.clone(), Duration::ZERO), |stage, _| {
            if stage == WriteStage::AfterCommit {
                Err(HistoryMirrorError::new(
                    HistoryMirrorErrorKind::InjectedFailure,
                    "postcommit failure",
                ))
            } else {
                Ok(())
            }
        })
        .expect("mirror should start");
        mirror
            .schedule(vec![text_row("new", b"new", b"new", 1)])
            .expect("snapshot should schedule");
        mirror
            .flush(Duration::from_secs(2))
            .expect_err("postcommit hook should report failure");

        assert_eq!(jsonl_values(&output)[0]["content_hash"], "new");
        assert_no_temporary_files(&root.path);
        mirror
            .shutdown(Duration::from_secs(2))
            .expect_err("shutdown should preserve latest failure status");
    }

    #[test]
    fn decode_failure_preserves_the_previous_complete_snapshot() {
        let root = TestDirectory::new("decode");
        let output = root.path.join("history.jsonl");
        std::fs::write(&output, b"{\"old\":true}\n").expect("old snapshot should exist");
        std::fs::set_permissions(&output, Permissions::from_mode(0o600))
            .expect("old snapshot should be private");
        let mirror = HistoryMirror::start(config(output.clone(), Duration::ZERO))
            .expect("mirror should start");
        mirror
            .schedule(vec![HistorySnapshotRow::new(
                "bad".to_string(),
                vec![0, 1, 2],
                "text".to_string(),
                b"bad".to_vec(),
                1,
            )])
            .expect("invalid row should schedule");

        let error = mirror
            .flush(Duration::from_secs(2))
            .expect_err("invalid event should fail in worker");
        assert_eq!(error.kind(), HistoryMirrorErrorKind::EventDecode);
        assert_eq!(
            std::fs::read(&output).expect("old snapshot should remain"),
            b"{\"old\":true}\n"
        );
        assert_no_temporary_files(&root.path);
        mirror
            .shutdown(Duration::from_secs(2))
            .expect_err("shutdown should report the decode failure");
    }

    #[test]
    fn flush_and_shutdown_time_out_without_panicking_or_blocking_forever() {
        let root = TestDirectory::new("bounded");
        let output = root.path.join("history.jsonl");
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let entered_tx = Mutex::new(Some(entered_tx));
        let release_rx = Mutex::new(release_rx);
        let mirror = start_with(config(output.clone(), Duration::ZERO), move |stage, _| {
            if stage == WriteStage::BeforeCommit {
                if let Some(sender) = lock_recover(&entered_tx).take() {
                    sender.send(()).map_err(|_| {
                        HistoryMirrorError::new(
                            HistoryMirrorErrorKind::InjectedFailure,
                            "signal blocked commit",
                        )
                    })?;
                }
                lock_recover(&release_rx).recv().map_err(|_| {
                    HistoryMirrorError::new(
                        HistoryMirrorErrorKind::InjectedFailure,
                        "release blocked commit",
                    )
                })?;
            }
            Ok(())
        })
        .expect("mirror should start");

        mirror
            .schedule(vec![text_row("latest", b"latest", b"latest", 1)])
            .expect("snapshot should schedule");
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("worker should reach commit barrier");

        assert_eq!(
            mirror
                .flush(Duration::from_millis(20))
                .expect_err("flush should time out")
                .kind(),
            HistoryMirrorErrorKind::TimedOut
        );
        assert_eq!(
            mirror
                .shutdown(Duration::from_millis(20))
                .expect_err("shutdown should time out")
                .kind(),
            HistoryMirrorErrorKind::TimedOut
        );

        release_tx.send(()).expect("worker should be released");
        mirror
            .shutdown(Duration::from_secs(2))
            .expect("second shutdown should finish");
        assert_eq!(jsonl_values(&output)[0]["content_hash"], "latest");
    }

    #[test]
    fn start_rejects_symlink_output_without_exposing_or_modifying_target() {
        let root = TestDirectory::new("path");
        let victim = root.path.join("victim");
        std::fs::write(&victim, b"victim").expect("victim should exist");
        std::fs::set_permissions(&victim, Permissions::from_mode(0o600))
            .expect("victim should be private");
        let output = root.path.join("secret-clipboard-export.jsonl");
        symlink(&victim, &output).expect("output symlink should exist");

        let error = HistoryMirror::start(config(output, Duration::ZERO))
            .expect_err("symlink output should be rejected");
        assert_eq!(
            error.kind(),
            HistoryMirrorErrorKind::Path(PrivateFsErrorKind::Symlink)
        );
        assert!(!error.to_string().contains("secret-clipboard-export"));
        assert_eq!(
            std::fs::read(&victim).expect("victim should remain readable"),
            b"victim"
        );
    }
}
