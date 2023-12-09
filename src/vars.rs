#[allow(non_camel_case_types)]
pub mod state {
    pub struct INIT;
    pub struct SEND_LINE;
    pub struct SEND_HEADERS;
    pub struct SEND_BODY;
    pub struct RECV_STATUS;
    pub struct RECV_HEADERS;
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
    use super::body::*;
    use super::method::*;
    use super::state::*;
    use super::version::*;

    pub trait State {}
    pub trait Version {
        fn httparse_version() -> u8;
    }
    pub trait Method {}

    impl State for () {}
    impl State for INIT {}
    impl State for SEND_LINE {}
    impl State for SEND_HEADERS {}
    impl State for SEND_BODY {}
    impl State for RECV_STATUS {}
    impl State for RECV_HEADERS {}

    impl Version for () {
        fn httparse_version() -> u8 {
            unreachable!()
        }
    }
    impl Version for HTTP_10 {
        fn httparse_version() -> u8 {
            0
        }
    }
    impl Version for HTTP_11 {
        fn httparse_version() -> u8 {
            1
        }
    }

    impl Method for () {}
    impl Method for OPTIONS {}
    impl Method for GET {}
    impl Method for POST {}
    impl Method for PUT {}
    impl Method for DELETE {}
    impl Method for HEAD {}
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
