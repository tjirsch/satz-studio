//! The snackbar queue: at most three toasts on screen, the oldest leaving first.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub text: String,
}

/// How many toasts stack on screen before the oldest is dropped.
pub const VISIBLE: usize = 3;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Append a toast and return its id; beyond [`VISIBLE`] the oldest goes.
pub fn enqueue(queue: &mut VecDeque<Toast>, kind: ToastKind, text: String) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    queue.push_back(Toast { id, kind, text });
    while queue.len() > VISIBLE {
        queue.pop_front();
    }
    id
}

/// Remove one toast by id; an id that already left is not an error.
pub fn dismiss(queue: &mut VecDeque<Toast>, id: u64) {
    queue.retain(|t| t.id != id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_queue_keeps_the_newest_three() {
        let mut q = VecDeque::new();
        let first = enqueue(&mut q, ToastKind::Info, "one".into());
        for n in 2..=5 {
            enqueue(&mut q, ToastKind::Info, n.to_string());
        }
        assert_eq!(q.len(), VISIBLE);
        assert!(q.iter().all(|t| t.id != first));
        assert_eq!(q.back().unwrap().text, "5");
    }

    #[test]
    fn dismiss_removes_one_and_tolerates_a_gone_id() {
        let mut q = VecDeque::new();
        let a = enqueue(&mut q, ToastKind::Error, "a".into());
        let b = enqueue(&mut q, ToastKind::Info, "b".into());
        dismiss(&mut q, a);
        dismiss(&mut q, a);
        assert_eq!(q.iter().map(|t| t.id).collect::<Vec<_>>(), vec![b]);
    }

    #[test]
    fn ids_are_unique_across_queues() {
        let mut q1 = VecDeque::new();
        let mut q2 = VecDeque::new();
        let a = enqueue(&mut q1, ToastKind::Info, "a".into());
        let b = enqueue(&mut q2, ToastKind::Info, "b".into());
        assert_ne!(a, b);
    }
}
