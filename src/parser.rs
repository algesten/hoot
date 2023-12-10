use core::str;
use httparse::Header;

use crate::res::Status;
use crate::util::cast_buf_for_headers;
use crate::HttpVersion;
use crate::{HootError, Result};

pub fn parse_response_line(src: &[u8]) -> Result<ParseResult<Status>> {
    // HTTP/1.0 200 OK\r\n

    const EMPTY_STATUS: Status = Status(HttpVersion::Http11, 0, "");
    const EMPTY_PARSE: ParseResult<Status> = ParseResult {
        complete: false,
        consumed: 0,
        output: EMPTY_STATUS,
    };

    let Some(crlf) = find_crlf(src) else {
        return Ok(EMPTY_PARSE);
    };

    // Limit source to anything up to CRLF
    let src = &src[..crlf];

    if src.len() < 12 {
        // The shortest we accept "HTTP/1.0 200" (allowing for empty status text)
        // If the content before crlf is less than this, we can never parse this
        // as a status.
        return Err(HootError::Status);
    }

    let version = match &src[0..9] {
        b"HTTP/1.0 " => HttpVersion::Http10,
        b"HTTP/1.1 " => HttpVersion::Http11,
        _ => return Err(HootError::Status),
    };

    let s = &src[9..12];
    let status = parse_u16(s).map_err(|_| HootError::Status)?;

    if status < 100 || status > 599 {
        return Err(HootError::Status);
    }

    let consumed = src.len() + 2; // + crlf

    let mut text = &src[12..];
    trim_in_place(&mut text);

    // Convert bytes to str.
    let text = str::from_utf8(text)?;

    Ok(ParseResult {
        complete: true,
        consumed,
        output: Status(version, status, text),
    })
}

pub(crate) struct ParseResult<T> {
    pub complete: bool,
    pub consumed: usize,
    pub output: T,
}

pub(crate) fn parse_headers<'a, 'b>(
    src: &'a [u8],
    dst: &'b mut [u8],
) -> Result<ParseResult<&'b [Header<'a>]>> {
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

    // Err case handled above.
    let result = result.unwrap();

    let (complete, consumed) = match result {
        httparse::Status::Complete((consumed, _)) => (true, consumed),
        httparse::Status::Partial => (false, 0),
    };

    // cast_buf_for_headers filles the array with EMPTY_HEADER where name=""
    // httparse::parse_headers still writes all found headers.
    let count = hbuf.iter().take_while(|h| !h.name.is_empty()).count();

    let output = &hbuf[..count];

    Ok(ParseResult {
        complete,
        consumed,
        output,
    })
}

pub(crate) fn find_crlf(b: &[u8]) -> Option<usize> {
    let cr = b.iter().position(|c| *c == b'\r')?;
    let maybe_lf = b.get(cr + 1)?;
    (*maybe_lf == b'\n').then_some(cr)
}

#[inline(always)]
fn parse_u16(b: &[u8]) -> core::result::Result<u16, ()> {
    let mut r = 0;
    for n in b {
        r *= 10;
        match n {
            b'0' => {}
            b'1' => r += 1,
            b'2' => r += 2,
            b'3' => r += 3,
            b'4' => r += 4,
            b'5' => r += 5,
            b'6' => r += 6,
            b'7' => r += 7,
            b'8' => r += 8,
            b'9' => r += 9,
            _ => return Err(()),
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
    fn test_find_crlf() {
        assert_eq!(find_crlf(b"\r"), None);
        assert_eq!(find_crlf(b"\r\n"), Some(0));
        assert_eq!(find_crlf(b" \r"), None);
        assert_eq!(find_crlf(b" \r\n"), Some(1));
    }

    #[test]
    fn test_parse_headers_partial() {
        let mut buf = [0; 1024];

        let input = b"Host: foo.com\r\nX-broken";

        let result = parse_headers(input, &mut buf).unwrap();
        assert_eq!(result.output.len(), 1);
        assert_eq!(result.consumed, 0);
        assert_eq!(result.output[0].name, "Host");
        assert_eq!(result.output[0].value, b"foo.com");
        assert!(!result.complete);
    }

    #[test]
    fn test_parse_headers_complete() {
        let mut buf = [0; 1024];

        let input = b"Host: foo.com\r\nX-fine:    foo\r\n\r\n";

        let result = parse_headers(input, &mut buf).unwrap();
        assert_eq!(result.output.len(), 2);
        assert_eq!(result.consumed, 33);
        assert_eq!(result.output[1].name, "X-fine");
        assert_eq!(result.output[1].value, b"foo");
        assert!(result.complete);
    }

    #[test]
    fn test_trim_in_place() {
        const TRIM_ME: &[u8] = b"   \r TRIM me!   \r\n";
        let mut t = TRIM_ME;
        trim_in_place(&mut t);
        assert_eq!(t, b"TRIM me!");
    }

    #[test]
    fn test_parse_response_line() {
        let r = parse_response_line(b"HTTP/1.0 200\r").unwrap();
        assert_eq!(r.complete, false);
        assert_eq!(r.consumed, 0);
        assert_eq!(r.output, Status(HttpVersion::Http11, 0, ""));

        let r = parse_response_line(b"HTTP/1.0 200\r\n").unwrap();
        assert_eq!(r.complete, true);
        assert_eq!(r.consumed, 14);
        assert_eq!(r.output, Status(HttpVersion::Http10, 200, ""));

        let r = parse_response_line(b"HTTP/1.1 418 I'm a teapot\r\n").unwrap();
        assert_eq!(r.complete, true);
        assert_eq!(r.consumed, 27);
        assert_eq!(r.output, Status(HttpVersion::Http11, 418, "I'm a teapot"));
    }
}
