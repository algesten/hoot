use httparse::Header;

use crate::util::cast_buf_for_headers;
use crate::{HootError, Result};

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
}
