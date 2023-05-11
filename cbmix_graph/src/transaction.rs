use std::collections::{hash_map::Entry, HashMap};
use std::hash::Hash;
use std::iter::Extend;

#[derive(Debug)]
pub struct Transaction<'a, K, V> {
    overlay: HashMap<K, V>,
    base: &'a mut HashMap<K, V>,
}

impl<'a, K, V> Transaction<'a, K, V> {
    pub fn new(base: &'a mut HashMap<K, V>) -> Self {
        Self {
            overlay: HashMap::new(),
            base,
        }
    }
}

impl<'a, K, V> Transaction<'a, K, V>
where
    K: Eq + Hash + Clone,
{
    pub fn get(&self, key: &K) -> Option<&V> {
        self.overlay.get(key).or_else(|| self.base.get(key))
    }

    pub fn get_mut<'b>(&'b mut self, key: &K) -> Option<&'b mut V>
    where
        V: Clone,
    {
        match self.overlay.entry(key.to_owned()) {
            Entry::Occupied(occupied) => Some(occupied.into_mut()),
            Entry::Vacant(vacant) => match self.base.get(key) {
                Some(val) => Some(vacant.insert(val.clone())),
                None => None,
            },
        }
    }

    pub fn insert(&mut self, key: K, val: V) -> Option<V> {
        self.overlay.insert(key, val)
    }

    pub fn commit(self) {
        self.base.extend(self.overlay.into_iter())
    }
}

pub trait MapLike<K, V> {
    fn get(&self, key: &K) -> Option<&V>;
    fn get_mut(&mut self, key: &K) -> Option<&mut V>;
}

impl<'a, K, V> MapLike<K, V> for Transaction<'a, K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    fn get(&self, key: &K) -> Option<&V> {
        Transaction::<'a, K, V>::get(self, key)
    }
    fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        Transaction::<'a, K, V>::get_mut(self, key)
    }
}
impl<K, V> MapLike<K, V> for HashMap<K, V>
where
    K: Eq + Hash,
{
    fn get(&self, key: &K) -> Option<&V> {
        HashMap::<K, V>::get(self, key)
    }
    fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        HashMap::<K, V>::get_mut(self, key)
    }
}
