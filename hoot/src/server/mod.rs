//! Server HTTP/1.1 request and response
//!
//! # Example
//!
//! ```
//! use hoot::server::{Request, Response, ResponseVariant};
//! use hoot::{Method, HttpVersion, BodyWriter};
//!
//! let mut buf = [0; 1024];
//!
//! // ************* READ REQUEST LINE *****************
//!
//! let mut request = Request::new();
//!
//! // Try read incomplete input. The provided buffer is required
//! // to parse request headers.
//! let attempt =
//!     request.try_read_request(b"GET /path HTTP/1.", &mut buf)?;
//! assert!(!attempt.is_success());
//!
//! const COMPLETE: &[u8] =
//!     b"GET /path HTTP/1.1\r\nHost: foo\r\nContent-Length: 10\r\n\r\n";
//!
//! // Try read complete input (and succeed). Borrow the buffer again.
//! let attempt = request.try_read_request(COMPLETE, &mut buf)?;
//! assert!(attempt.is_success());
//!
//! // Read request line information.
//! let line = attempt.line().unwrap();
//! assert_eq!(line.version(), HttpVersion::Http11);
//! assert_eq!(line.method(), Method::GET);
//! assert_eq!(line.path(), "/path");
//!
//! // Read headers.
//! let headers = attempt.headers().unwrap();
//! assert_eq!(headers[0].name(), "Host");
//! assert_eq!(headers[0].value(), "foo");
//! assert_eq!(headers[1].name(), "Content-Length");
//! assert_eq!(headers[1].value(), "10");
//!
//! // Proceed to reading request body.
//! let request = request.proceed();
//!
//! // GET requests have no body.
//! assert!(request.is_finished());
//!
//! // ************* SERVE RESPONSE *****************
//!
//! // This gives us variants, i.e. methods, that we can
//! // respond differently to.
//! let variants = request.into_response()?;
//!
//! // This matches out a response token with the type
//! // state that helps us form a correct response.
//! let token = match variants {
//!     ResponseVariant::Get(v) => v,
//!     ResponseVariant::Head(_) => todo!(),
//!     ResponseVariant::Post(_) => todo!(),
//!     ResponseVariant::Put(_) => todo!(),
//!     ResponseVariant::Delete(_) => todo!(),
//!     ResponseVariant::Connect(_) => todo!(),
//!     ResponseVariant::Options(_) => todo!(),
//!     ResponseVariant::Trace(_) => todo!(),
//!     ResponseVariant::Patch(_) => todo!(),
//! };
//!
//! let response = Response::resume(token, &mut buf);
//!
//! let output = response
//!     .send_status(200, "Ok")?
//!     .header("content-type", "application/json")?
//!     .with_chunked()?
//!     .flush();
//!
//! const EXPECTED: &[u8] = b"HTTP/1.1 200 Ok\r\n\
//!     content-type: application/json\r\n\
//!     Transfer-Encoding: chunked\r\n\r\n";
//!
//! // Output derefs to `&[u8]`, but if that feels opaque,
//! // we can use `as_bytes()`.
//! assert_eq!(&*output, EXPECTED);
//!
//! let token = output.ready();
//!
//! let response = Response::resume(token, &mut buf);
//!
//! let output = response
//!     .write_bytes(b"{\"hello\":\"world\"}")?
//!     .finish()?
//!     .flush();
//!
//! const EXPECTED_BODY:
//!     &[u8] = b"11\r\n{\"hello\":\"world\"}\r\n0\r\n\r\n";
//!
//! assert_eq!(&*output, EXPECTED_BODY);
//!
//! # Ok::<(), hoot::HootError>(())
//! ```

mod req;
pub use req::{Line, Request};

mod res;
pub use res::{Response, ResponseVariant, ResumeToken};
