use crate::from_req::{FromRequest, FromRequestRef};
use crate::{Request, Response};

pub trait Handler<T, S>: Clone + Send + Sized + 'static {
    fn call(self, state: S, request: Request) -> Response;
}

impl<S, F, Ret> Handler<(), S> for F
where
    F: FnOnce() -> Ret + Clone + Send + 'static,
    Ret: Into<Response>,
{
    fn call(self, _state: S, _request: Request) -> Response {
        (self)().into()
    }
}

impl<S, F, Ret> Handler<((),), S> for F
where
    F: FnOnce(S) -> Ret + Clone + Send + 'static,
    Ret: Into<Response>,
{
    fn call(self, state: S, _request: Request) -> Response {
        (self)(state).into()
    }
}

impl<S, F, T1, Ret> Handler<((), T1), S> for F
where
    F: FnOnce(T1) -> Ret + Clone + Send + 'static,
    T1: FromRequest<S>,
    Ret: Into<Response>,
{
    fn call(self, state: S, request: Request) -> Response {
        let t1 = match T1::from_request(&state, request) {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
        (self)(t1).into()
    }
}

impl<S, F, T1, Ret> Handler<(((),), T1), S> for F
where
    F: FnOnce(S, T1) -> Ret + Clone + Send + 'static,
    T1: FromRequest<S>,
    Ret: Into<Response>,
{
    fn call(self, state: S, request: Request) -> Response {
        let t1 = match T1::from_request(&state, request) {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
        (self)(state, t1).into()
    }
}

impl<S, F, T1, T2, Ret> Handler<((), T1, T2), S> for F
where
    F: FnOnce(T1, T2) -> Ret + Clone + Send + 'static,
    T1: FromRequestRef<S>,
    T2: FromRequest<S>,
    Ret: Into<Response>,
{
    fn call(self, state: S, request: Request) -> Response {
        let t1 = match T1::from_request(&state, &request) {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
        let t2 = match T2::from_request(&state, request) {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
        (self)(t1, t2).into()
    }
}

impl<S, F, T1, T2, Ret> Handler<(((),), T1, T2), S> for F
where
    F: FnOnce(S, T1, T2) -> Ret + Clone + Send + 'static,
    T1: FromRequestRef<S>,
    T2: FromRequest<S>,
    Ret: Into<Response>,
{
    fn call(self, state: S, request: Request) -> Response {
        let t1 = match T1::from_request(&state, &request) {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
        let t2 = match T2::from_request(&state, request) {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
        (self)(state, t1, t2).into()
    }
}
