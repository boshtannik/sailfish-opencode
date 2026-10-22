use std::collections::HashMap;

use crate::types::tool::ToolState;

/// A single tool of the current turn: name + state.
#[derive(Debug, Clone)]
pub struct ToolEntry {
    pub name: String,
    pub state: ToolState,
}

/// Tool state store: callID -> ToolEntry, plus the order the calls appeared in
/// (a HashMap alone loses chronology, and the UI strip must read left-to-right).
#[derive(Debug, Default)]
pub struct ToolStore {
    pub states: HashMap<String, ToolEntry>,
    pub order: Vec<String>,
}

impl ToolStore {
    pub fn upsert(&mut self, call_id: String, name: String, state: ToolState) {
        if !self.states.contains_key(&call_id) {
            self.order.push(call_id.clone());
        }
        self.states.insert(call_id, ToolEntry { name, state });
    }

    /// Insert a placeholder entry if the call is not known yet (no name/state
    /// change otherwise): keeps `order` in sync for events that carry no name.
    pub fn ensure(&mut self, call_id: &str, name: &str) {
        if !self.states.contains_key(call_id) {
            self.upsert(
                call_id.to_string(),
                name.to_string(),
                ToolState::Pending {
                    input: serde_json::Value::Null,
                },
            );
        }
    }

    /// Replace the state of an existing call, keeping its name/order.
    pub fn set_state(&mut self, call_id: &str, state: ToolState) {
        if let Some(e) = self.states.get_mut(call_id) {
            e.state = state;
        }
    }

    /// Name of an existing call (used to build a `Completed` title).
    pub fn name_of(&self, call_id: &str) -> Option<&str> {
        self.states.get(call_id).map(|e| e.name.as_str())
    }

    /// Input of an existing call, or `Null`.
    pub fn input_of(&self, call_id: &str) -> serde_json::Value {
        self.states
            .get(call_id)
            .map(|e| e.state.input().clone())
            .unwrap_or(serde_json::Value::Null)
    }

    /// Current state of an existing call (to render a feed row after a
    /// transition), or `None` if the call is unknown.
    pub fn state_of(&self, call_id: &str) -> Option<&ToolState> {
        self.states.get(call_id).map(|e| &e.state)
    }

    /// Drop everything (new user turn / session switch).
    pub fn clear(&mut self) {
        self.states.clear();
        self.order.clear();
    }

    /// Number of tools in each state — handy for indicators in QML.
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let mut pending = 0;
        let mut running = 0;
        let mut completed = 0;
        let mut failed = 0;
        for e in self.states.values() {
            match &e.state {
                ToolState::Pending { .. } => pending += 1,
                ToolState::Running { .. } => running += 1,
                ToolState::Completed { .. } => completed += 1,
                ToolState::Error { .. } => failed += 1,
            }
        }
        (pending, running, completed, failed)
    }
}
