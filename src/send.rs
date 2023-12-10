use core::fmt::Write;

use crate::body::{BODY_CHUNKED, BODY_LENGTH};
use crate::model::SendByteChecker;
use crate::out::Writer;
use crate::parser::parse_headers;
use crate::vars::private;
use crate::{Call, HttpVersion, OVERFLOW};
use crate::{HootError, Result};

use crate::method::*;
use crate::state::*;
use crate::version::*;
use private::*;

impl<'a> Call<'a, INIT, (), (), ()> {
    pub fn http_10(self) -> Call<'a, SEND_LINE, HTTP_10, (), ()> {
        self.transition()
    }

    pub fn http_11(self) -> Call<'a, SEND_LINE, HTTP_11, (), ()> {
        self.transition()
    }
}

macro_rules! write_line_10 {
    ($meth:ident, $meth_up:tt) => {
        pub fn $meth(
            mut self,
            path: &str,
        ) -> Result<Call<'a, SEND_HEADERS, HTTP_10, $meth_up, ()>> {
            write_line_10(self.out.writer(), stringify!($meth_up), path)?;
            Ok(self.transition())
        }
    };
}

fn write_line_10(mut w: Writer<'_, '_>, method: &str, path: &str) -> Result<()> {
    write!(w, "{} {} HTTP/1.0\r\n", method, path).or(OVERFLOW)?;
    w.commit();
    Ok(())
}

macro_rules! write_line_11 {
    ($meth:ident, $meth_up:tt) => {
        pub fn $meth(
            mut self,
            host: &str,
            path: &str,
        ) -> Result<Call<'a, SEND_HEADERS, HTTP_11, $meth_up, ()>> {
            write_line_11(self.out.writer(), stringify!($meth_up), host, path)?;
            Ok(self.transition())
        }
    };
}

fn write_line_11(mut w: Writer<'_, '_>, method: &str, host: &str, path: &str) -> Result<()> {
    write!(w, "{} {} HTTP/1.1\r\nHost: {}\r\n", method, path, host).or(OVERFLOW)?;
    w.commit();
    Ok(())
}

impl<'a> Call<'a, SEND_LINE, HTTP_10, (), ()> {
    write_line_10!(get, GET);
    write_line_10!(head, HEAD);
    write_line_10!(post, POST);
}

impl<'a> Call<'a, SEND_LINE, HTTP_11, (), ()> {
    write_line_11!(get, GET);
    write_line_11!(head, HEAD);
    write_line_11!(post, POST);
    write_line_11!(put, PUT);
    write_line_11!(delete, DELETE);
    // CONNECT
    write_line_11!(options, OPTIONS);
    write_line_11!(trace, TRACE);
}

