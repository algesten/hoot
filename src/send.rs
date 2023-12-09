use core::fmt::Write;

use crate::body::{BODY_CHUNKED, BODY_LENGTH, BODY_NONE};
use crate::out::Writer;
use crate::util::cast_buf_for_headers;
use crate::vars::private;
use crate::{Call, OVERFLOW};
use crate::{HootError, Result};

use crate::method::*;
use crate::state::*;
use crate::version::*;
use httparse::parse_headers;
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

impl<'a, M: Method, V: Version> Call<'a, SEND_HEADERS, V, M, ()> {
    pub fn header(self, name: &str, value: &str) -> Result<Self> {
        self.header_bytes(name, value.as_bytes())
    }

    pub fn header_bytes(mut self, name: &str, bytes: &[u8]) -> Result<Self> {
        // Attempt writing the header
        let mut w = self.out.writer();
        write!(w, "{}: ", name).or(OVERFLOW)?;
        w.write_bytes(bytes)?;
        write!(w, "\r\n").or(OVERFLOW)?;

        // Parse the written result to see if httparse can validate it.
        let (written, buf) = w.split_and_borrow();
        let headers = cast_buf_for_headers(buf)?;

        // These headers are forbidden because we write them with
        check_forbidden_headers(name, HEADERS_FORBID_BODY, HootError::ForbiddenBodyHeader)?;

        // TODO: forbid specific headers for 1.0/1.1
        // TODO: forbid headers that are not allowed to be repeated

        if let Err(e) = parse_headers(written, headers) {
            match e {
                httparse::Error::HeaderName => return Err(HootError::HeaderName),
                httparse::Error::HeaderValue => return Err(HootError::HeaderValue),
                // If we get any other error than an indication the name or value
                // is wrong, we've encountered a bug in hoot.
                _ => panic!("Written header is not parseable"),
            }
        };

        // If nothing error before this, commit the result to Out.
        w.commit();

        Ok(self)
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
        Ok(self.transition())
    }

    pub fn without_body(mut self) -> Result<Call<'a, RECV_STATUS, HTTP_11, M, BODY_NONE>> {
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
        Ok(self.transition())
    }

    pub fn with_chunked(mut self) -> Result<Call<'a, SEND_BODY, HTTP_11, M, BODY_CHUNKED>> {
        let mut w = self.out.writer();
        write!(w, "Transfer-Encoding: chunked\r\n\r\n").or(OVERFLOW)?;
        w.commit();
        Ok(self.transition())
    }

    pub fn without_body(mut self) -> Result<Call<'a, RECV_STATUS, HTTP_11, M, BODY_NONE>> {
        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        w.commit();
        Ok(self.transition())
    }
}

impl<'a, V: Version, M: MethodWithoutBody> Call<'a, SEND_HEADERS, V, M, ()> {
    // TODO: Can we find a trait bound that allows us to call this without_body()?
    pub fn finish(mut self) -> Result<Call<'a, RECV_STATUS, V, M, BODY_NONE>> {
        let mut w = self.out.writer();
        write!(w, "\r\n").or(OVERFLOW)?;
        w.commit();
        Ok(self.transition())
    }
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
}

// Headers that are not allowed because we set them as part of making a call.
const HEADERS_FORBID_BODY: &[&str] = &[
    // header set by with_body()
    "content-length",
    // header set by with_chunked()
    "transfer-encoding",
];

fn check_forbidden_headers(name: &str, forbidden: &[&str], err: HootError) -> Result<()> {
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
