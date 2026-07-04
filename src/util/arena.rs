use std::fmt;
use std::num::NonZero;
use std::convert::TryInto;
use std::marker::PhantomData;
use std::hash::{ Hash, Hasher };
use std::ops::{ Index, IndexMut };


#[derive(Debug)]
pub struct Arena<T>(Vec<T>);

pub struct ArenaMap<T, V> {
    map: Vec<Option<V>>,
    _phantom: PhantomData<Id<T>>
}

pub struct Id<T>(NonZero<u32>, PhantomData<fn() -> T>);

impl<T> Id<T> {
    fn new(n: usize) -> Id<T> {
        let id: u32 = n.try_into().unwrap();
        let id = NonZero::new(id + 1).unwrap();
        Id(id, PhantomData)
    }

    fn get(&self) -> usize {
        (self.0.get() - 1).try_into().unwrap()
    }
}

impl<T> Arena<T> {
    pub const fn new() -> Arena<T> {
        Arena(Vec::new())
    }

    pub fn alloc(&mut self, val: T) -> Id<T> {
        let id = self.0.len();
        self.0.push(val);
        Id::new(id)
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
        Id(<NonZero<u32>>::MAX, PhantomData)
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
        &self.0[index.get()]
    }
}

impl<T> IndexMut<Id<T>> for Arena<T> {
    fn index_mut(&mut self, index: Id<T>) -> &mut Self::Output {
        &mut self.0[index.get()]
    }
}

impl<T> Extend<T> for Arena<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.0.extend(iter)
    }
}

impl<T> Id<T> {
    pub fn raw(&self) -> u32 {
        self.0.get()
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
        let id = Id::new(self.index);
        self.index += 1;
        Some(id)
    }
}

impl<T> ExactSizeIterator for Iter<'_, T> {
    fn len(&self) -> usize {
        self.arena.0.len()
    }
}

impl<T> Iter<'_, T> {
    pub fn bump(&mut self) {
        self.next();
    }

    pub fn peek(&self) -> Option<Id<T>> {
        let next = self.index;
        self.arena.0.get(next)?;
        Some(Id::new(next))
    }

    pub fn prev(&self) -> Option<Id<T>> {
        let prev = self.index.checked_sub(1)?;
        self.arena.0.get(prev)?;
        Some(Id::new(prev))
    }
}

impl<T, V> ArenaMap<T, V> {
    pub fn insert(&mut self, key: Id<T>, value: V) -> Option<V> {
        let id: usize = key.get();
        let min_len = id + 1;

        if self.map.len() < min_len {
            self.map.resize_with(min_len, || None);
        }

        self.map[id].replace(value)
    }

    pub fn get(&self, id: Id<T>) -> Option<&V> {
        self.map.get(id.get())?.as_ref()
    }

    pub fn remove(&mut self, id: Id<T>) -> Option<V> {
        self.map.get_mut(id.get())?.take()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl<T, V> Default for ArenaMap<T, V> {
    fn default() -> Self {
        ArenaMap { map: Vec::new(), _phantom: PhantomData }
    }
}
