use core::fmt;
use core::marker::PhantomData;
use core::str;

use crate::body::{do_read_body, RecvBodyMode};
use crate::header::transmute_headers;
use crate::types::state::*;
use crate::types::*;
use crate::util::{cast_buf_for_headers, compare_lowercase_ascii, LengthChecker};
use crate::BodyPart;
use crate::{CallState, Result};
use crate::{Header, HootError, HttpVersion};

use super::ResumeToken;

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

    #[cfg(all(test, feature = "std"))]
    fn new_test() -> Response<RECV_RESPONSE> {
        use crate::Method as M;
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
        Response {
            _typ: PhantomData,
            state: self.state,
        }
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

        let lookup = |name: &str| {
            for header in &*headers {
                if compare_lowercase_ascii(header.name(), name) {
                    return Some(header.value());
                }
            }
            None
        };

        let mode = RecvBodyMode::for_response(http10, method, status.1, &lookup)?;
        self.state.recv_body_mode = Some(mode);

        // If we are awaiting a length, put a length checker in place
        if let RecvBodyMode::LengthDelimited(len) = mode {
            if len > 0 {
                self.state.recv_checker = Some(LengthChecker::new(len));
            }
        }

        Ok(ResponseAttempt {
            input_used: n,
            status: Some(status),
            headers: Some(headers),
        })
    }
}

pub struct ResponseAttempt<'a, 'b> {
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
            input_used: 0,
            status: None,
            headers: None,
        }
    }

    pub fn is_success(&self) -> bool {
        self.input_used > 0
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
            if !r.is_success() {
                return Ok(BodyPart::empty());
            }
        }

        do_read_body(&mut self.state, src, dst)
    }

    pub fn is_finished(&self) -> bool {
        use RecvBodyMode::*;

        let mode = match self.state.recv_body_mode {
            Some(v) => v,
            None => return false,
        };

        match mode {
            LengthDelimited(n) => n == 0 || self.state.did_read_to_end,
            Chunked => self.state.did_read_to_end,
            CloseDelimited => unreachable!("CloseDelimited is not possible for server::Request"),
        }
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

impl fmt::Debug for Status<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Status")
            .field(&self.0)
            .field(&self.1)
            .field(&self.2)
            .finish()
    }
}

#[cfg(all(test, feature = "std"))]
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
