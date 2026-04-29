use std::fmt;
use std::hash::{ Hash, Hasher };
use std::ops::{ Index, IndexMut };
use std::convert::TryInto;
use std::marker::PhantomData;


#[derive(Debug)]
pub struct Arena<T>(Vec<T>);

pub struct ArenaMap<T, V> {
    map: Vec<Option<V>>,
    _phantom: PhantomData<Id<T>>
}

pub struct Id<T>(u32, PhantomData<fn() -> T>);

impl<T> Arena<T> {
    pub const fn new() -> Arena<T> {
        Arena(Vec::new())
    }

    pub fn alloc(&mut self, val: T) -> Id<T> {
        let id = self.0.len();
        self.0.push(val);
        Id(id.try_into().unwrap(), PhantomData)
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter{ arena: self, index: 0 }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Id(u32::MAX, PhantomData)
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Arena::new()
    }
}

impl<T> Index<Id<T>> for Arena<T> {
    type Output = T;
    
    fn index(&self, index: Id<T>) -> &Self::Output {
        let idx: usize = index.0.try_into().unwrap();
        &self.0[idx]
    }
}

impl<T> IndexMut<Id<T>> for Arena<T> {
    fn index_mut(&mut self, index: Id<T>) -> &mut Self::Output {
        let idx: usize = index.0.try_into().unwrap();
        &mut self.0[idx]
    }
}

impl<T> Extend<T> for Arena<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.0.extend(iter)
    }
}

impl<T> Id<T> {
    pub fn raw(&self) -> u32 {
        self.0
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl<T> Eq for Id<T> {}

impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

pub struct Iter<'a, T> {
    arena: &'a Arena<T>,
    index: usize
}

impl<T> Iterator for Iter<'_, T> {
    type Item = Id<T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.arena.0.get(self.index)?;
        let id = Id(self.index.try_into().unwrap(), PhantomData);
        self.index += 1;
        Some(id)
    }
}

impl<T> Iter<'_, T> {
    pub fn bump(&mut self) {
        self.next();
    }

    pub fn peek(&self) -> Option<Id<T>> {
        let next = self.index;
        self.arena.0.get(next)?;
        Some(Id(next.try_into().unwrap(), PhantomData))
    }

    pub fn prev(&self) -> Option<Id<T>> {
        let prev = self.index.saturating_sub(1);
        self.arena.0.get(prev)?;
        Some(Id(prev.try_into().unwrap(), PhantomData))
    }
}

impl<T, V> ArenaMap<T, V> {
    pub fn insert(&mut self, key: Id<T>, value: V) -> Option<V> {
        let id: usize = key.0.try_into().unwrap();
        let min_len = id + 1;

        if self.map.len() < min_len {
            self.map.resize_with(min_len, || None);
        }

        self.map[id].replace(value)
    }

    pub fn get(&self, id: Id<T>) -> Option<&V> {
        let id: usize = id.0.try_into().unwrap();
        self.map.get(id)?.as_ref()
    }

    pub fn remove(&mut self, id: Id<T>) -> Option<V> {
        let id: usize = id.0.try_into().unwrap();
        self.map.get_mut(id)?.take()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl<K, V> Index<Id<K>> for ArenaMap<K, V> {
    type Output = V;
    
    fn index(&self, index: Id<K>) -> &Self::Output {
        let idx: usize = index.0.try_into().unwrap();
        self.map[idx].as_ref().unwrap()
    }
}

impl<K, V> IndexMut<Id<K>> for ArenaMap<K, V> {
    fn index_mut(&mut self, index: Id<K>) -> &mut Self::Output {
        let idx: usize = index.0.try_into().unwrap();
        self.map[idx].as_mut().unwrap()
    }
}

impl<T, V> Default for ArenaMap<T, V> {
    fn default() -> Self {
        ArenaMap { map: Vec::new(), _phantom: PhantomData }
    }
}
