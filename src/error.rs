use core::fmt;
use core::num::ParseIntError;
use core::str::Utf8Error;

use crate::url::UrlError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HootError {
    /// The borrowed buffer did not have enough space to hold the
    /// data we attempted to write.
    ///
    /// Call `.flush()`, write the output to the transport followed by `Call::resume()`.
    OutputOverflow,

    /// Invalid byte in header name.
    HeaderName,

    /// Invalid byte in header value.
    HeaderValue,

    /// Invalid Response status.
    Status,

    /// Invalid byte in new line.
    NewLine,

    /// Parsed more headers than provided buffer can contain.
    TooManyHeaders,

    /// Encountered a forbidden header name.
    ///
    /// `content-length` and `transfer-encoding` must be set using
    /// `with_body()` and `with_body_chunked()`.
    ForbiddenBodyHeader,

    /// Header is not allowed for HTTP/1.1
    ForbiddenHttp11Header,

    /// The trailer name is not allowed.
    ForbiddenTrailer,

    /// Attempt to send more content than declared in the `Content-Length` header.
    SentMoreThanContentLength,

    /// Attempt to send less content than declared in the `Content-Length` header.
    SentLessThanContentLength,

    /// Attempt to send more content than declared in the `Content-Length` header.
    RecvMoreThanContentLength,

    /// Attempt to send less content than declared in the `Content-Length` header.
    RecvLessThanContentLength,

    /// Failed to read bytes as &str
    ConvertBytesToStr,

    /// The requested HTTP version does not match the response HTTP version.
    HttpVersionMismatch,

    /// If we attempt to call `.complete()` on an AttemptStatus that didn't get full input to succeed.
    StatusIsNotComplete,

    /// Failed to parse an integer. This can happen if a Content-Length header contains bogus.
    ParseIntError,

    /// More than one Content-Length header in response.
    DuplicateContentLength,

    /// Incoming chunked encoding is incorrect.
    IncorrectChunk,

    /// Invalid byte where token is required.
    Token,

    /// Invalid byte in HTTP version.
    Version,

    /// Did not read body to finish.
    BodyNotFinished,

    /// Request method is unknown.
    UnknownMethod,

    /// Url parsing error
    UrlError(UrlError),
}

pub(crate) static OVERFLOW: Result<()> = Err(HootError::OutputOverflow);

pub type Result<T> = core::result::Result<T, HootError>;

impl From<Utf8Error> for HootError {
    fn from(_: Utf8Error) -> Self {
        HootError::ConvertBytesToStr
    }
}

impl From<ParseIntError> for HootError {
    fn from(_: ParseIntError) -> Self {
        HootError::ParseIntError
    }
}

impl From<httparse::Error> for HootError {
    fn from(value: httparse::Error) -> Self {
        match value {
            httparse::Error::HeaderName => HootError::HeaderName,
            httparse::Error::HeaderValue => HootError::HeaderValue,
            httparse::Error::NewLine => HootError::NewLine,
            httparse::Error::Status => HootError::Status,
            httparse::Error::Token => HootError::Token,
            httparse::Error::TooManyHeaders => HootError::TooManyHeaders,
            httparse::Error::Version => HootError::Version,
        }
    }
}

impl fmt::Display for HootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use HootError::*;
        let s = match self {
            OutputOverflow => "output buffer overflow",
            HeaderName => "invalid header name",
            HeaderValue => "invalid header value",
            NewLine => "invalid new line",
            Status => "invalid response status",
            Token => "invalid token",
            TooManyHeaders => "too many headers",
            Version => "invalid HTTP version",
            ForbiddenBodyHeader => "forbidden header name",
            ForbiddenHttp11Header => "forbidden header for http1.1",
            ForbiddenTrailer => "forbidden trailer",
            SentMoreThanContentLength => "sent more than content-length",
            SentLessThanContentLength => "sent less than content-length",
            RecvMoreThanContentLength => "received more than content-length",
            RecvLessThanContentLength => "received less than content-length",
            ConvertBytesToStr => "failed to convert &[u8] to &str",
            HttpVersionMismatch => "http version mismatch",
            StatusIsNotComplete => "called complete() before entire status read",
            ParseIntError => "failed to parse integer",
            DuplicateContentLength => "multiple content-length headers",
            IncorrectChunk => "incorrect incoming body chunk",
            BodyNotFinished => "called finish() before body was finished",
            UnknownMethod => "unknown incoming method",
            UrlError(v) => {
                write!(f, "url: {}", v)?;
                return Ok(());
            }
        };

        write!(f, "{}", s)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for HootError {}
