//! Small LRU map with a cost limit (entry count or estimated bytes).

use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

pub struct Lru<K, V> {
    map: HashMap<K, (V, u64, usize)>,
    order: BTreeMap<u64, K>,
    clock: u64,
    cost: usize,
    limit: usize,
    pub hits: u64,
    pub misses: u64,
}

impl<K: Hash + Eq + Clone, V: Clone> Lru<K, V> {
    /// `limit`: maximum total cost (sum of the costs passed to [`Lru::insert`]).
    pub fn new(limit: usize) -> Self {
        Lru { map: HashMap::new(), order: BTreeMap::new(), clock: 0, cost: 0, limit, hits: 0, misses: 0 }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn cost(&self) -> usize {
        self.cost
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        self.evict();
    }

    pub fn contains(&self, k: &K) -> bool {
        self.map.contains_key(k)
    }

    pub fn get(&mut self, k: &K) -> Option<V> {
        self.clock += 1;
        let now = self.clock;
        match self.map.get_mut(k) {
            Some((v, stamp, _)) => {
                self.order.remove(stamp);
                *stamp = now;
                self.order.insert(now, k.clone());
                self.hits += 1;
                Some(v.clone())
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    pub fn insert(&mut self, k: K, v: V, cost: usize) {
        self.clock += 1;
        let now = self.clock;
        if let Some((_, stamp, c)) = self.map.remove(&k) {
            self.order.remove(&stamp);
            self.cost -= c;
        }
        self.map.insert(k.clone(), (v, now, cost));
        self.order.insert(now, k);
        self.cost += cost;
        self.evict();
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
        self.cost = 0;
    }

    fn evict(&mut self) {
        // The newest entry is kept even if it alone exceeds the limit.
        while self.cost > self.limit && self.map.len() > 1 {
            let Some((&stamp, _)) = self.order.iter().next() else { break };
            let k = self.order.remove(&stamp).unwrap();
            if let Some((_, _, c)) = self.map.remove(&k) {
                self.cost -= c;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_least_recently_used() {
        let mut c = Lru::new(2);
        c.insert(1, "a", 1);
        c.insert(2, "b", 1);
        assert_eq!(c.get(&1), Some("a"));
        c.insert(3, "c", 1);
        assert!(c.contains(&1) && c.contains(&3) && !c.contains(&2));
        c.set_limit(1);
        assert_eq!(c.len(), 1);
        assert!(c.contains(&3));
    }
}
