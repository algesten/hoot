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

macro_rules! impl_handler {
    (
        [$($ty:ident),*], $last:ident
    ) => {
        #[allow(non_snake_case)]
        impl<S, F, $($ty,)* $last, Ret> Handler<((), $($ty,)* $last), S> for F
        where
            F: FnOnce($($ty,)* $last) -> Ret + Clone + Send + 'static,
            Ret: Into<Response>,
            $( $ty: FromRequestRef<S> + Send, )*
            $last: FromRequest<S> + Send,
        {
            fn call(self, state: S, request: Request) -> Response {

                $(
                    let $ty = match <$ty>::from_request(&state, &request) {
                        Ok(v) => v,
                        Err(e) => return e.into(),
                    };
                )*

                let $last = match $last::from_request(&state, request) {
                    Ok(v) => v,
                    Err(e) => return e.into(),
                };

                (self)($($ty,)* $last).into()
            }
        }
        #[allow(non_snake_case)]
        impl<S, F, $($ty,)* $last, Ret> Handler<(((),), $($ty,)* $last), S> for F
        where
            F: FnOnce(S, $($ty,)* $last) -> Ret + Clone + Send + 'static,
            Ret: Into<Response>,
            $( $ty: FromRequestRef<S> + Send, )*
            $last: FromRequest<S> + Send,
        {
            fn call(self, state: S, request: Request) -> Response {

                $(
                    let $ty = match <$ty>::from_request(&state, &request) {
                        Ok(v) => v,
                        Err(e) => return e.into(),
                    };
                )*

                let $last = match $last::from_request(&state, request) {
                    Ok(v) => v,
                    Err(e) => return e.into(),
                };

                (self)(state, $($ty,)* $last).into()
            }
        }
    }
}

impl_handler! { [T1], T2 }
impl_handler! { [T1, T2], T3 }
impl_handler! { [T1, T2, T3], T4 }
impl_handler! { [T1, T2, T3, T4], T5 }
impl_handler! { [T1, T2, T3, T4, T5], T6 }
impl_handler! { [T1, T2, T3, T4, T5, T6], T7 }
impl_handler! { [T1, T2, T3, T4, T5, T6, T7], T8 }
impl_handler! { [T1, T2, T3, T4, T5, T6, T7, T8], T9 }
impl_handler! { [T1, T2, T3, T4, T5, T6, T7, T9, T10], T11 }
impl_handler! { [T1, T2, T3, T4, T5, T6, T7, T9, T10, T11], T12 }
impl_handler! { [T1, T2, T3, T4, T5, T6, T7, T9, T10, T11, T12], T13 }
impl_handler! { [T1, T2, T3, T4, T5, T6, T7, T9, T10, T11, T12, T13], T14 }
impl_handler! { [T1, T2, T3, T4, T5, T6, T7, T9, T10, T11, T12, T13, T14], T15 }
impl_handler! { [T1, T2, T3, T4, T5, T6, T7, T9, T10, T11, T12, T13, T14, T15], T16 }
