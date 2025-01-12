pub mod arena;
pub mod stdout;

use std::{ io, fmt };
use std::borrow::Cow;
use serde::Deserialize;

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

pub struct RefWriter<'a>(pub &'a mut dyn io::Write);

impl RefWriter<'_> {
    pub fn reborrow(&mut self) -> RefWriter<'_> {
        RefWriter(self.0)
    }
}

impl io::Write for RefWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[derive(Deserialize)]
pub struct CowStr<'s>(
    #[serde(borrow)]
    #[serde(deserialize_with = "deserialize_cow")]
    pub Cow<'s, str>
);

fn deserialize_cow<'de, D>(deserializer: D) -> Result<Cow<'de, str>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    use std::fmt;
    use serde::de;
    
    struct Visitor;

    impl<'de> de::Visitor<'de> for Visitor {
        type Value = Cow<'de, str>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a string")
        }

        fn visit_borrowed_str<E: de::Error>(self, val: &'de str) -> Result<Self::Value, E> {
            Ok(Cow::Borrowed(val))
        }

        fn visit_str<E: de::Error>(self, val: &str) -> Result<Self::Value, E> {
            self.visit_string(val.into())
        }

        fn visit_string<E: de::Error>(self, val: String) -> Result<Self::Value, E> {
            Ok(Cow::Owned(val))
        }
    }

    deserializer.deserialize_str(Visitor)
}

#[derive(Clone, Copy)]
pub struct FmtDebug<T>(pub T);

impl<T: std::fmt::Debug> fmt::Display for FmtDebug<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}
