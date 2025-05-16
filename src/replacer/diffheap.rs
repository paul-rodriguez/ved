use super::diff::Diff;
use std::collections::BinaryHeap;

pub struct DiffHeap<'str> {
    heap: BinaryHeap<Diff<'str>>,
}

impl<'str> DiffHeap<'str> {
    pub fn new(diff: Diff<'str>) -> Self {
        Self {
            heap: BinaryHeap::from(vec![diff]),
        }
    }

    pub fn push(&mut self, diff: Diff<'str>) {
        self.heap.push(diff)
    }
}
