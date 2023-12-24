use core::marker::PhantomData;
use core::mem;

use crate::body::{do_read_body, RecvBodyMode};
use crate::error::Result;
use crate::header::transmute_headers;
use crate::util::{cast_buf_for_headers, LengthChecker};
use crate::vars::private::*;
use crate::vars::state::*;
use crate::{BodyPart, CallState};
use crate::{Header, HootError, HttpVersion, Method};

pub struct Request<S: State> {
    typ: PhantomData<S>,
    state: CallState,
}

impl<S: State> Request<S> {
    fn transition<S2: State>(self) -> Request<S2> {
        // SAFETY: this only changes the type state of the PhantomData
        unsafe { mem::transmute(self) }
    }

    fn do_try_read_request<'a, 'b>(
        &mut self,
        input: &'a [u8],
        buf: &'b mut [u8],
    ) -> Result<RequestAttempt<'a, 'b>> {
        let already_read_request = self.state.recv_body_mode.is_some();

        // Request/headers reads only work once.
        if already_read_request {
            return Ok(RequestAttempt::empty());
        }

        let headers = cast_buf_for_headers(buf);
        let mut r = httparse::Request::new(headers);

        let input_used = match r.parse(input)? {
            httparse::Status::Complete(v) => v,
            httparse::Status::Partial => return Ok(RequestAttempt::empty()),
        };

        let method: Method = r.method.unwrap().try_into()?;

        let path = r.path.unwrap();

        let ver = match r.version.unwrap() {
            0 => HttpVersion::Http10,
            1 => HttpVersion::Http11,
            _ => return Err(HootError::Version),
        };

        let line = Line(method, path, ver);

        // Derive body mode from knowledge this far.
        let http10 = ver == HttpVersion::Http10;
        let method = self.state.method.unwrap(); // Ok for same reason as above.
        let headers = transmute_headers(r.headers);
        let mode = RecvBodyMode::for_request(http10, method, headers)?;

        // If we are awaiting a length, put a length checker in place
        if let RecvBodyMode::LengthDelimited(len) = mode {
            if len > 0 {
                self.state.recv_checker = Some(LengthChecker::new(len));
            }
        }

        // Remember the body mode
        self.state.recv_body_mode = Some(mode);

        Ok(RequestAttempt {
            input_used,
            line: Some(line),
            headers: Some(headers),
        })
    }
}

pub struct RequestAttempt<'a, 'b> {
    input_used: usize,
    line: Option<Line<'a>>,
    headers: Option<&'b [Header<'a>]>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Line<'a>(Method, &'a str, HttpVersion);

impl Line<'_> {
    pub fn method(&self) -> Method {
        self.0
    }

    pub fn path(&self) -> &str {
        self.1
    }

    pub fn version(&self) -> HttpVersion {
        self.2
    }
}

impl<'a, 'b> RequestAttempt<'a, 'b> {
    const fn empty() -> Self {
        RequestAttempt {
            input_used: 0,
            line: None,
            headers: None,
        }
    }

    pub fn is_success(&self) -> bool {
        self.input_used > 0
    }

    pub fn input_used(&self) -> usize {
        self.input_used
    }

    pub fn line(&self) -> Option<&Line<'a>> {
        self.line.as_ref()
    }

    pub fn headers(&self) -> Option<&'b [Header<'a>]> {
        self.headers
    }
}

impl Request<RECV_REQUEST> {
    pub fn try_read_request<'a, 'b>(
        &mut self,
        input: &'a [u8],
        buf: &'b mut [u8],
    ) -> Result<RequestAttempt<'a, 'b>> {
        self.do_try_read_request(input, buf)
    }

    pub fn proceed(self) -> Request<RECV_BODY> {
        self.transition()
    }
}

impl Request<RECV_BODY> {
    pub fn read_body<'a, 'b>(&mut self, src: &'a [u8], dst: &'b mut [u8]) -> Result<BodyPart<'b>> {
        let already_read_response = self.state.recv_body_mode.is_some();

        // It's valid to skip try_read_response() and progress straight to reading
        // the body. This ensures we skip the corresponding input.
        if !already_read_response {
            let r = self.do_try_read_request(src, dst)?;

            // Still not enough input for the entire status and headers. Need
            // to try again later.
            if !r.is_success() {
                return Ok(BodyPart::empty());
            }
        }

        do_read_body(&mut self.state, src, dst)
    }

    pub fn is_finished(&self) -> bool {
        self.state.did_read_to_end
    }

    // pub fn into_response(self) -> Result<Response> {
    //     if let Some(checker) = &self.state.recv_checker {
    //         checker.assert_expected(HootError::RecvLessThanContentLength)?;
    //     }

    //     if !self.is_finished() {
    //         return Err(HootError::BodyNotFinished);
    //     }

    //     Ok(self.transition())
    // }
}
