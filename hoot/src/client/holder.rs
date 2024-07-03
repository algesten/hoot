use http::Request;

use crate::ext::MethodExt;
use crate::Error;

use super::amended::AmendedRequest;
use super::call::state::{RecvBody, RecvResponse, WithBody, WithoutBody};
use super::Call;

/// Holder of [`Call`] regardless of type state
///
/// TODO(martin): is it weird to type state and then erase it?
#[derive(Debug)]
pub(crate) enum CallHolder<'a, B> {
    WithoutBody(Call<'a, WithoutBody, B>),
    WithBody(Call<'a, WithBody, B>),
    RecvResponse(Call<'a, RecvResponse, B>),
    RecvBody(Call<'a, RecvBody, B>),
}

impl<'a, B> CallHolder<'a, B> {
    pub fn new(request: &'a Request<B>) -> Result<Self, Error> {
        Ok(if request.method().need_request_body() {
            CallHolder::WithBody(Call::with_body(request)?)
        } else {
            CallHolder::WithoutBody(Call::without_body(request)?)
        })
    }

    pub fn request(&self) -> &AmendedRequest<'a, B> {
        match self {
            CallHolder::WithoutBody(v) => v.amended(),
            CallHolder::WithBody(v) => v.amended(),
            CallHolder::RecvResponse(v) => v.amended(),
            CallHolder::RecvBody(v) => v.amended(),
        }
    }

    pub fn request_mut(&mut self) -> &mut AmendedRequest<'a, B> {
        match self {
            CallHolder::WithoutBody(v) => v.amended_mut(),
            CallHolder::WithBody(v) => v.amended_mut(),
            CallHolder::RecvResponse(v) => v.amended_mut(),
            CallHolder::RecvBody(v) => v.amended_mut(),
        }
    }

    pub fn as_with_body(&self) -> &Call<'a, WithBody, B> {
        match self {
            CallHolder::WithBody(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_with_body_mut(&mut self) -> &mut Call<'a, WithBody, B> {
        match self {
            CallHolder::WithBody(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_recv_response(&self) -> &Call<'a, RecvResponse, B> {
        match self {
            CallHolder::RecvResponse(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_recv_response_mut(&mut self) -> &mut Call<'a, RecvResponse, B> {
        match self {
            CallHolder::RecvResponse(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_recv_body(&self) -> &Call<'a, RecvBody, B> {
        match self {
            CallHolder::RecvBody(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_recv_body_mut(&mut self) -> &mut Call<'a, RecvBody, B> {
        match self {
            CallHolder::RecvBody(v) => v,
            _ => unreachable!(),
        }
    }
}
