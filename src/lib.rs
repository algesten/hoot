//! no_std, allocation free http library.

// For tests we use std.
#![cfg_attr(not(test), no_std)]

mod out;
mod util;

mod vars;
pub use vars::{body, method, state, version};

mod parser;

mod error;
pub use error::HootError;
pub(crate) use error::Result;

mod req;
pub use req::{Output, Request, ResumeToken};

mod res;
pub use res::{Response, Status};

// Re-export this
pub use httparse::Header;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HttpVersion {
    Http10,
    Http11,
}

#[cfg(any(std, test))]
mod std_impls {
    use super::*;
    use std::fmt;

    impl fmt::Debug for HttpVersion {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Http10 => write!(f, "Http10"),
                Self::Http11 => write!(f, "Http11"),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    pub fn test_req_get() -> Result<()> {
        let mut buf = [0; 1024];

        // ************* GET REQUEST *****************

        // Request::new starts a new request. The buffer can be on the stack, heap or anywe you want.
        // It is borrowed until we call .flush().
        let output = Request::new(&mut buf)
            // First we select if this is HTTP/1.0 or HTTP/1.1
            .http_11()
            // Then comes the verb (method) + PATH. The methods are narrowed by the typo only be
            // valid for HTTP/1.0. This writes to the underlying buffer – hence the Res return in
            // case buffer overflows.
            .get("myhost.test:8080", "/some-path")?
            // At any point we can release the buffer. This returns `Output`, which we need to
            // write to the underlying transport.
            .flush();

        const EXPECTED_LINE: &[u8] = b"GET /some-path HTTP/1.1\r\nHost: myhost.test:8080\r\n";

        // Output derefs to `&[u8]`, but if that feels opaque, we can use `as_bytes()`.
        assert_eq!(&*output, EXPECTED_LINE);
        assert_eq!(output.as_bytes(), EXPECTED_LINE);

        // Once we have written the output to the underlying transport, we call `ready()`, to
        // get a state we can resume.
        let state = output.ready();

        // ************* SEND HEADERS *****************

        // `Request::resume` takes the state and continues where we left off before calling `.flush()`.
        // The buffer to borrow can be the same we used initially or not. Subsequent output is
        // written to this buffer.
        let output = Request::resume(state, &mut buf)
            // Headers write to the buffer, hence the Result return.
            .header("accept", "text/plain")?
            .header("x-my-thing", "martin")?
            // Finish writes the header end into the buffer and transitions the state to expect
            // response input.
            // The `.finish()` call is only available for HTTP verbs that have no body.
            .send()?
            // Again, release the buffer to write to a transport.
            .flush();

        const EXPECTED_HEADERS: &[u8] = b"accept: text/plain\r\nx-my-thing: martin\r\n\r\n";

        assert_eq!(&*output, EXPECTED_HEADERS);

        // ************* READ STATUS LINE *****************

        //  Response from the resume token.
        let response = output.ready().into_response();

        // Try read incomplete input.
        let attempt = response.try_read_response(b"HTTP/1.", &mut buf)?;
        assert!(!attempt.is_success());

        // Get the Response back from an failed attempt. unwrap_retry() will
        // definitely work since !attempt.is_success()
        let response = attempt.next().unwrap_retry();

        // Try read complete input
        let attempt =
            response.try_read_response(b"HTTP/1.1 200 OK\r\nHost: foo\r\n\r\n", &mut buf)?;
        assert!(attempt.is_success());

        // Status line information.
        let status = attempt.status().unwrap();
        assert_eq!(status, &Status(HttpVersion::Http11, 200, "OK"));

        // Headers
        let headers = attempt.headers().unwrap();
        assert_eq!(headers[0].name, "Host");
        assert_eq!(headers[0].value, b"foo");

        Ok(())
    }
}
