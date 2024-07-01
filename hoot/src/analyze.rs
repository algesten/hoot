use http::{HeaderName, HeaderValue, Method, Request, StatusCode, Version};

use crate::body::BodyWriter;
use crate::util::compare_lowercase_ascii;
use crate::Error;

pub(crate) trait MethodExt {
    fn is_http10(&self) -> bool;
    fn is_http11(&self) -> bool;
    fn need_request_body(&self) -> bool;
    fn verify_version(&self, version: Version) -> Result<(), Error>;
}

impl MethodExt for Method {
    fn is_http10(&self) -> bool {
        self == Method::GET || self == Method::HEAD || self == Method::POST
    }

    fn is_http11(&self) -> bool {
        self == Method::PUT
            || self == Method::DELETE
            || self == Method::CONNECT
            || self == Method::OPTIONS
            || self == Method::TRACE
            || self == Method::PATCH
    }

    fn need_request_body(&self) -> bool {
        self == Method::POST || self == Method::PUT || self == Method::PATCH
    }

    fn verify_version(&self, v: Version) -> Result<(), Error> {
        if v == Version::HTTP_10 && v == Version::HTTP_11 {
            return Err(Error::UnsupportedVersion);
        }

        let method_ok = self.is_http10() || v == Version::HTTP_11 && self.is_http11();

        if !method_ok {
            return Err(Error::MethodVersionMismatch(self.clone(), v));
        }

        Ok(())
    }
}

pub(crate) trait HeaderIterExt {
    fn has(self, key: &str, value: &str) -> bool;
    fn has_expect_100(self) -> bool;
}

impl<'a, I: Iterator<Item = (&'a HeaderName, &'a HeaderValue)>> HeaderIterExt for I {
    fn has(self, key: &str, value: &str) -> bool {
        self.filter(|i| i.0 == key).any(|i| i.1 == value)
    }

    fn has_expect_100(self) -> bool {
        self.has("expect", "100-continue")
    }
}

pub(crate) trait StatusExt {
    /// Detect 307/308 redirect
    fn is_redirect_retaining_status(&self) -> bool;
}

impl StatusExt for StatusCode {
    fn is_redirect_retaining_status(&self) -> bool {
        *self == StatusCode::TEMPORARY_REDIRECT || *self == StatusCode::PERMANENT_REDIRECT
    }
}

pub(crate) trait RequestExt {
    fn analyze(&self, wanted_mode: BodyWriter) -> Result<RequestInfo, Error>;
}

pub(crate) struct RequestInfo {
    pub body_mode: BodyWriter,
    pub req_host_header: bool,
    pub req_body_header: bool,
}

impl<B> RequestExt for Request<B> {
    fn analyze(&self, wanted_mode: BodyWriter) -> Result<RequestInfo, Error> {
        let v = self.version();
        let m = self.method();

        m.verify_version(v)?;

        let count_host = self.headers().get_all("host").iter().count();
        if count_host > 1 {
            return Err(Error::TooManyHostHeaders);
        }

        let count_len = self.headers().get_all("content-length").iter().count();
        if count_len > 1 {
            return Err(Error::TooManyContentLengthHeaders);
        }

        let mut req_host_header = false;
        if let Some(h) = self.headers().get("host") {
            h.to_str().map_err(|_| Error::BadHostHeader)?;
            req_host_header = true;
        }

        let mut content_length: Option<u64> = None;
        if let Some(h) = self.headers().get("content-length") {
            let n = h
                .to_str()
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or(Error::BadContentLengthHeader)?;
            content_length = Some(n);
        }

        let has_chunked = self
            .headers()
            .get_all("transfer-encoding")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .any(|v| compare_lowercase_ascii(v, "chunked"));

        let mut req_body_header = false;

        // https://datatracker.ietf.org/doc/html/rfc2616#section-4.4
        // Messages MUST NOT include both a Content-Length header field and a
        // non-identity transfer-coding. If the message does include a non-
        // identity transfer-coding, the Content-Length MUST be ignored.
        let body_mode = if has_chunked {
            // chunked "wins"
            req_body_header = true;
            BodyWriter::new_chunked()
        } else if let Some(n) = content_length {
            // user provided content-length second
            req_body_header = true;
            BodyWriter::new_sized(n)
        } else {
            wanted_mode
        };

        let need_body = self.method().need_request_body();
        let has_body = body_mode.has_body();

        if !need_body && has_body {
            return Err(Error::MethodForbidsBody(self.method().clone()));
        } else if need_body && !has_body {
            return Err(Error::MethodRequiresBody(self.method().clone()));
        }

        Ok(RequestInfo {
            body_mode,
            req_host_header,
            req_body_header,
        })
    }
}
