#![allow(private_bounds)]

use crate::HttpVersion;

trait Private {}
pub trait State: Private {}

pub trait Version: Private {
    #[doc(hidden)]
    fn version() -> HttpVersion
    where
        Self: Sized;
}

pub trait Method: Private {
    #[doc(hidden)]
    fn is_head() -> bool
    where
        Self: Sized,
    {
        false
    }

    #[doc(hidden)]
    fn is_connect() -> bool
    where
        Self: Sized,
    {
        false
    }
}

pub trait MethodWithRequestBody: Method {}
pub trait MethodWithoutRequestBody: Method {}
pub trait MethodWithResponseBody: Method {}
pub trait MethodWithoutResponseBody: Method {}

pub trait BodyType: Private {}

impl Private for () {}

macro_rules! impl_private {
    ($trait:ty, $target:ty) => {
        impl crate::types::Private for $target {}
        impl $trait for $target {}
    };
}

#[allow(non_camel_case_types)]
pub mod state {
    use super::State;

    pub struct INIT;
    pub struct SEND_LINE;
    pub struct SEND_STATUS;
    pub struct SEND_HEADERS;
    pub struct SEND_BODY;
    pub struct SEND_TRAILER;
    pub struct RECV_REQUEST;
    pub struct RECV_RESPONSE;
    pub struct RECV_BODY;
    pub struct RECV_TRAILERS;
    pub struct ENDED;

    impl State for () {}

    impl_private!(State, INIT);
    impl_private!(State, SEND_LINE);
    impl_private!(State, SEND_STATUS);
    impl_private!(State, SEND_HEADERS);
    impl_private!(State, SEND_BODY);
    impl_private!(State, SEND_TRAILER);
    impl_private!(State, RECV_RESPONSE);
    impl_private!(State, RECV_REQUEST);
    impl_private!(State, RECV_BODY);
    impl_private!(State, RECV_TRAILERS);
    impl_private!(State, ENDED);
}

#[allow(non_camel_case_types)]
pub mod version {
    use super::Version;
    use crate::HttpVersion;

    pub struct HTTP_10;
    pub struct HTTP_11;

    impl Version for () {
        fn version() -> HttpVersion {
            // Calling .version() on a () is a bug.
            unreachable!()
        }
    }

    impl super::Private for HTTP_10 {}
    impl Version for HTTP_10 {
        fn version() -> HttpVersion {
            HttpVersion::Http10
        }
    }

    impl super::Private for HTTP_11 {}
    impl Version for HTTP_11 {
        fn version() -> HttpVersion {
            HttpVersion::Http11
        }
    }
}

#[allow(non_camel_case_types)]
pub mod method {
    use super::{Method, MethodWithRequestBody, MethodWithResponseBody};
    use super::{MethodWithoutRequestBody, MethodWithoutResponseBody};

    pub struct OPTIONS;
    pub struct GET;
    pub struct POST;
    pub struct PUT;
    pub struct DELETE;
    pub struct HEAD;
    pub struct TRACE;
    pub struct CONNECT;
    pub struct PATCH;

    impl Method for () {}
    impl_private!(Method, OPTIONS);
    impl_private!(Method, GET);
    impl_private!(Method, POST);
    impl_private!(Method, PUT);
    impl_private!(Method, DELETE);
    impl_private!(Method, TRACE);
    impl_private!(Method, PATCH);

    impl super::Private for HEAD {}
    impl Method for HEAD {
        fn is_head() -> bool {
            true
        }
    }

    impl super::Private for CONNECT {}
    impl Method for CONNECT {
        fn is_connect() -> bool {
            true
        }
    }

    impl MethodWithRequestBody for POST {}
    impl MethodWithRequestBody for PUT {}
    impl MethodWithRequestBody for PATCH {}

    impl MethodWithoutRequestBody for OPTIONS {}
    impl MethodWithoutRequestBody for GET {}
    impl MethodWithoutRequestBody for DELETE {}
    impl MethodWithoutRequestBody for HEAD {}
    impl MethodWithoutRequestBody for CONNECT {}
    impl MethodWithoutRequestBody for TRACE {}

    impl MethodWithResponseBody for OPTIONS {}
    impl MethodWithResponseBody for GET {}
    impl MethodWithResponseBody for POST {}
    impl MethodWithResponseBody for PUT {}
    impl MethodWithResponseBody for DELETE {}
    impl MethodWithResponseBody for TRACE {}
    impl MethodWithResponseBody for PATCH {}

    impl MethodWithoutResponseBody for HEAD {}
    impl MethodWithoutResponseBody for CONNECT {}
}

#[allow(non_camel_case_types)]
pub mod body {
    use super::BodyType;

    pub struct BODY_LENGTH;
    pub struct BODY_CHUNKED;

    impl BodyType for () {}
    impl_private!(BodyType, BODY_LENGTH);
    impl_private!(BodyType, BODY_CHUNKED);
}
