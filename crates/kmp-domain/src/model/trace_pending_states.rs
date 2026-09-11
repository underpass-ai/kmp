use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, VecDeque};

/// Three preference pops, then one FIFO pop. States are never pruned by score.
pub(super) struct TracePendingStates {
    focused: bool,
    fifo: VecDeque<(usize, u64)>,
    priority: BinaryHeap<(bool, Reverse<u32>, Reverse<u64>, usize)>,
    pending: BTreeMap<usize, u64>,
    ticket: u64,
    pub priority_pops: u32,
    pub exploration_pops: u32,
}

impl TracePendingStates {
    pub fn new(focused: bool) -> Self {
        Self {
            focused,
            fifo: VecDeque::new(),
            priority: BinaryHeap::new(),
            pending: BTreeMap::new(),
            ticket: 0,
            priority_pops: 0,
            exploration_pops: 0,
        }
    }

    pub fn push(&mut self, index: usize, depth: u32, preferred: bool) {
        let ticket = self.ticket;
        self.ticket += 1;
        self.fifo.push_back((index, ticket));
        if self.focused {
            self.priority
                .push((preferred, Reverse(depth), Reverse(ticket), index));
            self.pending.insert(index, ticket);
        }
    }

    pub fn pop(&mut self) -> Option<usize> {
        if !self.focused {
            return self.fifo.pop_front().map(|(index, _)| index);
        }
        let explore = (self.priority_pops + self.exploration_pops) % 4 == 3;
        loop {
            let (index, ticket) = if explore {
                self.fifo.pop_front()?
            } else {
                let (_, _, ticket, index) = self.priority.pop()?;
                (index, ticket.0)
            };
            if self.pending.get(&index) == Some(&ticket) {
                self.pending.remove(&index);
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
