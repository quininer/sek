pub mod arena;

macro_rules! matches2 {
    ( $expr:expr, $item:path ) => {
        match $expr {
            $item ( val ) => Some(val),
            _ => None
        }
    };
}
