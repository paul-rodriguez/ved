use super::diff::Diff;
use std::collections::BinaryHeap;

pub struct DiffHeap<'str> {
    heap: BinaryHeap<Diff<'str>>,
}

impl<'str> DiffHeap<'str> {
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }

    pub fn push(&mut self, diff: Diff<'str>) {
        self.heap.push(diff)
    }

    pub fn merge_with(&mut self, mut other: Self) {
        self.heap.append(&mut other.heap)
    }

    pub fn pop(&mut self) -> Option<Diff<'str>> {
        self.heap.pop()
    }
}
