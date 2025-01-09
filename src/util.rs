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

impl<T, F: Fn(&mut T)> AsRef<T> for ScopeGuard<T, F> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T, F: Fn(&mut T)> AsMut<T> for ScopeGuard<T, F> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T, F: Fn(&mut T)> Drop for ScopeGuard<T, F> {
    fn drop(&mut self) {
        (self.1)(&mut self.0);
    }
}
