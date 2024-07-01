use http::Request;

use crate::ext::MethodExt;
use crate::Error;

use super::amended::AmendedRequest;
use super::call::state::{RecvBody, RecvResponse, WithBody, WithoutBody};
use super::Call;

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

    pub fn request(&self) -> &AmendedRequest<'a> {
        match self {
            CallHolder::WithoutBody(v) => v.amended(),
            CallHolder::WithBody(v) => v.amended(),
            CallHolder::RecvResponse(v) => v.amended(),
            CallHolder::RecvBody(v) => v.amended(),
        }
    }

    pub fn request_mut(&mut self) -> &mut AmendedRequest<'a> {
        match self {
            CallHolder::WithoutBody(v) => v.amended_mut(),
            CallHolder::WithBody(v) => v.amended_mut(),
            CallHolder::RecvResponse(v) => v.amended_mut(),
            CallHolder::RecvBody(v) => v.amended_mut(),
        }
    }

    pub fn as_with_body(&self) -> &Call<'a, WithBody> {
        match self {
            CallHolder::WithBody(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_with_body_mut(&mut self) -> &mut Call<'a, WithBody> {
        match self {
            CallHolder::WithBody(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_recv_response(&self) -> &Call<'a, RecvResponse> {
        match self {
            CallHolder::RecvResponse(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_recv_response_mut(&mut self) -> &mut Call<'a, RecvResponse> {
        match self {
            CallHolder::RecvResponse(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_recv_body(&self) -> &Call<'a, RecvBody> {
        match self {
            CallHolder::RecvBody(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_recv_body_mut(&mut self) -> &mut Call<'a, RecvBody> {
        match self {
            CallHolder::RecvBody(v) => v,
            _ => unreachable!(),
        }
    }
}
