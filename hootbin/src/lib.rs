//!

use error::Error;
use hoot::types;
use hoot::types::state::{SEND_HEADERS, SEND_STATUS};
use hoot::{server::*, Header, Method, Url};
use serde::Serialize;
use std::collections::HashMap;
use std::iter::repeat_with;
use std::mem;
use std::str;
use std::time::Duration;
use std::{io, thread};

use buffer::InputBuffer;

mod buffer;
mod error;

const BUFFER_SIZE: usize = 1024;

/// A request answer.
#[derive(Debug, Default)]
pub(crate) struct Answer {
    status: u16,
    text: &'static str,
    body: Option<Body>,

    // Temporary holder of request body until we received the entire input.
    request_body: Vec<u8>,
}

/// Serialized to JSON as response body.
#[derive(Debug, Default, Serialize)]
pub(crate) struct Body {
    status: u16,
    text: &'static str,
    query: HashMap<String, Arg>,
    headers: HashMap<String, String>,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    json: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub(crate) enum Arg {
    Single(String),
    Multiple(Vec<String>),
}

pub fn serve_single(i: impl io::Read, mut o: impl io::Write, base_url: &str) -> Result<(), Error> {
    // Buffer for reading the request into.
    let mut buf = [0_u8; BUFFER_SIZE];

    let base = Url::parse_str(base_url)?.base();

    // Helper around the input to peek until we have an entire request
    // with all the headers.
    let mut input = InputBuffer::new(i);

    let mut req = Request::new();

    let mut answer = Answer {
        status: 200,
        text: "Ok",
        ..Default::default()
    };

    enum Mode {
        Get,
        Post,
        Put,
        Headers,
        Status(u16),
        Bytes(usize),
        Delay(u64),
        Abort,
    }

    fn send_400(a: &mut Answer) -> Mode {
        a.status = 400;
        a.text = "Bad Request";
        Mode::Abort
    }

    let (mode, has_request_body) = loop {
        // Read more data from the inner io::Read.
        input.fill_more()?;

        // Try to read all headers etc from the current input.
        let attempt = req.try_read_request(&*input, &mut buf)?;

        if !attempt.is_success() {
            // Input might be ended, in which case we have a problem.
            if input.is_ended() {
                // Broken request. The input stream stopped before we
                // got enough to build the entire request with all headers.
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Input stream end before full request",
                )
                .into());
            }

            // Try to read more input from io::Read.
            continue;
        }

        // The read attempt is a success, we got a full request
        // with all the headers.
        let headers = attempt.headers().unwrap();
        let line = attempt.line().unwrap();
        let path = line.path();
        let method = line.method();

        // Fill out the body with headers, URL etc.
        answer.fill_body(headers, &base, path)?;

        let mode = if path == "/get" && method == Method::GET {
            Mode::Get
        } else if path == "/post" && method == Method::POST {
            Mode::Post
        } else if path == "/put" && method == Method::PUT {
            Mode::Put
        } else if path == "/headers" {
            Mode::Headers
        } else if path.starts_with("/status/") {
            match path[8..].parse() {
                Ok(v) => Mode::Status(v),
                Err(_) => send_400(&mut answer),
            }
        } else if path.starts_with("/bytes/") {
            match path[7..].parse() {
                Ok(v) => Mode::Bytes(v),
                Err(_) => send_400(&mut answer),
            }
        } else if path.starts_with("/delay/") {
            match path[7..].parse() {
                Ok(v) => Mode::Delay(v),
                Err(_) => send_400(&mut answer),
            }
        } else {
            answer.body = None;
            answer.status = 404;
            answer.text = "Not Found";
            Mode::Abort
        };

        input.consume(attempt.input_used());
        break (mode, method.has_request_body());
    };

    // Put the request in a state ready to receive the request body.
    let mut req = req.proceed();

    loop {
        if req.is_finished() {
            break;
        }

        // Read more data from the inner io::Read.
        input.fill_more()?;

        // For request methods that has no body (like GET), this will
        // error if the client sent any data. That means we don't
        // need to check it further down.
        let body_part = req.read_body(&*input, &mut buf)?;

        // This is ok also for methods that don't have bodies since
        // body_part would be empty.
        answer.append_body_data(&*body_part);

        // Mark used body input as consumed.
        input.consume(body_part.input_used());
    }

    match mode {
        Mode::Status(status) => {
            answer.status = status;
            answer.text = "";
        }
        Mode::Bytes(amount) => {
            answer.generate_body_data(amount);
        }
        Mode::Delay(secs) => {
            thread::sleep(Duration::from_secs(secs));
        }
        // Abort should not handle a request body, even if the method indicate one.
        Mode::Abort => {}
        // Method indicates request body. Try interpret it.
        _ if has_request_body => {
            answer.attempt_parse_body_data();
        }
        _ => {}
    }

    answer.set_status_on_body();

    // Continue to make a response.
    let resp = req.into_response()?;

    match resp {
        ResponseVariant::Get(r) => send_response(r, &mut buf, answer, o),
        ResponseVariant::Post(r) => send_response(r, &mut buf, answer, o),
        ResponseVariant::Put(r) => send_response(r, &mut buf, answer, o),
        ResponseVariant::Patch(r) => send_response(r, &mut buf, answer, o),
        ResponseVariant::Delete(r) => send_response(r, &mut buf, answer, o),
        ResponseVariant::Head(r) => {
            // No body for HEAD. Just send the start.
            let resp = Response::resume(r, &mut buf);
            let output = send_response_start(resp, &answer)?.flush();

            o.write_all(&output)?;
            Ok(())
        }
        _ => return Err(Error::UnhandledMethod),
    }
}

