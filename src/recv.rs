use crate::model::Status;
use crate::parser::{parse_headers, parse_response_line, ParseResult};
use crate::vars::private;
use crate::Result;
use crate::{Call, HootError};

use crate::state::*;
use httparse::Header;
use private::*;

pub trait Attempt<C1, C2> {
    type Output: ?Sized;
    fn is_success(&self) -> bool;
    fn consumed(&self) -> usize;
    fn output(&mut self) -> Option<&Self::Output>;
    fn proceed(self) -> MaybeNext<C1, C2>;
}

pub enum MaybeNext<C1, C2> {
    Stay(C1),
    Next(C2),
}

impl<C1, C2> MaybeNext<C1, C2> {
    pub fn unwrap_stay(self) -> C1 {
        let MaybeNext::Stay(c) = self else {
            panic!("unwrap_stay on a MaybeNext::Next");
        };
        c
    }

    pub fn unwrap_next(self) -> C2 {
        let MaybeNext::Next(c) = self else {
            panic!("unwrap_next on a MaybeNext::Stay");
        };
        c
    }
}

pub struct AttemptStatus<'a, 'b, V: Version, M: Method> {
    call: Call<'a, RECV_STATUS, V, M, ()>,
    success: bool,
    consumed: usize,
    output: Status<'b>,
}

impl<'a, 'b: 'a, V: Version, M: Method>
    Attempt<
        // Starting state.
        Call<'a, RECV_STATUS, V, M, ()>,
        // Transitioned state.
        Call<'a, RECV_HEADERS, (), M, ()>,
    > for AttemptStatus<'a, 'b, V, M>
{
    type Output = Status<'b>;

    fn is_success(&self) -> bool {
        self.success
    }

    fn consumed(&self) -> usize {
        self.consumed
    }

    fn output(&mut self) -> Option<&Self::Output> {
        self.success.then_some(&self.output)
    }

    fn proceed(
        self,
    ) -> MaybeNext<Call<'a, RECV_STATUS, V, M, ()>, Call<'a, RECV_HEADERS, (), M, ()>> {
        if self.success {
            MaybeNext::Next(self.call.transition())
        } else {
            MaybeNext::Stay(self.call)
        }
    }
}

impl<'a, V: Version, M: Method> Call<'a, RECV_STATUS, V, M, ()> {
    pub fn try_read_status<'b>(self, buf: &'b [u8]) -> Result<AttemptStatus<'a, 'b, V, M>> {
        let ParseResult {
            complete,
            consumed,
            output,
        } = parse_response_line(buf)?;

        if complete {
            // Check server responded as we expect
            if output.0 != V::version() {
                return Err(HootError::HttpVersionMismatch);
            }
        }

        Ok(AttemptStatus {
            call: self,
            success: complete,
            consumed,
            output,
        })
    }
}

pub struct AttemptHeaders<'a, 'b, M: Method> {
    call: Call<'a, RECV_HEADERS, (), M, ()>,
    success: bool,
    consumed: usize,
    output_ptr: *const Header<'b>,
    output_len: usize,
}

impl<'a, 'b: 'a, M: Method>
    Attempt<
        // Starting state.
        Call<'a, RECV_HEADERS, (), M, ()>,
        // Transitioned state.
        Call<'a, RECV_BODY, (), M, ()>,
    > for AttemptHeaders<'a, 'b, M>
{
    type Output = [Header<'b>];

    fn is_success(&self) -> bool {
        self.success
    }

    fn consumed(&self) -> usize {
        self.consumed
    }

    fn output(&mut self) -> Option<&Self::Output> {
        // SAFETY: The header_ptr points into the buffer captured by lifetime 'b. As long as
        // lifetime 'b is valid, the pointer is valid.
        let output = unsafe { core::slice::from_raw_parts(self.output_ptr, self.output_len) };

        Some(output)
    }

    fn proceed(
        self,
    ) -> MaybeNext<Call<'a, RECV_HEADERS, (), M, ()>, Call<'a, RECV_BODY, (), M, ()>> {
        if self.success {
            MaybeNext::Next(self.call.transition())
        } else {
            MaybeNext::Stay(self.call)
        }
    }
}

impl<'a, M: Method> Call<'a, RECV_HEADERS, (), M, ()> {
    pub fn try_read_headers<'b: 'a>(mut self, buf: &'b [u8]) -> Result<AttemptHeaders<'a, 'b, M>> {
        // Borrow the remaining part of the buffer. This is probably the entire buffer since
        // the user would have flushed the request before parsing a response.
        let dst = self.out.borrow_remaining();

        let parse = parse_headers(buf, dst)?;

        Ok(AttemptHeaders {
            success: parse.complete,
            consumed: parse.consumed,
            output_len: parse.output.len(),
            output_ptr: parse.output.as_ptr(),
            call: self,
        })
    }
}

// https://datatracker.ietf.org/doc/html/rfc2616#section-4.4
// Messages MUST NOT include both a Content-Length header field and a
// non-identity transfer-coding. If the message does include a non-
// identity transfer-coding, the Content-Length MUST be ignored.
