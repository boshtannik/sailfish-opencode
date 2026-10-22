use crate::types::v2::permission::PermissionRequest;

/// Queue of permission requests waiting for a user response (opencode v2).
///
/// The queue is the single source of truth for the UI: it is filled by the
/// `permission.asked` event and reconciled with the server's permission list
/// (the poll), so a missed SSE frame cannot leave a request invisible — and a
/// request the server has already settled cannot stay on screen.
#[derive(Debug, Default)]
pub struct PermissionQueue {
    pub pending: Vec<PermissionRequest>,
}

impl PermissionQueue {
    /// Add a request. A duplicate id is ignored — the SSE event and the poll
    /// may both deliver the same request.
    pub fn push(&mut self, p: PermissionRequest) {
        if !self.pending.iter().any(|x| x.id == p.id) {
            self.pending.push(p);
        }
    }

    /// Remove a settled request (answered or withdrawn by the server).
    pub fn pop(&mut self, id: &str) -> Option<PermissionRequest> {
        let idx = self.pending.iter().position(|x| x.id == id)?;
        Some(self.pending.remove(idx))
    }

    /// Is a request with this id still pending?
    pub fn contains(&self, id: &str) -> bool {
        self.pending.iter().any(|x| x.id == id)
    }

    /// The request to show first (the queue is ordered by the UI).
    pub fn first(&self) -> Option<&PermissionRequest> {
        self.pending.first()
    }

    /// Replace the whole queue with the server's list (the poll). The server is
    /// the authority on what is still pending, so requests it does not know
    /// about are dropped (a request the server already settled must not stay
    /// on screen as a ghost card).
    pub fn replace(&mut self, requests: Vec<PermissionRequest>) {
        self.pending = requests;
    }

    /// Move the given session's request to the front of the queue — the UI
    /// shows the current session's request first; requests of other sessions
    /// wait their turn.
    pub fn promote_session(&mut self, session_id: &str) {
        if let Some(idx) = self.pending.iter().position(|p| p.session_id == session_id) {
            if idx != 0 {
                let p = self.pending.remove(idx);
                self.pending.insert(0, p);
            }
        }
    }

    /// Drop every request of a session (the model gave up on them — e.g. the
    /// server reported `permission.rejected`).
    pub fn clear_session(&mut self, session_id: &str) {
        self.pending.retain(|p| p.session_id != session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn req(id: &str, session: &str) -> PermissionRequest {
        serde_json::from_value(json!({
            "id": id, "sessionID": session, "action": "bash", "resources": ["ls"]
        }))
        .expect("permission request deserialises")
    }

    #[test]
    fn duplicate_requests_are_not_stored_twice() {
        let mut q = PermissionQueue::default();
        q.push(req("per_1", "ses_1"));
        q.push(req("per_1", "ses_1"));
        assert_eq!(q.pending.len(), 1);
        assert!(q.contains("per_1"));
    }

    #[test]
    fn pop_removes_the_request() {
        let mut q = PermissionQueue::default();
        q.push(req("per_1", "ses_1"));
        assert!(q.pop("per_1").is_some());
        assert!(q.pop("per_1").is_none());
        assert!(q.first().is_none());
    }

    #[test]
    fn replace_drops_requests_the_server_does_not_know() {
        let mut q = PermissionQueue::default();
        q.push(req("per_1", "ses_1"));
        q.replace(vec![req("per_2", "ses_1")]);
        assert_eq!(q.pending.len(), 1);
        assert_eq!(q.pending[0].id, "per_2");
    }

    #[test]
    fn promote_session_moves_the_current_session_request_first() {
        let mut q = PermissionQueue::default();
        q.push(req("per_1", "ses_other"));
        q.push(req("per_2", "ses_cur"));
        q.promote_session("ses_cur");
        assert_eq!(q.pending[0].id, "per_2");
        q.promote_session("ses_missing");
        assert_eq!(q.pending[0].id, "per_2");
    }

    #[test]
    fn clear_session_drops_only_that_session() {
        let mut q = PermissionQueue::default();
        q.push(req("per_1", "ses_cur"));
        q.push(req("per_2", "ses_other"));
        q.clear_session("ses_cur");
        assert_eq!(q.pending.len(), 1);
        assert_eq!(q.pending[0].id, "per_2");
    }
}
