use crate::types::v2::form::FormInfo;

/// Queue of forms from the model awaiting a user answer (opencode v2).
///
/// The queue is the single source of truth for the UI: it is filled by the
/// `form.created` event and reconciled with the server's form list (the poll),
/// so a missed SSE frame cannot leave a form invisible — and a form the server
/// has already settled cannot stay on screen.
#[derive(Debug, Default)]
pub struct FormQueue {
    pub pending: Vec<FormInfo>,
}

impl FormQueue {
    /// Add a form. A duplicate id is ignored — the SSE event and the poll may
    /// both deliver the same form.
    pub fn push(&mut self, f: FormInfo) {
        if !self.pending.iter().any(|x| x.id == f.id) {
            self.pending.push(f);
        }
    }

    /// Remove a settled form (answered or cancelled).
    pub fn pop(&mut self, id: &str) -> Option<FormInfo> {
        let idx = self.pending.iter().position(|x| x.id == id)?;
        Some(self.pending.remove(idx))
    }

    /// Is a form with this id still pending?
    pub fn contains(&self, id: &str) -> bool {
        self.pending.iter().any(|x| x.id == id)
    }

    /// The form to show first (the queue is ordered by the UI).
    pub fn first(&self) -> Option<&FormInfo> {
        self.pending.first()
    }

    /// Replace the whole queue with the server's list (the poll). The server is
    /// the authority on what is still pending, so forms it does not know about
    /// are dropped.
    pub fn replace(&mut self, forms: Vec<FormInfo>) {
        self.pending = forms;
    }

    /// Move the given session's form to the front of the queue — the UI shows
    /// the current session's form first; forms of other sessions wait their
    /// turn.
    pub fn promote_session(&mut self, session_id: &str) {
        if let Some(idx) = self.pending.iter().position(|f| f.session_id == session_id) {
            if idx != 0 {
                let f = self.pending.remove(idx);
                self.pending.insert(0, f);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn form(id: &str, session: &str) -> FormInfo {
        serde_json::from_value(json!({
            "id": id, "sessionID": session, "title": id,
            "fields": [{"type": "string", "key": "a"}]
        }))
        .expect("form deserialises")
    }

    #[test]
    fn duplicate_forms_are_not_stored_twice() {
        let mut q = FormQueue::default();
        q.push(form("frm_1", "ses_1"));
        q.push(form("frm_1", "ses_1"));
        assert_eq!(q.pending.len(), 1);
        assert!(q.contains("frm_1"));
    }

    #[test]
    fn pop_removes_the_form() {
        let mut q = FormQueue::default();
        q.push(form("frm_1", "ses_1"));
        assert!(q.pop("frm_1").is_some());
        assert!(q.pop("frm_1").is_none());
        assert!(q.first().is_none());
    }

    #[test]
    fn replace_drops_forms_the_server_does_not_know() {
        let mut q = FormQueue::default();
        q.push(form("frm_1", "ses_1"));
        q.replace(vec![form("frm_2", "ses_1")]);
        assert_eq!(q.pending.len(), 1);
        assert_eq!(q.pending[0].id, "frm_2");
    }

    #[test]
    fn promote_session_moves_the_current_session_form_first() {
        let mut q = FormQueue::default();
        q.push(form("frm_1", "ses_other"));
        q.push(form("frm_2", "ses_cur"));
        q.promote_session("ses_cur");
        assert_eq!(q.pending[0].id, "frm_2");
        q.promote_session("ses_missing");
        assert_eq!(q.pending[0].id, "frm_2");
    }
}
