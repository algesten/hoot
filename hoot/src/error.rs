use http::{Method, Version};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("unsupported http version")]
    UnsupportedVersion,

    #[error("{0} not valid for HTTP version {1:?}")]
    MethodVersionMismatch(Method, Version),

    #[error("more than one host header")]
    TooManyHostHeaders,

    #[error("more than one content-length header")]
    TooManyContentLengthHeaders,

    #[error("host header is not a string")]
    BadHostHeader,

    #[error("content-length header not a number")]
    BadContentLengthHeader,

    #[error("method forbids body: {0}")]
    MethodForbidsBody(Method),

    #[error("method requires body: {0}")]
    MethodRequiresBody(Method),

    #[error("output too small to write output")]
    OutputOverflow,

    #[error("chunk length is not ascii")]
    ChunkLenNotAscii,

    #[error("chunk length cannot be read as a number")]
    ChunkLenNotANumber,

    #[error("chunk expected crlf as next character")]
    ChunkExpectedCrLf,

    #[error("attempt to stream body after sending finish (&[])")]
    StreamingContentAfterFinish,

    #[error("attempt to write larger body than content-length")]
    BodyLargerThanContentLength,

    #[error("request is not finished")]
    UnfinishedRequest,

    #[error("http parse fail: {0}")]
    HttpParseFail(String),

    #[error("http response missing version")]
    MissingResponseVersion,

    #[error("http response missing status")]
    ResponseMissingStatus,

    #[error("http response invalid status")]
    ResponseInvalidStatus,

    #[error("must read http response before body")]
    IncompleteResponse,
}

impl From<httparse::Error> for Error {
    fn from(value: httparse::Error) -> Self {
        Error::HttpParseFail(value.to_string())
    }
}
