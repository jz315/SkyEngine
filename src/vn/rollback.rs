use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::vn::runtime::{VnRuntime, VnRuntimeError, VnRuntimeSnapshot};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnRollbackStack {
    limit: usize,
    #[serde(default, skip_serializing_if = "VecDeque::is_empty")]
    snapshots: VecDeque<VnRollbackSnapshot>,
}

impl Default for VnRollbackStack {
    fn default() -> Self {
        Self::new(64)
    }
}

impl VnRollbackStack {
    pub fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            snapshots: VecDeque::new(),
        }
    }

    pub fn push_runtime(&mut self, reason: VnRollbackReason, runtime: &VnRuntime) {
        self.push(VnRollbackSnapshot {
            reason,
            runtime: runtime.snapshot(),
        });
    }

    pub fn push(&mut self, snapshot: VnRollbackSnapshot) {
        if self.snapshots.len() == self.limit {
            self.snapshots.pop_front();
        }
        self.snapshots.push_back(snapshot);
    }

    pub fn pop(&mut self) -> Option<VnRollbackSnapshot> {
        self.snapshots.pop_back()
    }

    pub fn rollback_runtime(
        &mut self,
        runtime: &mut VnRuntime,
    ) -> Result<Option<VnRollbackSnapshot>, VnRuntimeError> {
        let Some(snapshot) = self.pop() else {
            return Ok(None);
        };
        runtime.restore_snapshot(snapshot.runtime.clone())?;
        Ok(Some(snapshot))
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VnRollbackSnapshot {
    pub reason: VnRollbackReason,
    pub runtime: VnRuntimeSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VnRollbackReason {
    Line,
    Choice,
    Checkpoint(String),
    Manual(String),
}

#[cfg(test)]
mod tests {
    use crate::vn::{VnRuntime, VnRuntimeEvent, YarnScript};

    use super::*;

    #[test]
    fn rollback_restores_previous_line_state() {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
First. #line:start.1
Second. #line:start.2
===
"#,
        )
        .unwrap();
        let mut runtime = VnRuntime::from_script(script, "Start").unwrap();
        assert!(matches!(
            runtime.advance().unwrap(),
            VnRuntimeEvent::Line(_)
        ));

        let mut rollback = VnRollbackStack::new(4);
        rollback.push_runtime(VnRollbackReason::Line, &runtime);
        assert!(matches!(
            runtime.advance().unwrap(),
            VnRuntimeEvent::Line(_)
        ));
        rollback.rollback_runtime(&mut runtime).unwrap();

        assert_eq!(
            runtime.dialogue().current_line.as_ref().unwrap().text,
            "First."
        );
    }
}
