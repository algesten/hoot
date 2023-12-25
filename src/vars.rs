#[allow(non_camel_case_types)]
pub mod state {
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
}

#[allow(non_camel_case_types)]
pub mod version {
    pub struct HTTP_10;
    pub struct HTTP_11;
}

#[allow(non_camel_case_types)]
pub mod method {
    pub struct OPTIONS;
    pub struct GET;
    pub struct POST;
    pub struct PUT;
    pub struct DELETE;
    pub struct HEAD;
    pub struct TRACE;
    pub struct CONNECT;
    pub struct PATCH;
}

#[allow(non_camel_case_types)]
pub mod body {
    pub struct BODY_LENGTH;
    pub struct BODY_CHUNKED;
}

pub(crate) mod private {
    use crate::HttpVersion;

    use super::body::*;
    use super::method::*;
    use super::state::*;
    use super::version::*;

    pub trait State {}
    pub trait Version {
        fn version() -> HttpVersion;
    }
    pub trait Method {
        fn is_head() -> bool {
            false
        }
        fn is_connect() -> bool {
            false
        }
    }

    impl State for () {}
    impl State for INIT {}
    impl State for SEND_LINE {}
    impl State for SEND_STATUS {}
    impl State for SEND_HEADERS {}
    impl State for SEND_BODY {}
    impl State for SEND_TRAILER {}
    impl State for RECV_RESPONSE {}
    impl State for RECV_REQUEST {}
    impl State for RECV_BODY {}
    impl State for RECV_TRAILERS {}
    impl State for ENDED {}

    impl Version for () {
        fn version() -> HttpVersion {
            // Calling .version() on a () is a bug.
            unreachable!()
        }
    }
    impl Version for HTTP_10 {
        fn version() -> HttpVersion {
            HttpVersion::Http10
        }
    }
    impl Version for HTTP_11 {
        fn version() -> HttpVersion {
            HttpVersion::Http11
        }
    }

    impl Method for () {}
    impl Method for OPTIONS {}
    impl Method for GET {}
    impl Method for POST {}
    impl Method for PUT {}
    impl Method for DELETE {}
    impl Method for HEAD {
        fn is_head() -> bool {
            true
        }
    }
    impl Method for TRACE {}
    impl Method for CONNECT {
        fn is_connect() -> bool {
            true
        }
    }
    impl Method for PATCH {}

    pub trait MethodWithRequestBody: Method {}
    impl MethodWithRequestBody for POST {}
    impl MethodWithRequestBody for PUT {}
    impl MethodWithRequestBody for PATCH {}

    pub trait MethodWithoutRequestBody: Method {}
    impl MethodWithoutRequestBody for OPTIONS {}
    impl MethodWithoutRequestBody for GET {}
    impl MethodWithoutRequestBody for DELETE {}
    impl MethodWithoutRequestBody for HEAD {}
    impl MethodWithoutRequestBody for CONNECT {}
    impl MethodWithoutRequestBody for TRACE {}

    pub trait MethodWithResponseBody: Method {}
    impl MethodWithResponseBody for OPTIONS {}
    impl MethodWithResponseBody for GET {}
    impl MethodWithResponseBody for POST {}
    impl MethodWithResponseBody for PUT {}
    impl MethodWithResponseBody for DELETE {}
    impl MethodWithResponseBody for TRACE {}
    impl MethodWithResponseBody for PATCH {}

    pub trait MethodWithoutResponseBody: Method {}
    impl MethodWithoutResponseBody for HEAD {}
    impl MethodWithoutResponseBody for CONNECT {}

    pub trait BodyType {}
    impl BodyType for () {}
    impl BodyType for BODY_LENGTH {}
    impl BodyType for BODY_CHUNKED {}
}
