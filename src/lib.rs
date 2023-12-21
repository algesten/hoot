//! no_std, allocation free http library.

// For tests we use std.
#![cfg_attr(not(test), no_std)]

mod chunk;

mod out;
mod util;

mod vars;

mod parser;

mod error;
pub use error::HootError;
pub(crate) use error::Result;

mod req;
pub use req::{Output, Request, ResumeToken};

mod res;
pub use res::{BodyPart, Response, Status};

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

        // Request::new starts a new request. The buffer can be on the stack, heap or anywhere you want.
        // It is borrowed until we call .flush().
        let output = Request::new(&mut buf)
            // First we select if this is HTTP/1.0 or HTTP/1.1
            .http_11()
            // Then comes the verb (method) + host and path. The methods are narrowed by http11 to
            // only be valid for HTTP/1.1. This writes to the underlying buffer – hence the result
            // return in case buffer overflows.
            .get("myhost.test:8080", "/some-path")?
            // At any point we can release the buffer. This returns `Output`, which we need to
            // write to the underlying transport.
            .flush();

        const EXPECTED_LINE: &[u8] = b"GET /some-path HTTP/1.1\r\nHost: myhost.test:8080\r\n";

        // Output derefs to `&[u8]`, but if that feels opaque, we can use `as_bytes()`.
        assert_eq!(&*output, EXPECTED_LINE);
        assert_eq!(output.as_bytes(), EXPECTED_LINE);

        // Once we have written the output to the underlying transport, we call `ready()`, to
        // get a "resumt token" we can resume the request from. This releases the borrowed buffer.
        let state = output.ready();

        // ************* SEND HEADERS *****************

        // `Request::resume` takes the resume token and continues where we left off before calling `.flush()`.
        // The buffer to borrow can be the same we used initially or not. Subsequent output is
        // written to this buffer.
        let output = Request::resume(state, &mut buf)
            // Headers write to the buffer, hence the Result return.
            .header("accept", "text/plain")?
            .header("x-my-thing", "martin")?
            // Finish writes the header end into the buffer and transitions the state to expect
            // response input.
            // The `.send()` call is only available for HTTP verbs that have no body.
            .send()?
            // Again, release the buffer to write to a transport.
            .flush();

        const EXPECTED_HEADERS: &[u8] = b"accept: text/plain\r\nx-my-thing: martin\r\n\r\n";

        assert_eq!(&*output, EXPECTED_HEADERS);

        // Free the buffer.
        let resume = output.ready();

        // ************* READ STATUS LINE *****************

        // After calling `send()` above, the resume token can now be converted into a response
        let mut response = resume.into_response();

        // Try read incomplete input. The provided buffer is required to parse response headers.
        // The buffer can be the same as in the request or another one.
        let attempt = response.try_read_response(b"HTTP/1.", &mut buf)?;
        assert!(!attempt.is_success());

        const COMPLETE: &[u8] = b"HTTP/1.1 200 OK\r\nHost: foo\r\nContent-Length: 10\r\n\r\n";

        // Try read complete input (and succeed). Borrow the buffer again.
        let attempt = response.try_read_response(COMPLETE, &mut buf)?;
        assert!(attempt.is_success());

        // Read status line information.
        let status = attempt.status().unwrap();
        assert_eq!(status.version(), HttpVersion::Http11);
        assert_eq!(status.code(), 200);
        assert_eq!(status.text(), "OK");

        // Read headers.
        let headers = attempt.headers().unwrap();
        assert_eq!(headers[0].name, "Host");
        assert_eq!(headers[0].value, b"foo");
        assert_eq!(headers[1].name, "Content-Length");
        assert_eq!(headers[1].value, b"10");

        // Once done with status and headers we proceed to reading the body.
        let mut response = response.proceed();

        const ENTIRE_BODY: &[u8] = b"Is a body!";

        // We can read a partial body. Depending on headers this can either be delimited by
        // Content-Length, or use Transfer-Encoding: chunked. The response keeps track of
        // how much data we read. The buffer is borrowed for the lifetime of the returned
        let part = response.read_body(&ENTIRE_BODY[0..5], &mut buf)?;

        // Check how much of the input was used. This might not be the entire available input.
        // In this case it is though.
        assert_eq!(part.input_used(), 5);

        // The response body is not finished, since we got a content-length of 10 and we read 5.
        assert!(!part.is_finished());

        // The read/decoded output is inside the part.
        assert_eq!(part.output(), b"Is a ");

        // Read the rest. This again borrows the buffer.
        let part = response.read_body(&ENTIRE_BODY[5..], &mut buf)?;

        // Content is now 10, fulfilling the content-length buffer.
        assert!(part.is_finished());

        // The read/decoded output.
        assert_eq!(part.output(), b"body!");

        // Should be finished now.
        assert!(response.is_finished());

        // Consider the body reading finished. Returns an error if we call this too early,
        // i.e. if we have not read to finish, and response.is_finished() is false.
        response.finish()?;

        Ok(())
    }
}
