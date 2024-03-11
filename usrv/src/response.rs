use crate::{Body, IntoResponse, Response};

pub struct NotFound;

impl IntoResponse for NotFound {
    fn into_response(self) -> Response {
        http::Response::builder()
            .status(404)
            .body(Body::Empty)
            .unwrap()
    }
}
