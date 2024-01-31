use core::fmt;
use core::fmt::Write;
use core::mem;
use core::str;
use httparse::Header as InnerHeader;

use crate::error::{Result, OVERFLOW};
use crate::out::Writer;
use crate::parser::parse_headers;
use crate::util::compare_lowercase_ascii;
use crate::{HootError, HttpVersion};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Header<'a> {
    name: &'a str,
    value: &'a [u8],
}

impl<'a> Header<'a> {
    #[inline(always)]
    pub fn name(&self) -> &str {
        self.name
    }

    #[inline(always)]
    pub fn try_value(&self) -> Option<&str> {
        str::from_utf8(self.value).ok()
    }

    #[inline(always)]
    pub fn value(&self) -> &str {
        self.try_value().expect("header value to be valid utf-8")
    }

    #[inline(always)]
    pub fn value_raw(&self) -> &[u8] {
        self.value
    }
}

impl<'a> fmt::Debug for Header<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("Header");
        f.field("name", &self.name);
        if let Some(value) = self.try_value() {
            f.field("value", &value);
        } else {
            f.field("value", &self.value);
        }
        f.finish()
    }
}

pub(crate) fn transmute_headers<'a, 'b>(headers: &'b [InnerHeader<'a>]) -> &'b [Header<'a>] {
    // SAFETY: Our goal is to have hoot::Header be structurally the same
    // as httparse::Header. This is asserted by the test below.
    unsafe { mem::transmute(headers) }
}

pub(crate) fn check_and_output_header(
    mut w: Writer,
    version: HttpVersion,
    name: &str,
    bytes: &[u8],
    trailer: bool,
) -> Result<()> {
    // Writer header
    write!(w, "{}: ", name).or(OVERFLOW)?;
    w.write_bytes(bytes)?;
    write!(w, "\r\n").or(OVERFLOW)?;

    if trailer {
        check_headers(name, HEADERS_FORBID_TRAILER, HootError::ForbiddenTrailer)?;
    } else {
        // These headers are forbidden because we write them with
        check_headers(name, HEADERS_FORBID_BODY, HootError::ForbiddenBodyHeader)?;

        match version {
            HttpVersion::Http10 => {
                // TODO: forbid specific headers for 1.0
            }
            HttpVersion::Http11 => {
                check_headers(name, HEADERS_FORBID_11, HootError::ForbiddenHttp11Header)?
            }
        }
    }

    // TODO: forbid headers that are not allowed to be repeated

    // Parse the written result to see if httparse can validate it.
    let (written, buf) = w.split_and_borrow();

    let result = parse_headers(written, buf)?;

    if result.len() != 1 {
        // If we don't manage to parse back the hedaer we just wrote, it's a bug in hoot.
        panic!("Failed to parse one written header");
    }

    // If nothing error before this, commit the result to Out.
    w.commit();

    Ok(())
}

// Headers that are not allowed because we set them as part of making a call.
const HEADERS_FORBID_BODY: &[&str] = &[
    // header set by with_body()
    "content-length",
    // header set by with_chunked()
    "transfer-encoding",
];

const HEADERS_FORBID_11: &[&str] = &[
    // host is already set by the Call::<verb>(host, path)
    "host",
];

const HEADERS_FORBID_TRAILER: &[&str] = &[
    "transfer-encoding",
    "content-length",
    "host",
    "cache-control",
    "max-forwards",
    "authorization",
    "set-cookie",
    "content-type",
    "content-range",
    "te",
    "trailer",
];

// message framing headers (e.g., Transfer-Encoding and Content-Length),
// routing headers (e.g., Host),
// request modifiers (e.g., controls and conditionals, like Cache-Control, Max-Forwards, or TE),
// authentication headers (e.g., Authorization or Set-Cookie),
// or Content-Encoding, Content-Type, Content-Range, and Trailer itself.

fn check_headers(name: &str, forbidden: &[&str], err: HootError) -> Result<()> {
    for c in forbidden {
        if !compare_lowercase_ascii(name, c) {
            continue;
        }

        // name matched c. This is a forbidden header.
        return Err(err);
    }

    Ok(())
}
#[cfg(test)]
mod test {
    use super::*;
    use memoffset::offset_of;

    #[test]
    fn assert_httparse_header_transmutability() {
        assert_eq!(mem::size_of::<Header>(), mem::size_of::<InnerHeader>());
        assert_eq!(mem::align_of::<Header>(), mem::align_of::<InnerHeader>());
        assert_eq!(offset_of!(Header, name), offset_of!(InnerHeader, name));
        assert_eq!(offset_of!(Header, value), offset_of!(InnerHeader, value));
    }
}
