use http::{Method, Version};
use thiserror::Error;

/// Error type for hoot
#[derive(Debug, Error, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Error {
    #[error("bad header: {0}")]
    BadHeader(String),

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
    BodyContentAfterFinish,

    #[error("attempt to write larger body than content-length")]
    BodyLargerThanContentLength,

    #[error("request is not finished")]
    UnfinishedRequest,

    #[error("http parse fail: {0}")]
    HttpParseFail(String),

    #[error("http parse resulted in too many headers")]
    HttpParseTooManyHeaders,

    #[error("http response missing version")]
    MissingResponseVersion,

    #[error("http response missing status")]
    ResponseMissingStatus,

    #[error("http response invalid status")]
    ResponseInvalidStatus,

    #[error("must read http response before body")]
    IncompleteResponse,

    #[error("missing a location header")]
    NoLocationHeader,

    #[error("location header is malformed")]
    BadLocationHeader,

    #[error("received headers with 100-continue response")]
    HeadersWith100,

    #[error("body is chunked")]
    BodyIsChunked,

    #[error("http request is missing a method")]
    RequestMissingMethod,

    #[error("http request invalid method")]
    RequestInvalidMethod,
}

impl From<httparse::Error> for Error {
    fn from(value: httparse::Error) -> Self {
        Error::HttpParseFail(value.to_string())
    }
}
