# h1-call

no_std friendly request library.

```rust
let mut buf = [0; 1024];

// Call::new starts a new request. The buffer can be on the stack, heap or anywhere you want.
// It is borrowed until we call .flush().
let (state, out) = Call::new(&mut buf)
    // First we select if this is HTTP/1.0 or HTTP/1.1
    .http_10()
    // Then comes the verb (method) + PATH. The methods are narrowed by the type to only be
    // valid for HTTP/1.0. This writes to the underlying buffer – hence the Result return in
    // case buffer overflows.
    .get("/some-path")?
    // At any point we can release the buffer. This returns a (state, out) tuple, where state is
    // a "state token" used to resume the call once the output is written to a transport.
    .flush();

assert_eq!(out, b"GET /some-path HTTP/1.0\r\n");

// Here we use Call::resume with the state token to resume over the same
// (but could be a different) buffer.
let (state, out) = Call::resume(state, &mut buf)
    // Headers write to the buffer, hence the Result return.
    .header("accept", "text/plain")?
    .header("x-my-thing", "martin")?
    // Finish takes us to awaiting the remote status. By using types, this is only available
    // for HTTP verbs (methods) that have no body.
    .finish()?
    // Again, release the buffer to write to a transport.
    .flush();

assert_eq!(out, b"accept: text/plain\r\nx-my-thing: martin\r\n\r\n");

// Resume call using the buffer.
let mut call = Call::resume(state, &mut buf);

// Attempt to parse a bunch of incomplete status lines. ParseResult::Incomplete
// means the state is not progressed.
const ATTEMPT: &[&[u8]] = &[b"HT", b"HTTP/1.0", b"HTTP/1.0 20"];
for a in ATTEMPT {
    call = match call.parse_status(a)? {
        ParseResult::Incomplete(c) => c,
        ParseResult::Complete(_, _, _) => unreachable!(),
    };
}

// Parse the complete status line. ParseResult::Complete continues the state.
let ParseResult::Complete(call, _n, status) = call.parse_status(b"HTTP/1.0 200 OK\r\n")?
else {
    panic!("Expected complete parse")
};

assert_eq!(status, Status(200, Some("OK")));
```