impl<'a, S, V, M, B> Call<'a, S, V, M, B>
where
    S: State,
    V: Version,
    M: Method,
    B: BodyType,
{
    fn header_raw(mut self, name: &str, bytes: &[u8], trailer: bool) -> Result<Self> {
        // Attempt writing the header
        let mut w = self.out.writer();
        write!(w, "{}: ", name).or(OVERFLOW)?;
        w.write_bytes(bytes)?;
        write!(w, "\r\n").or(OVERFLOW)?;

        if trailer {
            check_headers(name, HEADERS_FORBID_TRAILER, HootError::ForbiddenTrailer)?;
        } else {
            // These headers are forbidden because we write them with
            check_headers(name, HEADERS_FORBID_BODY, HootError::ForbiddenBodyHeader)?;

            match V::version() {
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

        if result.output.len() != 1 {
            // If we don't manage to parse back the hedaer we just wrote, it's a bug in hoot.
            panic!("Failed to parse one written header");
        }

        // If nothing error before this, commit the result to Out.
        w.commit();

        Ok(self)
    }
}

impl<'a, M: Method, V: Version> Call<'a, SEND_HEADERS, V, M, ()> {
    pub fn header(self, name: &str, value: &str) -> Result<Self> {
        self.header_raw(name, value.as_bytes(), false)
    }

    pub fn header_bytes(self, name: &str, bytes: &[u8]) -> Result<Self> {
        self.header_raw(name, bytes, false)
    }
}

impl<'a, M: MethodWithBody> Call<'a, SEND_HEADERS, HTTP_10, M, ()> {
    pub fn with_body(
        mut self,
        length: u64,
    ) -> Result<Call<'a, SEND_BODY, HTTP_10, M, BODY_LENGTH>> {
        let mut w = self.out.writer();
        write!(w, "Content-Length: {}\r\n\r\n", length).or(OVERFLOW)?;
        w.commit();

        self.state.send_byte_checker = Some(SendByteChecker::new(length));

        Ok(self.transition())
    }

    pub fn without_body(mut self) -> Result<Call<'a, RECV_STATUS, HTTP_11, M, ()>> {
        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

impl<'a, M: MethodWithBody> Call<'a, SEND_HEADERS, HTTP_11, M, ()> {
    pub fn with_body(
        mut self,
        length: u64,
    ) -> Result<Call<'a, SEND_BODY, HTTP_11, M, BODY_LENGTH>> {
        let mut w = self.out.writer();
        write!(w, "Content-Length: {}\r\n\r\n", length).or(OVERFLOW)?;
        w.commit();

        self.state.send_byte_checker = Some(SendByteChecker::new(length));

        Ok(self.transition())
    }

    pub fn with_chunked(mut self) -> Result<Call<'a, SEND_BODY, HTTP_11, M, BODY_CHUNKED>> {
        let mut w = self.out.writer();
        write!(w, "Transfer-Encoding: chunked\r\n\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }

    pub fn without_body(mut self) -> Result<Call<'a, RECV_STATUS, HTTP_11, M, ()>> {
        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

impl<'a, V: Version, M: MethodWithoutBody> Call<'a, SEND_HEADERS, V, M, ()> {
    // TODO: Can we find a trait bound that allows us to call this without_body()?
    pub fn send(mut self) -> Result<Call<'a, RECV_STATUS, V, M, ()>> {
        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

impl<'a, V: Version, M: MethodWithBody> Call<'a, SEND_BODY, V, M, BODY_LENGTH> {
    #[inline(always)]
    fn checker(&mut self) -> &mut SendByteChecker {
        self.state
            .send_byte_checker
            .as_mut()
            // If we don't have the checker when in type state SEND_BODY, we got a bug.
            .expect("SendByteCheck when SEND_BODY")
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        // This returns Err if we try to write more bytes than content-length.
        self.checker().append(bytes.len())?;

        let mut w = self.out.writer();
        w.write_bytes(bytes)?;
        w.commit();

        Ok(())
    }

    pub fn complete(mut self) -> Result<Call<'a, RECV_STATUS, V, M, ()>> {
        // This returns Err if we have written less than content-length.
        self.checker().assert_expected()?;

        Ok(self.transition())
    }
}

impl<'a, V: Version, M: MethodWithBody> Call<'a, SEND_BODY, V, M, BODY_CHUNKED> {
    pub fn write_chunk(&mut self, bytes: &[u8]) -> Result<()> {
        // Writing no bytes is ok. Ending the chunk writing is by doing the complete() call.
        if bytes.is_empty() {
            return Ok(());
        }

        let mut w = self.out.writer();

        // chunk length
        write!(w, "{:0x?}\r\n", bytes.len()).or(OVERFLOW)?;

        // chunk
        w.write_bytes(bytes)?;

        // chunk end
        write!(w, "\r\n").or(OVERFLOW)?;

        w.commit();

        Ok(())
    }

    pub fn with_trailer(mut self) -> Result<Call<'a, SEND_TRAILER, V, M, ()>> {
        let mut w = self.out.writer();
        write!(w, "0\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }

    pub fn complete(mut self) -> Result<Call<'a, RECV_STATUS, V, M, ()>> {
        let mut w = self.out.writer();
        write!(w, "0\r\n\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
}

// TODO: ensure trailers are declared in a `Trailer: xxx` header.
impl<'a, V: Version, M: MethodWithBody> Call<'a, SEND_TRAILER, V, M, ()> {
    pub fn trailer(self, name: &str, value: &str) -> Result<Self> {
        self.header_raw(name, value.as_bytes(), true)
    }

    pub fn trailer_bytes(self, name: &str, bytes: &[u8]) -> Result<Self> {
        self.header_raw(name, bytes, true)
    }

    pub fn complete(mut self) -> Result<Call<'a, RECV_STATUS, V, M, ()>> {
        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        w.commit();

        Ok(self.transition())
    }
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
        // Length diffing, then not equal.
        if name.len() != c.len() {
            continue;
        }

        for (a, b) in name.chars().zip(c.chars()) {
            if !a.is_ascii_alphabetic() {
                // a is not even ascii, then not equal.
                continue;
            }
            let norm = a.to_ascii_lowercase();
            if norm != b {
                // after normalizing a, not matching b, then not equal.
                continue;
            }
        }

        // name matched c. This is a forbidden header.
        return Err(err);
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::HootError;

    #[test]
    pub fn test_illegal_header_name() -> Result<()> {
        let mut buf = [0; 1024];

        let x = Call::new(&mut buf)
            .http_11()
            .get("myhost.test:8080", "/path")?
            .header(":bad:", "fine value");

        let e = x.unwrap_err();
        assert_eq!(e, HootError::HeaderName);

        Ok(())
    }

    #[test]
    pub fn test_illegal_header_value() -> Result<()> {
        let mut buf = [0; 1024];

        let x = Call::new(&mut buf)
            .http_11()
            .get("myhost.test:8080", "/path")?
            .header_bytes("x-broken", b"value\0xx");

        let e = x.unwrap_err();
        assert_eq!(e, HootError::HeaderValue);

        Ok(())
    }
    #[test]
    pub fn test_illegal_body_header() -> Result<()> {
        let mut buf = [0; 1024];

        let x = Call::new(&mut buf)
            .http_10()
            .get("/path")?
            .header("transfer-encoding", "chunked");

        let e = x.unwrap_err();
        assert_eq!(e, HootError::ForbiddenBodyHeader);

        Ok(())
    }

    #[test]
    pub fn test_illegal_http11_header() -> Result<()> {
        let mut buf = [0; 1024];

        let x = Call::new(&mut buf)
            .http_11()
            .get("myhost.test:8080", "/path")?
            .header("Host", "another.test:4489");

        let e = x.unwrap_err();
        assert_eq!(e, HootError::ForbiddenHttp11Header);

        Ok(())
    }
}
