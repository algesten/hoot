use core::marker::PhantomData;
use core::mem;
use core::str;

use crate::chunk::Dechunker;
use crate::header::transmute_headers;
use crate::req::CallState;
use crate::util::{cast_buf_for_headers, compare_lowercase_ascii, LengthChecker};
use crate::vars::state::*;
use crate::vars::{private::*, M};
use crate::{Header, HootError, HttpVersion};
use crate::{Result, ResumeToken};

pub struct Response<S: State> {
    _typ: PhantomData<S>,
    state: CallState,
}

impl Response<()> {
    pub(crate) fn resume(request: ResumeToken<ENDED, (), (), ()>) -> Response<RECV_RESPONSE> {
        Response {
            _typ: PhantomData,
            state: request.into_state(),
        }
    }

    #[cfg(test)]
    fn new_test() -> Response<RECV_RESPONSE> {
        Response {
            _typ: PhantomData,
            state: CallState {
                version: Some(HttpVersion::Http11),
                method: Some(M::GET),
                ..Default::default()
            },
        }
    }
}

impl<S: State> Response<S> {
    fn transition<S2: State>(self) -> Response<S2> {
        // SAFETY: this only changes the type state of the PhantomData
        unsafe { mem::transmute(self) }
    }

    fn do_try_read_response<'a, 'b>(
        &mut self,
        input: &'a [u8],
        buf: &'b mut [u8],
    ) -> Result<ResponseAttempt<'a, 'b>> {
        let already_read_response = self.state.recv_body_mode.is_some();

        // Status/header reads only work once.
        if already_read_response {
            return Ok(ResponseAttempt::empty());
        }

        let headers = cast_buf_for_headers(buf);
        let mut r = httparse::Response::new(headers);

        let n = match r.parse(input)? {
            httparse::Status::Complete(v) => v,
            httparse::Status::Partial => return Ok(ResponseAttempt::empty()),
        };

        let ver = match r.version.unwrap() {
            0 => HttpVersion::Http10,
            1 => HttpVersion::Http11,
            _ => return Err(HootError::Version),
        };

        let status = Status(ver, r.code.unwrap(), r.reason.unwrap_or(""));

        // Derive body mode from knowledge this far.
        let http10 = ver == HttpVersion::Http10;
        let method = self.state.method.unwrap(); // Ok for same reason as above.
        let headers = transmute_headers(r.headers);
        let mode = RecvBodyMode::from(http10, method, status.1, headers)?;

        // If we are awaiting a length, put a length checker in place
        if let RecvBodyMode::LengthDelimited(len) = mode {
            if len > 0 {
                self.state.recv_checker = Some(LengthChecker::new(len));
            }
        }

        // Remember the body mode
        self.state.recv_body_mode = Some(mode);

        Ok(ResponseAttempt {
            success: true,
            input_used: n,
            status: Some(status),
            headers: Some(headers),
        })
    }
}

pub struct ResponseAttempt<'a, 'b> {
    success: bool,
    input_used: usize,
    status: Option<Status<'a>>,
    headers: Option<&'b [Header<'a>]>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Status<'a>(HttpVersion, u16, &'a str);

impl Status<'_> {
    pub fn version(&self) -> HttpVersion {
        self.0
    }

    pub fn code(&self) -> u16 {
        self.1
    }

    pub fn text(&self) -> &str {
        self.2
    }
}

impl<'a, 'b> ResponseAttempt<'a, 'b> {
    const fn empty() -> Self {
        ResponseAttempt {
            success: false,
            input_used: 0,
            status: None,
            headers: None,
        }
    }

    pub fn is_success(&self) -> bool {
        self.success
    }

    pub fn input_used(&self) -> usize {
        self.input_used
    }

    pub fn status(&self) -> Option<&Status<'a>> {
        self.status.as_ref()
    }

    pub fn headers(&self) -> Option<&'b [Header<'a>]> {
        self.headers
    }
}

impl Response<RECV_RESPONSE> {
    pub fn try_read_response<'a, 'b>(
        &mut self,
        input: &'a [u8],
        buf: &'b mut [u8],
    ) -> Result<ResponseAttempt<'a, 'b>> {
        self.do_try_read_response(input, buf)
    }

    pub fn proceed(self) -> Response<RECV_BODY> {
        self.transition()
    }
}

impl Response<RECV_BODY> {
    pub fn read_body<'a, 'b>(&mut self, src: &'a [u8], dst: &'b mut [u8]) -> Result<BodyPart<'b>> {
        let already_read_response = self.state.recv_body_mode.is_some();

        // It's valid to skip try_read_response() and progress straight to reading
        // the body. This ensures we skip the corresponding input.
        if !already_read_response {
            let r = self.do_try_read_response(src, dst)?;

            // Still not enough input for the entire status and headers. Need
            // to try again later.
            if !r.success {
                return Ok(BodyPart::empty());
            }
        }

        // If we already read to completion, do not use any more input.
        if self.state.did_read_to_end {
            return Ok(BodyPart::empty());
        }

        // unwrap is ok because we can't be in state RECV_BODY without setting it.
        let bit = match self.state.recv_body_mode.unwrap() {
            RecvBodyMode::LengthDelimited(_) => self.read_limit(src, dst, true),
            RecvBodyMode::Chunked => self.read_chunked(src, dst),
            RecvBodyMode::CloseDelimited => self.read_limit(src, dst, false),
        }?;

        if bit.finished {
            self.state.did_read_to_end = true;
        }

