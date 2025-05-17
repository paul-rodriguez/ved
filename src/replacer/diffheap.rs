use super::diff::Diff;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Min-heap over Diffs
///
/// Normal BinaryHeaps are max-heaps.
/// This implementation works by using cmp::Reverse which reverses the normal
/// Ord implementation over the Diff object inside.
pub struct DiffHeap<'str> {
    heap: BinaryHeap<Reverse<Diff<'str>>>,
}

impl<'str> DiffHeap<'str> {
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }

    pub fn push(&mut self, diff: Diff<'str>) {
        self.heap.push(Reverse(diff))
    }

    pub fn merge_with(&mut self, mut other: Self) {
        self.heap.append(&mut other.heap)
    }

    pub fn pop(&mut self) -> Option<Diff<'str>> {
        match self.heap.pop() {
            None => None,
            Some(reversed_diff) => Some(reversed_diff.0),
        }
    }
}
