use core::marker::PhantomData;

use crate::out::Out;
use crate::vars::body::*;
use crate::vars::method::*;
use crate::vars::private::*;
use crate::vars::state::*;
use crate::vars::version::*;
use crate::CallState;

#[non_exhaustive]
pub enum ResponseVariant<'a> {
    Http10Get(Response<'a, INIT, HTTP_10, GET, ()>),
    Http10Head(Response<'a, INIT, HTTP_10, HEAD, ()>),
    Http10Post(Response<'a, INIT, HTTP_10, POST, ()>),

    Http11Get(Response<'a, INIT, HTTP_11, GET, ()>),
    Http11Head(Response<'a, INIT, HTTP_11, HEAD, ()>),
    Http11Post(Response<'a, INIT, HTTP_11, POST, ()>),
    Http11Put(Response<'a, INIT, HTTP_11, PUT, ()>),
    Http11Delete(Response<'a, INIT, HTTP_11, DELETE, ()>),
    Http11Connect(Response<'a, INIT, HTTP_11, CONNECT, ()>),
    Http11Options(Response<'a, INIT, HTTP_11, OPTIONS, ()>),
    Http11Trace(Response<'a, INIT, HTTP_11, TRACE, ()>),
    Http11Patch(Response<'a, INIT, HTTP_11, PATCH, ()>),

    #[doc(hidden)]
    Ph(&'a ()),
}

pub struct Response<'a, S: State, V: Version, M: Method, B: BodyType> {
    typ: Typ<S, V, M, B>,
    state: CallState,
    out: Out<'a>,
}

/// Zero sized struct only to hold type state.
#[derive(Default)]
struct Typ<S: State, V: Version, M: Method, B: BodyType>(
    PhantomData<S>,
    PhantomData<V>,
    PhantomData<M>,
    PhantomData<B>,
);
