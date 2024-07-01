use http::{Method, Response, StatusCode};

use super::scenario::Scenario;

#[test]
fn without_recv_body() {
    let scenario = Scenario::builder()
        .get("https://a.test")
        .redirect(StatusCode::FOUND, "https://b.test")
        .build();

    scenario.to_redirect();
}

#[test]
fn with_recv_body() {
    let scenario = Scenario::builder()
        .get("https://a.test")
        .redirect(StatusCode::FOUND, "https://b.test")
        .recv_body(b"hi there", false)
        .build();

    scenario.to_redirect();
}

#[test]
fn absolute_url() {
    let scenario = Scenario::builder()
        .get("https://a.test")
        .redirect(StatusCode::FOUND, "https://b.test")
        .build();

    let state = scenario.to_redirect().as_new_state().unwrap().unwrap();

    assert_eq!(&state.uri().to_string(), "https://b.test/");
}

#[test]
fn relative_url_absolute_path() {
    let scenario = Scenario::builder()
        .get("https://a.test")
        .redirect(StatusCode::FOUND, "/foo.html")
        .build();

    let state = scenario.to_redirect().as_new_state().unwrap().unwrap();

    assert_eq!(&state.uri().to_string(), "https://a.test/foo.html");
}

#[test]
fn relative_url_relative_path() {
    let scenario = Scenario::builder()
        .get("https://a.test/x/foo.html")
        .redirect(StatusCode::FOUND, "y/bar.html")
        .build();

    let state = scenario.to_redirect().as_new_state().unwrap().unwrap();

    assert_eq!(&state.uri().to_string(), "https://a.test/x/y/bar.html");
}

#[test]
fn last_location_header() {
    let scenario = Scenario::builder()
        .get("https://a.test")
        .response(
            Response::builder()
                .status(StatusCode::MOVED_PERMANENTLY)
                .header("location", "https://b.test")
                .header("location", "https://c.test")
                .header("location", "https://d.test")
                .header("location", "https://e.test")
                .body(())
                .unwrap(),
        )
        .build();

    let state = scenario.to_redirect().as_new_state().unwrap().unwrap();

    assert_eq!(&state.uri().to_string(), "https://e.test/");
}

const METHOD_CHANGES: &[(StatusCode, &[(Method, Option<Method>)])] = &[
    (
        StatusCode::FOUND,
        &[
            (Method::GET, Some(Method::GET)),
            (Method::HEAD, Some(Method::HEAD)),
            (Method::POST, Some(Method::GET)),
            (Method::PUT, Some(Method::GET)),
            (Method::PATCH, Some(Method::GET)),
            (Method::DELETE, Some(Method::GET)),
            (Method::OPTIONS, Some(Method::GET)),
            (Method::CONNECT, Some(Method::GET)),
            (Method::TRACE, Some(Method::GET)),
        ],
    ),
    (
        StatusCode::MOVED_PERMANENTLY,
        &[
            (Method::GET, Some(Method::GET)),
            (Method::HEAD, Some(Method::HEAD)),
            (Method::POST, Some(Method::GET)),
            (Method::PUT, Some(Method::GET)),
            (Method::PATCH, Some(Method::GET)),
            (Method::DELETE, Some(Method::GET)),
            (Method::OPTIONS, Some(Method::GET)),
            (Method::CONNECT, Some(Method::GET)),
            (Method::TRACE, Some(Method::GET)),
        ],
    ),
    (
        StatusCode::TEMPORARY_REDIRECT,
        &[
            (Method::GET, Some(Method::GET)),
            (Method::HEAD, Some(Method::HEAD)),
            (Method::POST, None),
            (Method::PUT, None),
            (Method::PATCH, None),
            (Method::DELETE, None),
            (Method::OPTIONS, Some(Method::OPTIONS)),
            (Method::CONNECT, Some(Method::CONNECT)),
            (Method::TRACE, Some(Method::TRACE)),
        ],
    ),
    (
        StatusCode::PERMANENT_REDIRECT,
        &[
            (Method::GET, Some(Method::GET)),
            (Method::HEAD, Some(Method::HEAD)),
            (Method::POST, None),
            (Method::PUT, None),
            (Method::PATCH, None),
            (Method::DELETE, None),
            (Method::OPTIONS, Some(Method::OPTIONS)),
            (Method::CONNECT, Some(Method::CONNECT)),
            (Method::TRACE, Some(Method::TRACE)),
        ],
    ),
];

#[test]
fn change_redirect_methods() {
    for (status, methods) in METHOD_CHANGES {
        for (method_from, method_to) in methods.iter() {
            let scenario = Scenario::builder()
                .method(method_from.clone(), "https://a.test")
                .redirect(*status, "https://b.test")
                .build();

            let maybe_state = scenario.to_redirect().as_new_state().unwrap();
            if let Some(state) = maybe_state {
                let inner = state.inner();
                let method = inner.call.request().method();
                assert_eq!(
                    method,
                    method_to.clone().unwrap(),
                    "{} {} -> {:?}",
                    status,
                    method_from,
                    method_to
                );
            } else {
                assert!(method_to.is_none());
            }
        }
    }
}
