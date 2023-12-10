use httparse::Header;

use crate::util::cast_buf_for_headers;
use crate::{HootError, Result};

pub(crate) fn parse_headers<'a, 'b>(src: &'a [u8], dst: &'b mut [u8]) -> Result<&'b [Header<'a>]> {
    let hbuf = cast_buf_for_headers(dst)?;

    // This parses into hbuf even if it fails due to an unfinished header line.
    let result = httparse::parse_headers(src, hbuf);

    if let Err(e) = result {
        match e {
            httparse::Error::HeaderName => return Err(HootError::HeaderName),
            httparse::Error::HeaderValue => return Err(HootError::HeaderValue),
            httparse::Error::NewLine => return Err(HootError::NewLine),
            httparse::Error::TooManyHeaders => return Err(HootError::TooManyHeaders),
            // If we get any other error than an indication the name or value
            // is wrong, we've encountered a bug in hoot.
            _ => panic!("Written header is not parseable"),
        }
    }

    // cast_buf_for_headers filles the array with EMPTY_HEADER where name=""
    // httparse::parse_headers still writes all found headers.
    let count = hbuf.iter().take_while(|h| !h.name.is_empty()).count();

    let output = &hbuf[..count];

    Ok(output)
}

pub(crate) fn find_crlf(b: &[u8]) -> Option<usize> {
    let cr = b.iter().position(|c| *c == b'\r')?;
    let maybe_lf = b.get(cr + 1)?;
    (*maybe_lf == b'\n').then_some(cr)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_find_crlf() {
        assert_eq!(find_crlf(b"\r"), None);
        assert_eq!(find_crlf(b"\r\n"), Some(0));
        assert_eq!(find_crlf(b" \r"), None);
        assert_eq!(find_crlf(b" \r\n"), Some(1));
    }
}
