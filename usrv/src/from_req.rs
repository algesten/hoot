use std::convert::Infallible;

use crate::{IntoResponse, Request, Response};

pub trait FromRequest<S>: Sized {
    type Rejection: IntoResponse;
    fn from_request(state: &S, request: Request) -> Result<Self, Self::Rejection>;
}

impl<S> FromRequest<S> for Request {
    type Rejection = Infallible;

    fn from_request(_state: &S, request: Request) -> Result<Self, Self::Rejection> {
        Ok(request)
    }
}

pub trait FromRequestRef<S>: Sized {
    type Rejection: Into<Response>;
    fn from_request<'a, 's>(state: &'s S, request: &'a Request) -> Result<Self, Self::Rejection>;
}
