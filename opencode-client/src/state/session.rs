use crate::types::session::SessionStatus;

/// Session FSM: idle -> busy -> (retry | idle).
#[derive(Debug, Default)]
pub struct SessionFsm {
    pub status: SessionStatus,
}

impl SessionFsm {
    pub fn apply(&mut self, status: SessionStatus) {
        self.status = status;
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.status, SessionStatus::Busy | SessionStatus::Retry { .. })
    }

    /// A pending server-reported retry (busy with a banner and a deadline).
    pub fn is_retry(&self) -> bool {
        matches!(self.status, SessionStatus::Retry { .. })
    }
}