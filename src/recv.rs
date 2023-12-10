use crate::model::Status;
use crate::parser::{parse_response_line, ParseResult};
use crate::vars::private;
use crate::Result;
use crate::{Call, HootError};

use crate::state::*;
use private::*;

pub enum Attempt<C1, C2, T> {
    Failure {
        call: C1,
    },
    Success {
        call: C2,
        consumed: usize,
        output: T,
    },
}

impl<C1, C2, T> Attempt<C1, C2, T> {
    pub fn is_failure(&self) -> bool {
        matches!(self, Attempt::Failure { .. })
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Attempt::Success { .. })
    }

    pub fn consumed(&self) -> usize {
        let Self::Success { consumed, .. } = self else {
            return 0;
        };
        *consumed
    }

    pub fn output(&self) -> Option<&T> {
        let Self::Success { output, .. } = self else {
            return None;
        };
        Some(output)
    }

    pub fn revert(self) -> Option<C1> {
        let Self::Failure { call } = self else {
            return None;
        };
        Some(call)
    }

    pub fn complete(self) -> Option<C2> {
        let Self::Success { call, .. } = self else {
            return None;
        };
        Some(call)
    }
}

type ParseStatus<'a, V, M> = Attempt<
    // Incoming state if Incomplete parse.
    Call<'a, RECV_STATUS, V, M, ()>,
    // Outgoing state if complete parse.
    Call<'a, RECV_HEADERS, (), M, ()>,
    // The parsed data
    Status<'a>,
>;

impl<'a, V: Version, M: Method> Call<'a, RECV_STATUS, V, M, ()> {
    pub fn try_read_status<'b: 'a>(self, buf: &'b [u8]) -> Result<ParseStatus<'a, V, M>> {
        let ParseResult {
            complete,
            consumed,
            output,
        } = parse_response_line(buf)?;

        let result = if complete {
            // Check server responded as we expect
            if output.0 != V::version() {
                return Err(HootError::HttpVersionMismatch);
            }

            Attempt::Success {
                call: self.transition(),
                consumed,
                output,
            }
        } else {
            Attempt::Failure { call: self }
        };

        Ok(result)
    }
}

impl<'a, M: Method> Call<'a, RECV_HEADERS, (), M, ()> {}

// https://datatracker.ietf.org/doc/html/rfc2616#section-4.4
// Messages MUST NOT include both a Content-Length header field and a
// non-identity transfer-coding. If the message does include a non-
// identity transfer-coding, the Content-Length MUST be ignored.
