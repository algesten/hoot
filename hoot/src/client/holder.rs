use http::Request;

use crate::ext::MethodExt;
use crate::Error;

use super::amended::AmendedRequest;
use super::call::state::{RecvBody, RecvResponse, WithBody, WithoutBody};
use super::call::Call;

/// Holder of [`Call`] regardless of type state
///
/// TODO(martin): is it weird to type state and then erase it?
#[derive(Debug)]
pub(crate) enum CallHolder<B> {
    WithoutBody(Call<WithoutBody, B>),
    WithBody(Call<WithBody, B>),
    RecvResponse(Call<RecvResponse, B>),
    RecvBody(Call<RecvBody, B>),
}

impl<B> CallHolder<B> {
    pub fn new(request: Request<B>) -> Result<Self, Error> {
        Ok(if request.method().need_request_body() {
            CallHolder::WithBody(Call::with_body(request)?)
        } else {
            CallHolder::WithoutBody(Call::without_body(request)?)
        })
    }

    pub fn request(&self) -> &AmendedRequest<B> {
        match self {
            CallHolder::WithoutBody(v) => v.amended(),
            CallHolder::WithBody(v) => v.amended(),
            CallHolder::RecvResponse(v) => v.amended(),
            CallHolder::RecvBody(v) => v.amended(),
        }
    }

    pub fn request_mut(&mut self) -> &mut AmendedRequest<B> {
        match self {
            CallHolder::WithoutBody(v) => v.amended_mut(),
            CallHolder::WithBody(v) => v.amended_mut(),
            CallHolder::RecvResponse(v) => v.amended_mut(),
            CallHolder::RecvBody(v) => v.amended_mut(),
        }
    }

    pub fn as_with_body(&self) -> &Call<WithBody, B> {
        match self {
            CallHolder::WithBody(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_with_body_mut(&mut self) -> &mut Call<WithBody, B> {
        match self {
            CallHolder::WithBody(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_recv_response(&self) -> &Call<RecvResponse, B> {
        match self {
            CallHolder::RecvResponse(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_recv_response_mut(&mut self) -> &mut Call<RecvResponse, B> {
        match self {
            CallHolder::RecvResponse(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_recv_body(&self) -> &Call<RecvBody, B> {
        match self {
            CallHolder::RecvBody(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_recv_body_mut(&mut self) -> &mut Call<RecvBody, B> {
        match self {
            CallHolder::RecvBody(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn analyze_request(&mut self) -> Result<(), Error> {
        match self {
            CallHolder::WithoutBody(v) => v.analyze_request(),
            CallHolder::WithBody(v) => v.analyze_request(),
            CallHolder::RecvResponse(v) => v.analyze_request(),
            CallHolder::RecvBody(v) => v.analyze_request(),
        }
    }
}
