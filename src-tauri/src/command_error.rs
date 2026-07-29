use chrono::Utc;
use serde::Serialize;
use std::collections::VecDeque;
use std::fmt;
use std::sync::Mutex;

const DIAGNOSTIC_CAPACITY: usize = 32;

pub type CommandResult<T> = Result<T, CommandError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    StartupFailed,
    DatabaseUnavailable,
    DatabaseOperationFailed,
    HistoryItemNotFound,
    ClipboardWriteFailed,
    RestorePostProcessingFailed,
    InvalidSetting,
    InvalidHistoryCursor,
    StateUnavailable,
    AutostartUnavailable,
    AutostartVerificationFailed,
    HistoryMirrorFailed,
    CaptureRejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Startup,
    CaptureClipboard,
    LoadHistory,
    LoadHistoryDetail,
    RestoreClipboard,
    DeleteHistory,
    ClearHistory,
    LoadSettings,
    UpdateSettings,
    UpdateAutostart,
    WriteHistoryMirror,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CommandError {
    pub code: ErrorCode,
    pub operation: Operation,
    pub retryable: bool,
}

impl CommandError {
    pub const fn new(code: ErrorCode, operation: Operation, retryable: bool) -> Self {
        Self {
            code,
            operation,
            retryable,
        }
    }

    pub const fn database(operation: Operation) -> Self {
        Self::new(ErrorCode::DatabaseOperationFailed, operation, true)
    }

    pub const fn state(operation: Operation) -> Self {
        Self::new(ErrorCode::StateUnavailable, operation, true)
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} during {:?} (retryable={})",
            self.code, self.operation, self.retryable
        )
    }
}

impl std::error::Error for CommandError {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SafeDiagnostic {
    pub timestamp: i64,
    pub version: &'static str,
    pub platform: &'static str,
    pub architecture: &'static str,
    pub code: ErrorCode,
    pub operation: Operation,
    pub retryable: bool,
}

impl From<&CommandError> for SafeDiagnostic {
    fn from(error: &CommandError) -> Self {
        Self {
            timestamp: Utc::now().timestamp_millis(),
            version: env!("CARGO_PKG_VERSION"),
            platform: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
            code: error.code,
            operation: error.operation,
            retryable: error.retryable,
        }
    }
}

#[derive(Default)]
pub struct DiagnosticLog {
    entries: Mutex<VecDeque<SafeDiagnostic>>,
}

impl DiagnosticLog {
    pub fn record(&self, error: &CommandError) -> Result<(), CommandError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| CommandError::state(error.operation))?;
        if entries.len() == DIAGNOSTIC_CAPACITY {
            entries.pop_front();
        }
        entries.push_back(SafeDiagnostic::from(error));
        Ok(())
    }

    pub fn snapshot(&self) -> Result<Vec<SafeDiagnostic>, CommandError> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| CommandError::state(Operation::LoadSettings))?;
        Ok(entries.iter().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_are_bounded_and_contain_only_enumerated_safe_fields() {
        let log = DiagnosticLog::default();
        let error = CommandError::database(Operation::LoadHistoryDetail);
        for _ in 0..(DIAGNOSTIC_CAPACITY + 5) {
            log.record(&error)
                .expect("diagnostic record should succeed");
        }

        let snapshot = log.snapshot().expect("diagnostics should be readable");
        assert_eq!(snapshot.len(), DIAGNOSTIC_CAPACITY);
        let serialized = serde_json::to_string(&snapshot).expect("diagnostics should serialize");

        for forbidden in [
            "content_hash",
            "source_bundle",
            "file://",
            "<html",
            "event_data",
            "clipboard_body",
        ] {
            assert!(!serialized.contains(forbidden));
        }
        assert!(serialized.contains("database_operation_failed"));
        assert!(serialized.contains("load_history_detail"));
    }
}
