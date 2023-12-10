//! no_std, allocation free http library.

// For tests we use std.
// #![cfg_attr(not(test), no_std)]

mod out;
mod util;

mod model;
use core::str::Utf8Error;

pub use model::{Call, CallState, HttpVersion, Output, Status};

mod vars;
pub use vars::{body, method, state, version};

mod recv;
pub use recv::Attempt;
mod parser;
mod send;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HootError {
    /// The borrowed buffer did not have enough space to hold the
    /// data we attempted to write.
    ///
    /// Call `.flush()`, write the output to the transport followed by `Call::resume()`.
    OutputOverflow,

    /// Invalid byte in header name.
    HeaderName,

    /// Invalid byte in header value.
    HeaderValue,

    /// Invalid Response status.
    Status,

    /// Invalid byte in new line.
    NewLine,

    /// Parsed more headers than provided buffer can contain.
    TooManyHeaders,

    /// Parsing headers (for sending or receiving) uses leftover space in the
    /// buffer. This error means there was not enough "spare" space to parse
    /// any headers.
    ///
    /// Call `.flush()`, write the output to the transport followed by `Call::resume()`.
    InsufficientSpaceToParseHeaders,

    /// Encountered a forbidden header name.
    ///
    /// `content-length` and `transfer-encoding` must be set using
    /// `with_body()` and `with_body_chunked()`.
    ForbiddenBodyHeader,

    /// Header is not allowed for HTTP/1.1
    ForbiddenHttp11Header,

    /// The trailer name is not allowed.
    ForbiddenTrailer,

    /// Attempt to send more content than declared in the `Content-Length` header.
    SentMoreThanContentLength,

    /// Attempt to send less content than declared in the `Content-Length` header.
    SentLessThanContentLength,

    /// Failed to read bytes as &str
    ConvertBytesToStr,

    /// The requested HTTP version does not match the response HTTP version.
    HttpVersionMismatch,
}

pub(crate) static OVERFLOW: Result<()> = Err(HootError::OutputOverflow);

pub type Result<T> = core::result::Result<T, HootError>;

impl From<Utf8Error> for HootError {
    fn from(_: Utf8Error) -> Self {
        HootError::ConvertBytesToStr
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    pub fn test_req_get() -> Result<()> {
        let mut buf = [0; 1024];

        // ************* GET REQUEST *****************

        // Call::new starts a new request. The buffer can be on the stack, heap or anywe you want.
        // It is borrowed until we call .flush().
        let output = Call::new(&mut buf)
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

        // `Call::resume` takes the state and continues where we left off before calling `.flush()`.
        // The buffer to borrow can be the same we used initially or not. Subsequent output is
        // written to this buffer.
        let output = Call::resume(state, &mut buf)
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

        // Resume call using the buffer.
        let call = Call::resume(output.ready(), &mut buf);

        // Try read incomplete input.
        let attempt = call.try_read_status(b"HTTP/1.")?;
        assert!(attempt.is_failure());

        // Get the Call back from an failed attempt.
        let call = attempt.revert().unwrap();

        // Try read complete input
        let attempt = call.try_read_status(b"HTTP/1.1 200 OK\r\n")?;
        assert!(attempt.is_success());

        // How many bytes of the input was consumed. This can be used to move
        // cursors in some input buffer.
        assert_eq!(attempt.consumed(), 17);

        // The parsed status
        let status = attempt.output().unwrap();
        assert_eq!(status, &Status(HttpVersion::Http11, 200, "OK"));

        // Complete the attempt, which gives us the call in a state expecting to read headers.
        let call = attempt.complete().unwrap();

        // ************* READ RESPONSE HEADERS *****************

        Ok(())
    }
}
