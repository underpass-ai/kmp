use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, VecDeque};

/// Three preference pops, then one FIFO pop. States are never pruned by score.
pub(super) struct TracePendingStates {
    focused: bool,
    fifo: VecDeque<usize>,
    priority: BinaryHeap<(bool, Reverse<u32>, Reverse<usize>)>,
    pending: BTreeSet<usize>,
    pub priority_pops: u32,
    pub exploration_pops: u32,
}

impl TracePendingStates {
    pub fn new(focused: bool) -> Self {
        Self {
            focused,
            fifo: VecDeque::new(),
            priority: BinaryHeap::new(),
            pending: BTreeSet::new(),
            priority_pops: 0,
            exploration_pops: 0,
        }
    }

    pub fn push(&mut self, index: usize, depth: u32, preferred: bool) {
        self.fifo.push_back(index);
        if self.focused {
            self.priority
                .push((preferred, Reverse(depth), Reverse(index)));
            self.pending.insert(index);
        }
    }

    pub fn pop(&mut self) -> Option<usize> {
        if !self.focused {
            return self.fifo.pop_front();
        }
        let explore = (self.priority_pops + self.exploration_pops) % 4 == 3;
        loop {
            let index = if explore {
                self.fifo.pop_front()?
            } else {
                self.priority.pop()?.2.0
            };
            if self.pending.remove(&index) {
                if explore {
                    self.exploration_pops += 1;
                } else {
                    self.priority_pops += 1;
                }
                return Some(index);
            }
        }
    }
}
