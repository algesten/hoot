use crate::header::transmute_headers;
use crate::util::cast_buf_for_headers;
use crate::{Header, Result};

pub(crate) fn parse_headers<'a, 'b>(src: &'a [u8], dst: &'b mut [u8]) -> Result<&'b [Header<'a>]> {
    let hbuf = cast_buf_for_headers(dst);

    // This parses into hbuf even if it fails due to an unfinished header line.
    httparse::parse_headers(src, hbuf)?;

    // cast_buf_for_headers fills the array with EMPTY_HEADER where name=""
    // httparse::parse_headers still writes all found headers.
    // This behavior is asserted in a test below.
    let count = hbuf.iter().take_while(|h| !h.name.is_empty()).count();

    // Transmute to our own header type.
    let output = transmute_headers(&hbuf[..count]);

    Ok(output)
}

pub(crate) fn find_crlf(b: &[u8]) -> Option<usize> {
    let cr = b.iter().position(|c| *c == b'\r')?;
    let maybe_lf = b.get(cr + 1)?;
    if *maybe_lf == b'\n' {
        Some(cr)
    } else {
        None
    }
}

#[cfg(test)]
mod test {
    use core::mem;

    use super::*;

    #[test]
    fn test_find_crlf() {
        assert_eq!(find_crlf(b"\r"), None);
        assert_eq!(find_crlf(b"\r\n"), Some(0));
        assert_eq!(find_crlf(b" \r"), None);
        assert_eq!(find_crlf(b" \r\n"), Some(1));
    }

    #[test]
    fn check_partial_httparse_parse_headers() {
        const BUF_SIZE: usize = 2048;

        // Depending on 32- or 64-bit architecture this might differ.
        const HEADER_SIZE: usize = mem::size_of::<httparse::Header>();
        const HEADER_COUNT: usize = BUF_SIZE / HEADER_SIZE;

        let mut buf = [0; BUF_SIZE];
        let mut headers = cast_buf_for_headers(&mut buf);
        assert_eq!(headers.len(), HEADER_COUNT);

        // All values should be "" before we try parsing.
        assert!(headers.iter().all(|h| h.name == "" && h.value == b""));

        // (missing last \n)
        const PARTIAL_INPUT: &[u8] = b"My-Header: 42\r\nSome-Partial: foo\r";

        let r = httparse::parse_headers(PARTIAL_INPUT, &mut headers);

        // the parse doesn't fail.
        assert!(r.is_ok());

        // but it also isn't complete.
        assert!(matches!(r.unwrap(), httparse::Status::Partial));

        // Despite that, we can still detect the headers we did find by
        // checking for the first ""/""
        assert_eq!(headers[0].name, "My-Header");
        assert_eq!(headers[0].value, b"42");
        assert_eq!(headers[1].name, "");
        assert_eq!(headers[1].value, b"");
    }
}
