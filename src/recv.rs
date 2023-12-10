use crate::model::{RecvBodyMode, Status};
use crate::parser::{parse_headers, parse_response_line, ParseResult};
use crate::vars::private;
use crate::{Call, HootError};
use crate::{HttpVersion, Result};

use crate::state::*;
use httparse::Header;
use private::*;

pub trait Attempt<C1, C2> {
    type Output: ?Sized;
    fn is_success(&self) -> bool;
    fn consumed(&self) -> usize;
    fn output(&self) -> Option<&Self::Output>;
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
        Call<'a, RECV_HEADERS, V, M, ()>,
    > for AttemptStatus<'a, 'b, V, M>
{
    type Output = Status<'b>;

    fn is_success(&self) -> bool {
        self.success
    }

    fn consumed(&self) -> usize {
        self.consumed
    }

    fn output(&self) -> Option<&Self::Output> {
        self.success.then_some(&self.output)
    }

    fn proceed(
        self,
    ) -> MaybeNext<Call<'a, RECV_STATUS, V, M, ()>, Call<'a, RECV_HEADERS, V, M, ()>> {
        if self.success {
            MaybeNext::Next(self.call.transition())
        } else {
            MaybeNext::Stay(self.call)
        }
    }
}

impl<'a, V: Version, M: Method> Call<'a, RECV_STATUS, V, M, ()> {
    pub fn try_read_status<'b>(mut self, buf: &'b [u8]) -> Result<AttemptStatus<'a, 'b, V, M>> {
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

            assert!(self.state.status_code.is_none());
            self.state.status_code = Some(output.1);
        }

        Ok(AttemptStatus {
            call: self,
            success: complete,
            consumed,
            output,
        })
    }
}

pub struct AttemptHeaders<'a, 'b, V: Version, M: Method> {
    call: Call<'a, RECV_HEADERS, V, M, ()>,
    success: bool,
    consumed: usize,
    output_ptr: *const Header<'b>,
    output_len: usize,
}

impl<'a, 'b: 'a, V: Version, M: Method>
    Attempt<
        // Starting state.
        Call<'a, RECV_HEADERS, V, M, ()>,
        // Transitioned state.
        MaybeBody<'a>,
    > for AttemptHeaders<'a, 'b, V, M>
{
    type Output = [Header<'b>];

    fn is_success(&self) -> bool {
        self.success
    }

    fn consumed(&self) -> usize {
        self.consumed
    }

    fn output(&self) -> Option<&Self::Output> {
        // SAFETY: The header_ptr points to array inside Call.Out buffer with lifetime 'a and each
        // Header points to input captured by lifetime 'b. AttemptHeaders does not have any mutations,
        // whicn means the pointers are correct as long as this struct is alive.
        let output = unsafe { core::slice::from_raw_parts(self.output_ptr, self.output_len) };

        Some(output)
    }

    fn proceed(self) -> MaybeNext<Call<'a, RECV_HEADERS, V, M, ()>, MaybeBody<'a>> {
        if self.success {
            let recv_body_mode = self.call.state.recv_body_mode.expect("body mode to be set");

            let maybe_body = match recv_body_mode {
                // We do not expect a body after the headers
                RecvBodyMode::LengthDelimited(len) if len == 0 => {
                    MaybeBody::NoBody(self.call.transition())
                }
                // There will be some body.
                _ => MaybeBody::HasBody(self.call.transition()),
            };
            MaybeNext::Next(maybe_body)
        } else {
            MaybeNext::Stay(self.call)
        }
    }
}

pub enum MaybeBody<'a> {
    HasBody(Call<'a, RECV_BODY, (), (), ()>),
    NoBody(Call<'a, ENDED, (), (), ()>),
}

impl<'a, V: Version, M: Method> Call<'a, RECV_HEADERS, V, M, ()> {
    pub fn try_read_headers<'b: 'a>(
        mut self,
        buf: &'b [u8],
    ) -> Result<AttemptHeaders<'a, 'b, V, M>> {
        // Borrow the remaining part of the buffer. This is probably the entire buffer since
        // the user would have flushed the request before parsing a response.
        let dst = self.out.borrow_remaining();

        let parse = parse_headers(buf, dst)?;

        if parse.complete {
            assert!(self.state.recv_body_mode.is_none());

            let is_http10 = V::version() == HttpVersion::Http10;
            let is_head = M::is_head();
            // Since we (successfully) read the response line, we must have a status.
            let status_code = self.state.status_code.expect("status code");

            let mode = RecvBodyMode::from(is_http10, is_head, status_code, parse.output)?;
            self.state.recv_body_mode = Some(mode);
        }

        Ok(AttemptHeaders {
            success: parse.complete,
            consumed: parse.consumed,
            output_len: parse.output.len(),
            output_ptr: parse.output.as_ptr(),
            call: self,
        })
    }
}

impl<'a> Call<'a, RECV_BODY, (), (), ()> {
    //
}
