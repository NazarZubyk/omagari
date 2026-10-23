//! Snapshot undo/redo with coalescing and a "saved" marker.
//!
//! The editor feeds the current state every frame ([`History::observe`]);
//! changes are committed as one undo step once the user's gesture is over
//! (pointer released, or typing paused), so a long drag or a typed word is one
//! step. Each committed state carries a revision number; the project is dirty
//! when the current state is not the revision last saved (undoing back to it
//! makes it clean again).

use std::collections::VecDeque;

/// Default number of undo steps kept.
pub const UNDO_LIMIT: usize = 200;
/// Seconds without a keystroke after which typed text becomes an undo step.
pub const TYPING_IDLE: f64 = 0.8;

/// What the user is doing this frame, for coalescing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Activity {
    Idle,
    /// A widget is being dragged (slider, drag value): wait for the release.
    Dragging,
    /// A text field has focus: commit after a pause.
    Typing,
}

#[derive(Clone, Debug)]
struct Entry<T> {
    state: T,
    rev: u64,
}

#[derive(Clone, Debug)]
struct Pending<T> {
    last_seen: T,
    changed_at: f64,
}

#[derive(Clone, Debug)]
pub struct History<T> {
    committed: Entry<T>,
    undo: VecDeque<Entry<T>>,
    redo: Vec<Entry<T>>,
    limit: usize,
    next_rev: u64,
    saved_rev: Option<u64>,
    pending: Option<Pending<T>>,
}

impl<T: Clone + PartialEq> History<T> {
    /// A history whose only state is `initial`, counted as saved.
    pub fn new(initial: T, limit: usize) -> Self {
        History {
            committed: Entry { state: initial, rev: 0 },
            undo: VecDeque::new(),
            redo: Vec::new(),
            limit: limit.max(1),
            next_rev: 1,
            saved_rev: Some(0),
            pending: None,
        }
    }

    /// Starts over from `state` (new / opened project): no undo, no redo.
    /// `saved` = the state is what is on disk.
    pub fn reset(&mut self, state: T, saved: bool) {
        let rev = self.bump();
        self.committed = Entry { state, rev };
        self.undo.clear();
        self.redo.clear();
        self.pending = None;
        self.saved_rev = saved.then_some(rev);
    }

    fn bump(&mut self) -> u64 {
        let rev = self.next_rev;
        self.next_rev += 1;
        rev
    }

    /// Feeds the current state; commits it as an undo step when it differs
    /// from the last committed one and the gesture is over. Returns `true`
    /// when a step was committed.
    pub fn observe(&mut self, current: &T, now: f64, activity: Activity) -> bool {
        if *current == self.committed.state {
            self.pending = None;
            return false;
        }
        let changed_at = match &self.pending {
            Some(p) if p.last_seen == *current => p.changed_at,
            _ => {
                self.pending = Some(Pending {
                    last_seen: current.clone(),
                    changed_at: now,
                });
                now
            }
        };
        let settled = match activity {
            Activity::Idle => true,
            Activity::Dragging => false,
            Activity::Typing => now - changed_at >= TYPING_IDLE,
        };
        settled && self.commit(current)
    }

    /// Commits `current` now if it differs from the last committed state.
    pub fn commit(&mut self, current: &T) -> bool {
        self.pending = None;
        if *current == self.committed.state {
            return false;
        }
        let rev = self.bump();
        let previous = std::mem::replace(&mut self.committed, Entry { state: current.clone(), rev });
        self.undo.push_back(previous);
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
        self.redo.clear();
        true
    }

    /// Goes one step back. An uncommitted change is committed first, so redo
    /// returns to it. Returns the state to restore.
    pub fn undo(&mut self, current: &T) -> Option<T> {
        self.commit(current);
        let previous = self.undo.pop_back()?;
        let now = std::mem::replace(&mut self.committed, previous);
        self.redo.push(now);
        Some(self.committed.state.clone())
    }

    /// Goes one step forward again.
    pub fn redo(&mut self, current: &T) -> Option<T> {
        if *current != self.committed.state {
            // A new edit since the undo: the redo branch is gone.
            self.commit(current);
            return None;
        }
        let next = self.redo.pop()?;
        let now = std::mem::replace(&mut self.committed, next);
        self.undo.push_back(now);
        Some(self.committed.state.clone())
    }

    /// Replaces the committed state with an equivalent one (e.g. the
    /// re-serialized text of a restored project), keeping its revision.
    pub fn normalize(&mut self, state: T) {
        self.committed.state = state;
        self.pending = None;
    }

    pub fn can_undo(&self, current: &T) -> bool {
        !self.undo.is_empty() || *current != self.committed.state
    }

