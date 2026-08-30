use std::collections::HashMap;

/// Insertion-ordered map: the document shows things in the order memory
/// met them, and a rewrite lands where the original already was.
pub(super) struct Ordered<T> {
    pub(super) items: Vec<T>,
    pub(super) index: HashMap<String, usize>,
}

impl<T> Default for Ordered<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<T> Ordered<T> {
    pub(super) fn upsert(&mut self, id: &str, value: T) {
        match self.index.get(id) {
            Some(&at) => self.items[at] = value,
            None => {
                self.index.insert(id.to_string(), self.items.len());
                self.items.push(value);
            }
        }
    }
}
