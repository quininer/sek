use std::{ cmp, fmt };
use std::ffi::OsStr;
use bstr::ByteSlice;
use icu_collator::Collator;


pub fn file_name_cmp(collator: &Collator, x: &OsStr, y: &OsStr) -> cmp::Ordering {
    collator.as_borrowed().compare_utf8(x.as_encoded_bytes(), y.as_encoded_bytes())
}

pub fn contains(path: &OsStr, needle: &str, case_sensitive: bool)
    -> bool
{
    if needle.is_empty() {
        false
    } else if !case_sensitive || !needle.is_ascii() {
        path.as_encoded_bytes().find(needle.as_bytes()).is_some()
    } else {
        let first = needle.as_bytes()[0];
        let lower = (first as char).to_ascii_lowercase() as u8;
        let upper = (first as char).to_ascii_uppercase() as u8;
        let haystack = path.as_encoded_bytes();

        for pos in memchr::memchr2_iter(lower, upper, haystack) {
            let end = pos + needle.len();

            if let Some(substr) = haystack.get(pos..end)
                && substr.eq_ignore_ascii_case(needle.as_bytes())
            {
                return true;
            }
        }

        false        
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
