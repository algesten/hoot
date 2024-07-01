use http::{Method, Response, StatusCode};

use crate::client::flow::RedirectAuthHeaders;
use crate::client::test::TestSliceExt;

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

    let flow = scenario
        .to_redirect()
        .as_new_state(RedirectAuthHeaders::Never)
        .unwrap()
        .unwrap();

    assert_eq!(&flow.uri().to_string(), "https://b.test/");
}

#[test]
fn relative_url_absolute_path() {
    let scenario = Scenario::builder()
        .get("https://a.test")
        .redirect(StatusCode::FOUND, "/foo.html")
        .build();

    let flow = scenario
        .to_redirect()
        .as_new_state(RedirectAuthHeaders::Never)
        .unwrap()
        .unwrap();

    assert_eq!(&flow.uri().to_string(), "https://a.test/foo.html");
}

#[test]
fn relative_url_relative_path() {
    let scenario = Scenario::builder()
        .get("https://a.test/x/foo.html")
        .redirect(StatusCode::FOUND, "y/bar.html")
        .build();

    let flow = scenario
        .to_redirect()
        .as_new_state(RedirectAuthHeaders::Never)
        .unwrap()
        .unwrap();

    assert_eq!(&flow.uri().to_string(), "https://a.test/x/y/bar.html");
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

    let flow = scenario
        .to_redirect()
        .as_new_state(RedirectAuthHeaders::Never)
        .unwrap()
        .unwrap();

    assert_eq!(&flow.uri().to_string(), "https://e.test/");
}

#[test]
fn change_redirect_methods() {
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

    for (status, methods) in METHOD_CHANGES {
        for (method_from, method_to) in methods.iter() {
            let scenario = Scenario::builder()
                .method(method_from.clone(), "https://a.test")
                .redirect(*status, "https://b.test")
                .build();

            let maybe_state = scenario
                .to_redirect()
                .as_new_state(RedirectAuthHeaders::Never)
                .unwrap();
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

#[test]
fn keep_auth_header_never() {
    let scenario = Scenario::builder()
        .get("https://a.test/")
        .header("authorization", "some secret")
        .redirect(StatusCode::FOUND, "https://b.test/")
        .build();

    let mut flow = scenario
        .to_redirect()
        .as_new_state(RedirectAuthHeaders::Never)
        .unwrap()
        .unwrap()
        .proceed();

    let mut o = vec![0; 1024];

    let n = flow.write(&mut o).unwrap();
    assert_eq!(n, 32);

    let cmp = "\
            GET / HTTP/1.1\r\n\
            host: b.test\r\n\
            \r\n";
    assert_eq!(o[..n].as_str(), cmp);
}
