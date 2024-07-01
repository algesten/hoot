//! HTTP/1.1 client
//!
//! hoot is Sans-IO, which means "writing" and "reading" are made via buffers
//! rather than the Write/Read std traits.
//!
//! The [`Flow`] object attempts to encode correct HTTP/1.1 handling using
//! state variables, for example `Flow<'a, SendRequest>` to represent the
//! lifecycle stage where we are to send the request.
//!
//! The states are:
//!
//! * **Prepare** - Preparing a request means 1) adding headers such as
//!   cookies. 2) acquiring the connection from a pool or opening a new
//!   socket (potentially wrappping in TLS)
//! * **SendRequest** - Send the "prelude", which is the method, path
//!   and version as well as the request headers
//! * **SendBody** - Send the request body
//! * **Await100** - If there is an `Expect: 100-continue` header, the
//!   client should pause before sending the body
//! * **RecvResponse** - Receive the response, meaning the status and
//!   version and the response headers
//! * *RecvBody** - Receive the response body
//! * **Redirect** - Handle redirects, potentially spawning new requests
//! * **Cleanup** - Return the connection to the pool or close it
//!
//!
//! ```text
//!                            ┌──────────────────┐
//! ┌ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ▶│     Prepare      │
//!                            └──────────────────┘
//! │                                    │
//!                                      ▼
//! │                          ┌──────────────────┐
//!                         ┌──│   SendRequest    │──────────────┐
//! │                       │  └──────────────────┘              │
//!                         │            │                       │
//! │                       │            ▼                       ▼
//!                         │  ┌──────────────────┐    ┌──────────────────┐
//! │                       │  │     SendBody     │◀───│     Await100     │
//!                         │  └──────────────────┘    └──────────────────┘
//! │                       │            │                       │
//!                         │            ▼                       │
//! │                       └─▶┌──────────────────┐◀─────────────┘
//!              ┌─────────────│   RecvResponse   │──┐
//! │            │             └──────────────────┘  │
//!              │                       │           │
//! │            ▼                       ▼           │
//!    ┌──────────────────┐    ┌──────────────────┐  │
//! └ ─│     Redirect     │◀───│     RecvBody     │  │
//!    └──────────────────┘    └──────────────────┘  │
//!              │                       │           │
//!              │                       ▼           │
//!              │             ┌──────────────────┐  │
//!              └────────────▶│     Cleanup      │◀─┘
//!                            └──────────────────┘
//! ```
//!
//! # Example
//!
//! ```
//! use hoot::client::Flow;
//! use hoot::http::Request;
//! use hoot::client::results::*;
//!
//! let request = Request::put("https://example.test/my-path")
//!     .header("Expect", "100-continue")
//!     .header("x-foo", "bar")
//!     .body(())
//!     .unwrap();
//!
//! // ********************************** Prepare
//!
//! let mut flow = Flow::new(&request).unwrap();
//!
//! // Prepare with state from cookie jar. The uri
//! // is used to key the cookies.
//! let uri = flow.uri();
//!
//! // flow.header("Cookie", "my_cookie1=value1");
//! // flow.header("Cookie", "my_cookie2=value2");
//!
//! // Obtain a connection for the uri, either a
//! // pooled connection from a previous http/1.1
//! // keep-alive, or open a new. The connection
//! // must be TLS wrapped if the scheme so indicate.
//! // let connection = todo!();
//!
//! // Hoot is Sans-IO meaning it does not use any
//! // Write trait or similar. Requests and request
//! // bodies are written to a buffer that in turn
//! // should be sent via the connection.
//! let mut output = vec![0_u8; 1024];
//!
//! // ********************************** SendRequest
//!
//! // Proceed to the next state writing the request.
//! // Hoot calls this the request method/path + headers
//! // the "prelude".
//! let mut flow = flow.proceed();
//!
//! let output_used = flow.write(&mut output).unwrap();
//! assert_eq!(output_used, 107);
//!
//! assert_eq!(&output[..output_used], b"\
//!     PUT /my-path HTTP/1.1\r\n\
//!     host: example.test\r\n\
//!     transfer-encoding: chunked\r\n\
//!     expect: 100-continue\r\n\
//!     x-foo: bar\r\n\
//!     \r\n");
//!
//! // Check we can continue to send the body
//! assert!(flow.can_proceed());
//!
//! // ********************************** Await100
//!
//! // In this example, we know the next state is Await100.
//! // A real client needs to match on the variants.
//! let mut flow = match flow.proceed() {
//!     Some(SendRequestResult::Await100(v)) => v,
//!     _ => panic!(),
//! };
//!
//! // When awaiting 100, the client should run a timer and
//! // proceed to sending the body either when the server
//! // indicates it can receive the body, or the timer runs out.
//!
//! // This boolean can be checked whether there's any point
//! // in keeping waiting for the timer to run out.
//! assert!(flow.can_keep_await_100());
//!
//! let input = b"HTTP/1.1 100 Continue\r\n\r\n";
//! let input_used = flow.try_read_100(input).unwrap();
//!
//! assert_eq!(input_used, 25);
//! assert!(!flow.can_keep_await_100());
//!
//! // ********************************** SendBody
//!
//! // Proceeding is possible regardless of whether the
//! // can_keep_await_100() is true or false.
//! // A real client needs to match on the variants.
//! let mut flow = match flow.proceed() {
//!     Await100Result::SendBody(v) => v,
//!     _ => panic!(),
//! };
//!
//! let (input_used, o1) =
//!     flow.write(b"hello", &mut output).unwrap();
//!
//! assert_eq!(input_used, 5);
//!
//! // When doing transfer-encoding: chunked,
//! // the end of body must be signaled with
//! // an empty input. This is also valid for
//! // regular content-length body.
//! assert!(!flow.can_proceed());
//!
//! let (_, o2) = flow.write(&[], &mut output[o1..]).unwrap();
//!
//! let output_used = o1 + o2;
//! assert_eq!(output_used, 15);
//!
//! assert_eq!(&output[..output_used], b"\
//!     5\r\n\
//!     hello\
//!     \r\n\
//!     0\r\n\
//!     \r\n");
//!
//! assert!(flow.can_proceed());
//!
//! // ********************************** RecvRequest
//!
//! // Proceed to read the request.
//! let mut flow = flow.proceed().unwrap();
//!
//! let part = b"HTTP/1.1 200 OK\r\nContent-Len";
//! let full = b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n";
//!
//! // try_response can be used repeatedly until we
//! // get enough content that is both a prelude and
//! // all headers.
//! let (input_used, maybe_response) =
//!     flow.try_response(part).unwrap();
//!
//! assert_eq!(input_used, 0);
//! assert!(maybe_response.is_none());
//!
//! let (input_used, maybe_response) =
//!     flow.try_response(full).unwrap();
//!
//! assert_eq!(input_used, 38);
//! let response = maybe_response.unwrap();
//!
//! // ********************************** RecvBody
//!
//! // It's not possible to proceed until we
//! // have read a response.
//! let mut flow = match flow.proceed() {
//!     Some(RecvResponseResult::RecvBody(v)) => v,
//!     _ => panic!(),
//! };
//!
//! let(input_used, output_used) =
//!     flow.read(b"hi there!", &mut output).unwrap();
//!
//! assert_eq!(input_used, 9);
//! assert_eq!(output_used, 9);
//!
//! assert_eq!(&output[..output_used], b"hi there!");
//!
//! // ********************************** Cleanup
//!
//! let flow = match flow.proceed() {
//!     Some(RecvBodyResult::Cleanup(v)) => v,
//!     _ => panic!(),
//! };
//!
//! if flow.must_close_connection() {
//!     // connection.close();
//! } else {
//!     // connection.return_to_pool();
//! }
//!
//! ```

mod call;
pub use call::Call;

mod flow;
pub use flow::{CloseReason, Flow};

pub mod results {
    pub use super::flow::{Await100Result, RecvBodyResult};
    pub use super::flow::{RecvResponseResult, SendRequestResult};
}

mod amended;

mod holder;

#[cfg(test)]
mod test;

/// Max number of additional headers to amend an HTTP request with
pub const MAX_EXTRA_HEADERS: usize = 64;

/// Max number of headers to parse from an HTTP response
pub const MAX_RESPONSE_HEADERS: usize = 128;