    pub fn can_redo(&self, current: &T) -> bool {
        !self.redo.is_empty() && *current == self.committed.state
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Marks `current` as saved (commits it first).
    pub fn mark_saved(&mut self, current: &T) {
        self.commit(current);
        self.saved_rev = Some(self.committed.rev);
    }

    /// Unsaved changes: the current state is not the saved revision.
    pub fn is_dirty(&self, current: &T) -> bool {
        *current != self.committed.state || self.saved_rev != Some(self.committed.rev)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> History<i32> {
        History::new(0, 100)
    }

    #[test]
    fn idle_changes_commit_and_undo_redo_walk_them() {
        let mut h = history();
        assert!(!h.observe(&0, 0.0, Activity::Idle));
        assert!(h.observe(&1, 0.1, Activity::Idle));
        assert!(h.observe(&2, 0.2, Activity::Idle));
        assert_eq!(h.undo(&2), Some(1));
        assert_eq!(h.undo(&1), Some(0));
        assert_eq!(h.undo(&0), None);
        assert_eq!(h.redo(&0), Some(1));
        assert_eq!(h.redo(&1), Some(2));
        assert_eq!(h.redo(&2), None);
    }

    #[test]
    fn a_drag_is_one_step() {
        let mut h = history();
        for (i, v) in [1, 2, 3, 4].iter().enumerate() {
            assert!(!h.observe(v, i as f64, Activity::Dragging));
        }
        assert!(h.observe(&4, 5.0, Activity::Idle), "released");
        assert_eq!(h.undo_len(), 1);
        assert_eq!(h.undo(&4), Some(0));
    }

    #[test]
    fn typing_commits_after_a_pause() {
        let mut h = history();
        assert!(!h.observe(&1, 0.0, Activity::Typing));
        assert!(!h.observe(&12, 0.3, Activity::Typing));
        assert!(!h.observe(&123, 0.6, Activity::Typing));
        assert!(!h.observe(&123, 0.6 + TYPING_IDLE - 0.01, Activity::Typing));
        assert!(h.observe(&123, 0.6 + TYPING_IDLE + 0.01, Activity::Typing));
        assert_eq!(h.undo_len(), 1);
        assert_eq!(h.undo(&123), Some(0));
    }

    #[test]
    fn undo_during_a_gesture_commits_it_first() {
        let mut h = history();
        h.observe(&1, 0.0, Activity::Idle);
        h.observe(&5, 1.0, Activity::Dragging);
        assert!(h.can_undo(&5));
        assert_eq!(h.undo(&5), Some(1));
        assert_eq!(h.redo(&1), Some(5), "the uncommitted change is redoable");
    }

    #[test]
    fn a_new_edit_drops_the_redo_branch() {
        let mut h = history();
        h.observe(&1, 0.0, Activity::Idle);
        h.observe(&2, 0.1, Activity::Idle);
        assert_eq!(h.undo(&2), Some(1));
        assert!(h.can_redo(&1));
        h.observe(&7, 0.2, Activity::Idle);
        assert!(!h.can_redo(&7));
        assert_eq!(h.redo(&7), None);
        assert_eq!(h.undo(&7), Some(1));
    }

    #[test]
    fn the_limit_drops_the_oldest_steps() {
        let mut h = History::new(0, 100);
        for v in 1..=150 {
            h.observe(&v, v as f64, Activity::Idle);
        }
        assert_eq!(h.undo_len(), 100);
        let mut state = 150;
        while let Some(previous) = h.undo(&state) {
            state = previous;
        }
        assert_eq!(state, 50, "oldest kept state");
    }

    #[test]
    fn dirty_follows_the_saved_revision() {
        let mut h = history();
        assert!(!h.is_dirty(&0), "a new history is clean");
        h.observe(&1, 0.0, Activity::Dragging);
        assert!(h.is_dirty(&1), "uncommitted changes are dirty");
        h.observe(&1, 0.1, Activity::Idle);
        assert!(h.is_dirty(&1));
        h.mark_saved(&1);
        assert!(!h.is_dirty(&1));
        h.observe(&2, 0.2, Activity::Idle);
        assert!(h.is_dirty(&2));
        let back = h.undo(&2).unwrap();
        assert!(!h.is_dirty(&back), "undo back to the saved state is clean");
        let undone_past_save = h.undo(&back).unwrap();
        assert!(h.is_dirty(&undone_past_save));
    }

    #[test]
    fn reset_clears_the_stacks() {
        let mut h = history();
        h.observe(&1, 0.0, Activity::Idle);
        h.reset(10, false);
        assert!(!h.can_undo(&10));
        assert!(h.is_dirty(&10), "an unsaved new project (from a recipe)");
        h.reset(20, true);
        assert!(!h.is_dirty(&20));
    }

    #[test]
    fn normalize_keeps_the_revision() {
        let mut h = history();
        h.observe(&1, 0.0, Activity::Idle);
        h.mark_saved(&1);
        h.normalize(1);
        assert!(!h.is_dirty(&1));
    }
}
