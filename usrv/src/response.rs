use std::convert::Infallible;

use crate::{Body, Response};

pub trait IntoResponse {
    fn into_response(self) -> Response;
}

pub struct NotFound;

impl IntoResponse for NotFound {
    fn into_response(self) -> Response {
        http::Response::builder()
            .status(404)
            .body(Body::Empty)
            .unwrap()
    }
}

impl IntoResponse for Infallible {
    fn into_response(self) -> Response {
        panic!("IntoResponse for Infallible");
    }
}
