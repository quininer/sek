pub mod arena;
pub mod stdout;

macro_rules! matches2 {
    ( $expr:expr, $item:path ) => {
        match $expr {
            $item ( val ) => Some(val),
            _ => None
        }
    };
}

pub struct ScopeGuard<T, F: Fn(&mut T)>(pub T, pub F);

impl<T, F: Fn(&mut T)> ScopeGuard<T, F> {
    pub fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
    
    pub fn forget(self) {
        std::mem::forget(self);
    }
}

impl<T, F: Fn(&mut T)> Drop for ScopeGuard<T, F> {
    fn drop(&mut self) {
        (self.1)(&mut self.0);
    }
}
