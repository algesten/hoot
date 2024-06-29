use http::Request;

use crate::analyze::MethodExt;
use crate::Error;

use super::amended::AmendedRequest;
use super::{Call, RecvBody, RecvResponse, WithBody, WithoutBody};

/// Holder of [`Call`] regardless of type state
///
/// TODO(martin): is it weird to type state and then erase it?
pub(crate) enum CallHolder<'a> {
    WithoutBody(Call<'a, WithoutBody>),
    WithBody(Call<'a, WithBody>),
    RecvResponse(Call<'a, RecvResponse>),
    RecvBody(Call<'a, RecvBody>),
}

impl<'a> CallHolder<'a> {
    pub fn new(request: &'a Request<()>) -> Result<Self, Error> {
        Ok(if request.method().need_request_body() {
            CallHolder::WithBody(Call::with_body(request)?)
        } else {
            CallHolder::WithoutBody(Call::without_body(request)?)
        })
    }

    pub fn amended(&self) -> &AmendedRequest<'a> {
        match self {
            CallHolder::WithoutBody(v) => v.amended(),
            CallHolder::WithBody(v) => v.amended(),
            CallHolder::RecvResponse(v) => v.amended(),
            CallHolder::RecvBody(v) => v.amended(),
        }
    }

    pub fn amended_mut(&mut self) -> &mut AmendedRequest<'a> {
        match self {
            CallHolder::WithoutBody(v) => v.amended_mut(),
            CallHolder::WithBody(v) => v.amended_mut(),
            CallHolder::RecvResponse(v) => v.amended_mut(),
            CallHolder::RecvBody(v) => v.amended_mut(),
        }
    }
}
