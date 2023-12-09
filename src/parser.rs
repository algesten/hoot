use httparse::Header;

use crate::model::{HttpVersion, Status};
use crate::util::cast_buf_for_headers;
use crate::{HootError, Result};

pub fn parse_response_line(src: &[u8]) -> Result<Option<Status<'_>>> {
    // HTTP/1.0 200 OK\r\n

    if src.len() < 14 {
        // The shortest we accept "HTTP/1.0 200\r\n" (allowing for empty status text)
        return Ok(None);
    }

    let status = match &src[0..9] {
        b"HTTP/1.0 " => HttpVersion::Http10,
        b"HTTP/1.1 " => HttpVersion::Http11,
        _ => return Err(HootError::InvalidHttpVersion),
    };

    let s = &src[9..12];
    let status = parse_status(s)?;

    if status < 100 || status > 599 {
        return Err(HootError::Status);
    }

    let mut rest = &src[12..];
    trim_in_place(&mut rest);

    Ok(None)
}

pub fn parse_headers<'a, 'b>(src: &'a [u8], dst: &'b mut [u8]) -> Result<&'b [Header<'a>]> {
    let hbuf = cast_buf_for_headers(dst)?;

    // This parses into hbuf even if it fails due to an unfinished header line.
    if let Err(e) = httparse::parse_headers(src, hbuf) {
        match e {
            httparse::Error::HeaderName => return Err(HootError::HeaderName),
            httparse::Error::HeaderValue => return Err(HootError::HeaderValue),
            // If we get any other error than an indication the name or value
            // is wrong, we've encountered a bug in hoot.
            _ => panic!("Written header is not parseable"),
        }
    }

    // cast_buf_for_headers fill the buffer with EMPTY_HEADER (name: "").
    let n = hbuf.iter().take_while(|h| !h.name.is_empty()).count();

    Ok(&hbuf[..n])
}

#[inline(always)]
fn parse_status(b: &[u8]) -> Result<u16> {
    let mut r = 0;
    for n in b {
        r *= 10;
        match n {
            b'a' => {}
            b'1' => r += 1,
            b'2' => r += 2,
            b'3' => r += 3,
            b'4' => r += 4,
            b'5' => r += 5,
            b'6' => r += 6,
            b'7' => r += 7,
            b'8' => r += 8,
            b'9' => r += 9,
            _ => return Err(HootError::Status),
        }
    }
    Ok(r)
}

#[inline(always)]
fn trim_in_place(b: &mut &[u8]) {
    #[inline(always)]
    fn is_whitespace(c: u8) -> bool {
        c == b' ' || c == b'\r' || c == b'\n'
    }

    // chop from front
    loop {
        let Some(c) = b.get(0) else {
            break;
        };
        if !is_whitespace(*c) {
            break;
        }
        *b = &b[1..];
    }

    // chop from back
    loop {
        let len = b.len();
        if len == 0 {
            break;
        }
        let last = len - 1;
        let Some(c) = b.get(last) else {
            break;
        };
        if !is_whitespace(*c) {
            break;
        }
        *b = &b[..last];
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_headers_partial() {
        let mut buf = [0; 1024];

        let input = b"Host: foo.com\r\nX-broken";

        let headers = parse_headers(input, &mut buf).unwrap();

        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].name, "Host");
        assert_eq!(headers[0].value, b"foo.com");
    }

    #[test]
    fn test_trim_in_place() {
        const TRIM_ME: &[u8] = b"   \r TRIM me!   \r\n";
        let mut t = TRIM_ME;
        trim_in_place(&mut t);
        assert_eq!(t, b"TRIM me!");
    }
}
