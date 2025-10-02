use std::{ cmp, fmt };
use std::ffi::OsStr;
use icu_collator::Collator;


pub fn file_name_cmp(collator: &Collator, x: &OsStr, y: &OsStr) -> cmp::Ordering {
    if let (Some(x), Some(y)) = (x.to_str(), y.to_str()) {
        collator.as_borrowed().compare(x, y)
    } else {
        Ord::cmp(x, y)
    }
}

pub struct EscapePath<'a>(pub &'a str);

impl fmt::Display for EscapePath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::fmt::Write;
        use bstr::ByteSlice;

        let s = self.0.as_bytes();

        if s.find_byteset(r#"#$&()|\;"'<> "#).is_none() {
            f.write_str(self.0)?;
        } else if s.find_byte(b'\'').is_none() {
            f.write_char('\'')?;
            f.write_str(self.0)?;
            f.write_char('\'')?;
        } else {
            f.write_char('"')?;
            for c in s.chars() {
                match c {
                    '"' => f.write_str(r#"\""#)?,
                    '$' => f.write_str(r#"\$"#)?,
                    '\\' => f.write_str(r#"\\"#)?,
                    c => f.write_char(c)?
                }
            }
            f.write_char('"')?;
        }

        Ok(())
    }
}
