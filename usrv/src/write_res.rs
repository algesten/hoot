use std::io;
use std::io::Read;

use hoot::server::Response as HootResponse;
use hoot::server::{ResponseVariant, ResumeToken};
use hoot::types::state::{SEND_HEADERS, SEND_STATUS};
use hoot::types::{Method, MethodWithResponseBody, MethodWithoutResponseBody};
use hoot::{BodyWriter, RecvBodyMode};

use crate::{Error, Response};

pub fn write_response(
    request_method: http::Method,
    response: Response,
    writer: &mut dyn io::Write,
) -> Result<(), Error> {
    let mut write_buf = vec![0_u8; 1024];

    write_response_with_buffer(request_method, response, writer, &mut write_buf)
}

pub(crate) fn write_response_with_buffer(
    request_method: http::Method,
    response: Response,
    writer: &mut dyn io::Write,
    write_buf: &mut Vec<u8>,
) -> Result<(), Error> {
    let method: hoot::Method = request_method.into();
    let variant = ResponseVariant::unchecked_from_method(method);

    match variant {
        ResponseVariant::Get(v) => write_with_body(method, response, writer, write_buf, v),
        ResponseVariant::Head(v) => write_without_body(response, writer, write_buf, v),
        ResponseVariant::Post(v) => write_with_body(method, response, writer, write_buf, v),
        ResponseVariant::Put(v) => write_with_body(method, response, writer, write_buf, v),
        ResponseVariant::Delete(v) => write_with_body(method, response, writer, write_buf, v),
        ResponseVariant::Connect(v) => write_without_body(response, writer, write_buf, v),
        ResponseVariant::Options(v) => write_with_body(method, response, writer, write_buf, v),
        ResponseVariant::Trace(v) => write_with_body(method, response, writer, write_buf, v),
        ResponseVariant::Patch(v) => write_with_body(method, response, writer, write_buf, v),
    }
}

fn write_with_body<M: MethodWithResponseBody>(
    method: hoot::Method,
    response: Response,
    writer: &mut dyn io::Write,
    write_buf: &mut Vec<u8>,
    token: ResumeToken<SEND_STATUS, M, ()>,
) -> Result<(), Error> {
    let token = write_header(&response, writer, write_buf, token)?;

    let http_10 = response.version() == http::Version::HTTP_10;
    let status = response.status().as_u16();

    let header_lookup = |name: &str| {
        if let Some(header) = response.headers().get(name) {
            return header.to_str().ok();
        }
        None
    };

    let body_mode = RecvBodyMode::for_response(http_10, method, status, &header_lookup)?;

    let (_, mut body) = response.into_parts();

    const DEFAULT_SIZE_STREAMING_BODIES: usize = 32_768;
    const CHUNK_OVERHEAD: usize = 10;

    let body_size = if let Some(size) = body.size() {
        size
    } else {
        DEFAULT_SIZE_STREAMING_BODIES
    };

    let needed_buffer_size = body_size * 2 + CHUNK_OVERHEAD;

    if write_buf.len() < needed_buffer_size {
        write_buf.resize(needed_buffer_size, 0);
    }

    let (tmp, output) = write_buf.split_at_mut(needed_buffer_size / 2 - CHUNK_OVERHEAD);

    let hoot_res = HootResponse::resume(token, output);

    match body_mode {
        RecvBodyMode::LengthDelimited(length) => {
            let mut hoot_res = hoot_res.with_body(length)?;
            loop {
                let n = body.read(tmp)?;

                if n == 0 {
                    break;
                }

                let out = hoot_res.write_bytes(&tmp[..n])?.flush();

                writer.write_all(&out)?;

                let token = out.ready();
                hoot_res = HootResponse::resume(token, output);
            }

            hoot_res.finish()?;
        }
        RecvBodyMode::Chunked => {
            let mut hoot_res = hoot_res.with_chunked()?;
            loop {
                let n = body.read(tmp)?;

                if n == 0 {
                    break;
                }

                let out = hoot_res.write_bytes(&tmp[..n])?.flush();

                writer.write_all(&out)?;

                let token = out.ready();
                hoot_res = HootResponse::resume(token, output);
            }

            hoot_res.finish()?;
        }
        RecvBodyMode::CloseDelimited => {
            todo!()
        }
    }

    Ok(())
}

fn write_without_body<M: MethodWithoutResponseBody>(
    response: Response,
    writer: &mut dyn io::Write,
    mut write_buf: &mut Vec<u8>,
    token: ResumeToken<SEND_STATUS, M, ()>,
) -> Result<(), Error> {
    let token = write_header(&response, writer, write_buf, token)?;

    let hoot_res = HootResponse::resume(token, &mut write_buf);

    let out = hoot_res.send()?.flush();
    writer.write_all(&out)?;

    Ok(())
}

fn write_header<M: Method>(
    response: &Response,
    writer: &mut dyn io::Write,
    mut write_buf: &mut Vec<u8>,
    token: ResumeToken<SEND_STATUS, M, ()>,
) -> Result<ResumeToken<SEND_HEADERS, M, ()>, Error> {
    const MAX_STATUS_LINE_LENGTH: usize = 256;

    if write_buf.len() < MAX_STATUS_LINE_LENGTH {
        write_buf.resize(write_buf.len() + MAX_STATUS_LINE_LENGTH, 0);
    }

    let hoot_res = HootResponse::resume(token, &mut write_buf);

    let out = hoot_res
        .send_status(response.status().as_u16(), response.status().as_str())?
        .flush();

    writer.write_all(&out)?;

    let mut token = out.ready();

    for header in response.headers() {
        if header.0.eq("content-length") || header.0.eq("transfer-encoding") {
            // These headers are handled when selecting body output.
            continue;
        }

        let name = header.0.as_str();
        let bytes = header.1.as_bytes();

        let needed_size = name.as_bytes().len() + bytes.len() + 10;

        if write_buf.len() < needed_size {
            write_buf.resize(write_buf.len() + needed_size, 0);
        }

        let hoot_res = HootResponse::resume(token, &mut write_buf);

        let out = hoot_res
            .header_bytes(header.0.as_str(), header.1.as_bytes())?
            .flush();

        writer.write_all(&out)?;

        token = out.ready();
    }

    Ok(token)
}