        Ok(BodyPart {
            input_used: bit.input_used,
            output: bit.output,
            finished: bit.finished,
        })
    }

    fn read_limit<'a, 'b>(
        &mut self,
        src: &'a [u8],
        dst: &'b mut [u8],
        use_checker: bool,
    ) -> Result<BodyBit<'b>> {
        let input_used = src.len().min(dst.len());

        let mut finished = false;
        if use_checker {
            let checker = self.state.recv_checker.as_mut().unwrap();
            checker.append(input_used, HootError::RecvMoreThanContentLength)?;
            finished = checker.complete();
        }

        let output = &mut dst[..input_used];
        output.copy_from_slice(&src[..input_used]);
        Ok(BodyBit {
            input_used,
            output,
            finished,
        })
    }

    fn read_chunked<'a>(&mut self, src: &[u8], dst: &'a mut [u8]) -> Result<BodyBit<'a>> {
        if self.state.dechunker.is_none() {
            self.state.dechunker = Some(Dechunker::new());
        }
        let dechunker = self.state.dechunker.as_mut().unwrap();
        let (input_used, produced_output) = dechunker.parse_input(src, dst)?;

        let output = &mut dst[..produced_output];
        let finished = dechunker.is_ended();

        Ok(BodyBit {
            input_used,
            output,
            finished,
        })
    }

    pub fn is_finished(&self) -> bool {
        let mode = self.state.recv_body_mode.unwrap();
        let close_delimited = matches!(mode, RecvBodyMode::CloseDelimited);
        !close_delimited && self.state.did_read_to_end
    }

    pub fn finish(self) -> Result<Response<ENDED>> {
        if let Some(checker) = &self.state.recv_checker {
            checker.assert_expected(HootError::RecvLessThanContentLength)?;
        }

        if !self.is_finished() {
            return Err(HootError::BodyNotFinished);
        }

        Ok(self.transition())
    }
}

struct BodyBit<'b> {
    input_used: usize,
    output: &'b [u8],
    finished: bool,
}

pub struct BodyPart<'b> {
    input_used: usize,
    output: &'b [u8],
    finished: bool,
}

impl BodyPart<'_> {
    fn empty() -> Self {
        BodyPart {
            input_used: 0,
            output: &[],
            finished: false,
        }
    }

    pub fn input_used(&self) -> usize {
        self.input_used
    }

    pub fn output(&self) -> &[u8] {
        &*self.output
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RecvBodyMode {
    /// Delimited by content-length. 0 is also a valid value when we don't expect a body,
    /// due to HEAD or status, but still want to leave the socket open.
    LengthDelimited(u64),
    /// Chunked transfer encoding
    Chunked,
    /// Expect remote to close at end of body.
    CloseDelimited,
}

impl RecvBodyMode {
    pub fn from(http10: bool, method: M, status_code: u16, headers: &[Header<'_>]) -> Result<Self> {
        let is_success = status_code >= 200 && status_code <= 299;
        let is_informational = status_code >= 100 && status_code <= 199;

        let has_no_body =
            // https://datatracker.ietf.org/doc/html/rfc2616#section-4.3
            // All responses to the HEAD request method
            // MUST NOT include a message-body, even though the presence of entity-
            // header fields might lead one to believe they do.
            method == M::HEAD ||
            // A client MUST ignore any Content-Length or Transfer-Encoding
            // header fields received in a successful response to CONNECT.
            is_success && method == M::CONNECT ||
            // All 1xx (informational), 204 (no content), and 304 (not modified) responses
            // MUST NOT include a message-body.
            is_informational ||
            matches!(status_code, 204 | 304);

        if has_no_body {
            return Ok(Self::LengthDelimited(0));
        }

        // https://datatracker.ietf.org/doc/html/rfc2616#section-4.3
        // All other responses do include a message-body, although it MAY be of zero length.

        let mut content_length: Option<u64> = None;
        let mut chunked = false;

        for head in headers {
            if compare_lowercase_ascii(head.name(), "content-length") {
                let v = str::from_utf8(head.value_raw())?.parse::<u64>()?;
                if content_length.is_some() {
                    return Err(HootError::DuplicateContentLength);
                }
                content_length = Some(v);
            } else if !chunked && compare_lowercase_ascii(head.name(), "transfer-encoding") {
                // Header can repeat, stop looking if we found "chunked"
                let s = str::from_utf8(head.value_raw())?;
                chunked = s
                    .split(",")
                    .map(|v| v.trim())
                    .any(|v| compare_lowercase_ascii(v, "chunked"));
            }
        }

        if chunked && !http10 {
            // https://datatracker.ietf.org/doc/html/rfc2616#section-4.4
            // Messages MUST NOT include both a Content-Length header field and a
            // non-identity transfer-coding. If the message does include a non-
            // identity transfer-coding, the Content-Length MUST be ignored.
            return Ok(Self::Chunked);
        }

        if let Some(len) = content_length {
            return Ok(Self::LengthDelimited(len));
        }

        Ok(Self::CloseDelimited)
    }
}

#[cfg(any(std, test))]
mod std_impls {
    use super::*;
    use std::fmt;

    impl fmt::Debug for Status<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_tuple("Status")
                .field(&self.0)
                .field(&self.1)
                .field(&self.2)
                .finish()
        }
    }

    impl fmt::Debug for RecvBodyMode {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::LengthDelimited(arg0) => {
                    f.debug_tuple("LengthDelimited").field(arg0).finish()
                }
                Self::Chunked => write!(f, "Chunked"),
                Self::CloseDelimited => write!(f, "CloseDelimited"),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_recv_no_headers() -> Result<()> {
        let mut buf = [0; 1024];
        let mut r: Response<RECV_RESPONSE> = Response::new_test();

        let a = r.try_read_response(b"HTTP/1.1 404\r\n\r\n", &mut buf)?;
        assert!(a.is_success());

        let status = a.status().unwrap();
        assert_eq!(status, &Status(HttpVersion::Http11, 404, ""));

        assert!(a.headers().unwrap().is_empty());
        Ok(())
    }
}
