use std::ops::{ Index, IndexMut };
use std::convert::TryInto;
use std::marker::PhantomData;


#[derive(Default)]
pub struct Arena<T>(Vec<T>);

#[derive(Debug)]
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

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

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
    pub fn back(&mut self) {
        self.index = self.index.saturating_sub(1);
    }

    pub fn peek(&self) -> Option<Id<T>> {
        let next = self.index + 1;
        self.arena.0.get(next)?;
        Some(Id(next.try_into().unwrap(), PhantomData))
    }

    pub fn current(&self) -> Option<Id<T>> {
        let next = self.index;
        self.arena.0.get(next)?;
        Some(Id(next.try_into().unwrap(), PhantomData))       
    }
}
