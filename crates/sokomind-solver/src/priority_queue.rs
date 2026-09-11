use std::cmp::Ordering;
use std::collections::BinaryHeap;

struct MinItem<K: Ord, V> {
    key: K,
    value: V,
}

impl<K: Ord, V> PartialEq for MinItem<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<K: Ord, V> Eq for MinItem<K, V> {}

impl<K: Ord, V> PartialOrd for MinItem<K, V> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: Ord, V> Ord for MinItem<K, V> {
    fn cmp(&self, other: &Self) -> Ordering {
        other.key.cmp(&self.key)
    }
}

/// Min-heap priority queue.
pub struct MinPriorityQueue<K: Ord, V> {
    heap: BinaryHeap<MinItem<K, V>>,
}

impl<K: Ord, V> MinPriorityQueue<K, V> {
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, key: K, value: V) {
        self.heap.push(MinItem { key, value });
    }

    pub fn pop(&mut self) -> Option<(K, V)> {
        self.heap.pop().map(|item| (item.key, item.value))
    }

    pub fn peek(&self) -> Option<&K> {
        self.heap.peek().map(|item| &item.key)
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
}

impl<K: Ord, V> Default for MinPriorityQueue<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_order() {
        let mut pq = MinPriorityQueue::new();
        pq.push(5, "five");
        pq.push(1, "one");
        pq.push(3, "three");
        assert_eq!(pq.pop(), Some((1, "one")));
        assert_eq!(pq.pop(), Some((3, "three")));
        assert_eq!(pq.pop(), Some((5, "five")));
        assert_eq!(pq.pop(), None);
    }

    #[test]
    fn peek_returns_min() {
        let mut pq = MinPriorityQueue::new();
        pq.push(10, ());
        pq.push(2, ());
        assert_eq!(pq.peek(), Some(&2));
    }

    #[test]
    fn len_and_empty() {
        let mut pq: MinPriorityQueue<i32, ()> = MinPriorityQueue::new();
        assert!(pq.is_empty());
        assert_eq!(pq.len(), 0);
        pq.push(1, ());
        assert!(!pq.is_empty());
        assert_eq!(pq.len(), 1);
    }

    #[test]
    fn duplicate_keys() {
        let mut pq = MinPriorityQueue::new();
        pq.push(3, "a");
        pq.push(3, "b");
        pq.push(1, "c");
        assert_eq!(pq.pop().unwrap().0, 1);
        let next = pq.pop().unwrap();
        assert_eq!(next.0, 3);
    }
}
