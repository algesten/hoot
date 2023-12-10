#[allow(non_camel_case_types)]
pub mod state {
    pub struct INIT;
    pub struct SEND_LINE;
    pub struct SEND_HEADERS;
    pub struct SEND_BODY;
    pub struct SEND_TRAILER;
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
        fn head() -> bool {
            false
        }
    }

    impl State for () {}
    impl State for INIT {}
    impl State for SEND_LINE {}
    impl State for SEND_HEADERS {}
    impl State for SEND_BODY {}
    impl State for SEND_TRAILER {}
    impl State for RECV_RESPONSE {}
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
        fn head() -> bool {
            true
        }
    }
    impl Method for TRACE {}
    impl Method for CONNECT {}
    impl Method for PATCH {}

    pub trait MethodWithBody: Method {}

    impl MethodWithBody for POST {}
    impl MethodWithBody for PUT {}
    impl MethodWithBody for PATCH {}

    pub trait MethodWithoutBody: Method {}
    impl MethodWithoutBody for OPTIONS {}
    impl MethodWithoutBody for GET {}
    impl MethodWithoutBody for DELETE {}
    impl MethodWithoutBody for HEAD {}
    impl MethodWithoutBody for TRACE {}

    pub trait MethodConnect: MethodWithoutBody {}
    impl MethodWithoutBody for CONNECT {}
    impl MethodConnect for CONNECT {}

    pub trait BodyType {}
    impl BodyType for () {}
    impl BodyType for BODY_LENGTH {}
    impl BodyType for BODY_CHUNKED {}
}
