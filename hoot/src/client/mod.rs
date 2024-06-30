//! HTTP/1.1 client
//!
//! hoot is Sans-IO, which means "writing" and "reading" are made via buffers
//! rather than the Write/Read std traits.
//!
//! The [`State`] object attempts to encode correct HTTP/1.1 handling using
//! state variables, for example `State<'a, SendRequest>` to represent the
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
//!                             ┌──────────────────┐
//! ┌ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─▶│     Prepare      │
//!                             └──────────────────┘
//! │                                     │
//!                                       ▼
//! │                           ┌──────────────────┐
//!                          ┌──│   SendRequest    │──────────────┐
//! │                        │  └──────────────────┘              │
//!                          │            │                       │
//! │                        │            ▼                       ▼
//!                          │  ┌──────────────────┐    ┌──────────────────┐
//! │                        │  │     SendBody     │◀───│     Await100     │
//!                          │  └──────────────────┘    └──────────────────┘
//! │                        │            │                       │
//!                          │            ▼                       │
//! │                        └─▶┌──────────────────┐◀─────────────┘
//!              ┌──────────────│   RecvResponse   │──┐
//! │            │              └──────────────────┘  │
//!              │                        │           │
//! │            ▼                        ▼           │
//!    ┌──────────────────┐     ┌──────────────────┐  │
//! └ ─│     Redirect     │◀────│     RecvBody     │  │
//!    └──────────────────┘     └──────────────────┘  │
//!              │                        │           │
//!              │                        ▼           │
//!              │              ┌──────────────────┐  │
//!              └─────────────▶│     Cleanup      │◀─┘
//!                             └──────────────────┘
//! ```
//!
//! # Example
//!
//! ```
//! use hoot::client::State;
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
//! let mut state = State::new(&request).unwrap();
//!
//! // Prepare with state from cookie jar. The uri
//! // is used to key the cookies.
//! let uri = state.uri();
//!
//! // state.header("Cookie", "my_cookie1=value1");
//! // state.header("Cookie", "my_cookie2=value2");
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
//! let mut state = state.proceed();
//!
//! let output_used = state.write(&mut output).unwrap();
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
//! assert!(state.can_proceed());
//!
//! // ********************************** Await100
//!
//! // In this example, we know the next state is Await100.
//! // A real client needs to match on the variants.
//! let mut state = match state.proceed() {
//!     SendRequestResult::Await100(v) => v,
//!     _ => panic!(),
//! };
//!
//! // When awaiting 100, the client should run a timer and
//! // proceed to sending the body either when the server
//! // indicates it can receive the body, or the timer runs out.
//!
//! // This boolean can be checked whether there's any point
//! // in keeping waiting for the timer to run out.
//! assert!(state.can_keep_await_100());
//!
//! let input = b"HTTP/1.1 100 Continue\r\n\r\n";
//! let input_used = state.try_read_100(input).unwrap().unwrap();
//!
//! assert_eq!(input_used, 25);
//! assert!(!state.can_keep_await_100());
//!
//! // ********************************** SendBody
//!
//! // Proceeding is possible regardless of whether the
//! // can_keep_await_100() is true or false.
//! // A real client needs to match on the variants.
//! let mut state = match state.proceed() {
//!     Await100Result::SendBody(v) => v,
//!     _ => panic!(),
//! };
//!
//! let (input_used, o1) =
//!     state.write(b"hello", &mut output).unwrap();
//!
//! assert_eq!(input_used, 5);
//!
//! // When doing transfer-encoding: chunked,
//! // the end of body must be signaled with
//! // an empty input. This is also valid for
//! // regular content-length body.
//! assert!(!state.can_proceed());
//!
//! let (_, o2) = state.write(&[], &mut output[o1..]).unwrap();
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
//! assert!(state.can_proceed());
//!
//! // ********************************** RecvRequest
//!
//! // Proceed to read the request.
//! let mut state = state.proceed();
//!
//! let part = b"HTTP/1.1 200 OK\r\nContent-Len";
//! let full = b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n";
//!
//! // try_response can be used repeatedly until we
//! // get enough content that is both a prelude and
//! // all headers.
//! let (input_used, maybe_response) =
//!     state.try_response(part).unwrap();
//!
//! assert_eq!(input_used, 0);
//! assert!(maybe_response.is_none());
//!
//! let (input_used, maybe_response) =
//!     state.try_response(full).unwrap();
//!
//! assert_eq!(input_used, 38);
//! let response = maybe_response.unwrap();
//!
//! // ********************************** RecvBody
//!
//! // It's not possible to proceed until we
//! // have read a response.
//! let mut state = match state.proceed() {
//!     RecvResponseResult::RecvBody(v) => v,
//!     _ => panic!(),
//! };
//!
//! let(input_used, output_used) =
//!     state.read(b"hi there!", &mut output).unwrap();
//!
//! assert_eq!(input_used, 9);
//! assert_eq!(output_used, 9);
//!
//! assert_eq!(&output[..output_used], b"hi there!");
//!
//! // ********************************** Cleanup
//!
//! let state = match state.proceed() {
//!     RecvBodyResult::Cleanup(v) => v,
//!     _ => panic!(),
//! };
//!
//! if state.must_close_connection() {
//!     // connection.close();
//! } else {
//!     // connection.return_to_pool();
//! }
//!
//! ```

mod call;
pub use call::Call;

mod state;
pub use state::{CloseReason, State};

pub mod results {
    pub use super::state::{Await100Result, RecvBodyResult};
    pub use super::state::{RecvResponseResult, SendRequestResult};
}

mod amended;

mod holder;

#[cfg(test)]
mod test;

// TODO(martin): let's move these type states somewhere more relevant.

/// Type state for requests without bodies via [`Call::without_body()`]
#[doc(hidden)]
pub struct WithoutBody(());

/// Type state for streaming bodies via [`Call::with_streaming_body()`]
#[doc(hidden)]
pub struct WithBody(());

/// Type state for receiving the HTTP Response
#[doc(hidden)]
pub struct RecvResponse(());

/// Type state for receiving the response body
#[doc(hidden)]
pub struct RecvBody(());

/// Max number of additional headers to amend an HTTP request with
pub const MAX_EXTRA_HEADERS: usize = 64;

/// Max number of headers to parse from an HTTP response
pub const MAX_RESPONSE_HEADERS: usize = 128;