fn send_response<M: types::MethodWithResponseBody>(
    token: ResumeToken<SEND_STATUS, M, ()>,
    buf: &mut [u8],
    mut answer: Answer,
    mut o: impl io::Write,
) -> Result<(), Error> {
    // The bytes to write to the output.
    let body_bytes = match answer.body.take() {
        // This unwrap is ok, because our Body should _definitely_ be serializable.
        Some(body) => serde_json::to_vec_pretty(&body).unwrap(),
        None => vec![],
    };

    let resp = Response::resume(token, buf);

    let output = send_response_start(resp, &answer)?
        // body with a known length
        .with_body(body_bytes.len())?
        .flush();

    // Writes response status + headers.
    o.write_all(&output)?;

    let token = output.ready();

    let chunk_len = buf.len();
    let mut resp = Response::resume(token, buf);

    // Chunk body into the max sizes possible.
    for chunk in body_bytes.chunks(chunk_len) {
        resp.write_bytes(chunk)?;
        let output = resp.flush();

        o.write_all(&output)?;

        let token = output.ready();
        resp = Response::resume(token, buf);
    }

    Ok(())
}

fn send_response_start<'a, M: types::Method>(
    resp: Response<'a, SEND_STATUS, M, ()>,
    answer: &Answer,
) -> Result<Response<'a, SEND_HEADERS, M, ()>, Error> {
    let resp = resp
        .send_status(answer.status, answer.text)?
        .header("content-type", "application/json")?
        .header("server", "hootbin")?
        .header("access-control-allow-origin", "*")?
        .header("access-control-allow-credentials", "true")?;
    Ok(resp)
}

impl Answer {
    fn fill_body(&mut self, headers: &[Header<'_>], base: &Url, line: &str) -> Result<(), Error> {
        let mut body = Body {
            // status: u16,
            // text: &'static str,
            // args: HashMap<String, Arg>,
            // headers: HashMap<String, String>,
            // origin: String,
            // url: String,
            ..Default::default()
        };

        body.fill_headers(headers);
        body.fill_url(base, line)?;

        self.body = Some(body);

        Ok(())
    }

    fn set_status_on_body(&mut self) {
        let Some(body) = &mut self.body else {
            return;
        };
        body.status = self.status;
        body.text = self.text;
    }

    fn append_body_data(&mut self, data: &[u8]) {
        self.request_body.extend_from_slice(data);
    }

    fn attempt_parse_body_data(&mut self) {
        // If we don't have a Body struct, we will not do anything.
        let Some(body) = &mut self.body else {
            return;
        };

        // Take the data since we will do our best to not allocate more than we need to.
        let data = mem::replace(&mut self.request_body, vec![]);

        // Attempt interpret the body as a string.
        let string = match String::from_utf8(data) {
            Ok(s) => s,
            Err(e) => format!("{:0x?}", e.into_bytes()),
        };

        // Attempt to interpret the body as json.
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&string) {
            body.json = Some(json);
        }

        body.data = Some(string);
    }

    fn generate_body_data(&mut self, amount: usize) {
        // If we don't have a Body struct, we will not do anything.
        let Some(body) = &mut self.body else {
            return;
        };

        const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

        let mut rng = fastrand::Rng::new();
        let bytes: Vec<u8> = repeat_with(|| CHARS[rng.usize(..CHARS.len())])
            .take(amount)
            .collect();

        // This will _definitely_ be possible to read as utf-8.
        let string = String::from_utf8(bytes).unwrap();

        body.data = Some(string);
    }
}

impl Body {
    fn fill_headers(&mut self, headers: &[Header<'_>]) {
        for h in headers {
            let v = self.headers.entry(h.name().to_string()).or_default();
            if !v.is_empty() {
                v.push_str(", ");
            }
            v.push_str(h.value());
        }
    }

    fn fill_url(&mut self, base: &Url, line: &str) -> Result<(), Error> {
        let mut buf_url = [0_u8; 1024];

        let base_url_bytes = base.as_bytes();
        (&mut buf_url[..base_url_bytes.len()]).copy_from_slice(base_url_bytes);

        let path_bytes = line.as_bytes();
        let full_url_len = base_url_bytes.len() + path_bytes.len();
        (&mut buf_url[base_url_bytes.len()..full_url_len]).copy_from_slice(path_bytes);
        let url = str::from_utf8(&buf_url[..full_url_len])?.to_string();

        self.url = url.to_string();

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn test_io() {
        let src: &[u8] = b"GET /get HTTP/1.1\r\nHost:myhost.com\r\n\r\n";
        let mut cur = Cursor::new(Vec::new());

        serve_single(src, &mut cur, "https://myhost.com").unwrap();

        let output = cur.into_inner();
        let s = String::from_utf8(output).unwrap();

        println!("{}", s);
    }
}
