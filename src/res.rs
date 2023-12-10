use core::marker::PhantomData;
use core::mem;

use httparse::Header;

use crate::model::{RecvBodyMode, Status};
use crate::parser::{parse_headers, parse_response_line};
use crate::req::CallState;
use crate::state::*;
use crate::vars::private::*;
use crate::{Result, ResumeToken};

pub struct Response<S: State> {
    _typ: PhantomData<S>,
    state: CallState,
}

impl Response<()> {
    pub fn resume(request: ResumeToken<ENDED, (), (), ()>) -> Response<RECV_RESPONSE> {
        Response {
            _typ: PhantomData,
            state: request.into_state(),
        }
    }
}

impl<S: State> Response<S> {
    fn transition<S2: State>(self) -> Response<S2> {
        // SAFETY: this only changes the type state of the PhantomData
        unsafe { mem::transmute(self) }
    }
}

pub struct ResponseAttempt<'a, 'b> {
    response: Response<RECV_RESPONSE>,
    success: bool,
    status: Option<Status<'a>>,
    headers: Option<&'b [Header<'a>]>,
}

impl<'a, 'b> ResponseAttempt<'a, 'b> {
    fn incomplete(response: Response<RECV_RESPONSE>) -> Self {
        ResponseAttempt {
            response,
            success: false,
            status: None,
            headers: None,
        }
    }

    pub fn is_success(&self) -> bool {
        self.success
    }

    pub fn status(&self) -> Option<&Status<'a>> {
        self.status.as_ref()
    }

    pub fn headers(&self) -> Option<&'b [Header<'a>]> {
        self.headers
    }

    pub fn next(self) -> AttemptNext {
        if !self.success {
            AttemptNext::Retry(self.response.transition())
        } else {
            match self.response.state.recv_body_mode.unwrap() {
                RecvBodyMode::LengthDelimited(v) if v == 0 => {
                    AttemptNext::NoBody(self.response.transition())
                }
                _ => AttemptNext::Body(self.response.transition()),
            }
        }
    }
}

pub enum AttemptNext {
    Retry(Response<RECV_RESPONSE>),
    Body(Response<RECV_BODY>),
    NoBody(Response<ENDED>),
}

impl AttemptNext {
    pub fn unwrap_retry(self) -> Response<RECV_RESPONSE> {
        let AttemptNext::Retry(r) = self else {
            panic!("unwrap_no_body when AttemptNext isnt Retry");
        };
        r
    }

    pub fn unwrap_body(self) -> Response<RECV_BODY> {
        let AttemptNext::Body(r) = self else {
            panic!("unwrap_no_body when AttemptNext isnt Body");
        };
        r
    }

    pub fn unwrap_no_body(self) -> Response<ENDED> {
        let AttemptNext::NoBody(r) = self else {
            panic!("unwrap_no_body when AttemptNext isnt NoBody");
        };
        r
    }
}

impl Response<RECV_RESPONSE> {
    pub fn try_read_response<'a, 'b>(
        mut self,
        input: &'a [u8],
        buf: &'b mut [u8],
    ) -> Result<ResponseAttempt<'a, 'b>> {
        let line = parse_response_line(input)?;
        if !line.complete {
            return Ok(ResponseAttempt::incomplete(self));
        }

        let status_offset = line.consumed;

        let headers = parse_headers(&input[status_offset..], buf)?;

        if !headers.complete {
            return Ok(ResponseAttempt::incomplete(self));
        }

        let is_http10 = self.state.is_head.unwrap();
        let is_head = self.state.is_head.unwrap();
        let mode = RecvBodyMode::from(is_http10, is_head, line.output.1, headers.output)?;
        self.state.recv_body_mode = Some(mode);

        Ok(ResponseAttempt {
            response: self,
            success: true,
            status: Some(line.output),
            headers: Some(headers.output),
        })
    }
}
