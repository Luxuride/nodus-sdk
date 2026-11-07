use super::{error_message, status_error};
use nodus_instance_api::Error;
use reqwest::StatusCode;

#[test]
fn a_401_stops_sync() {
    assert!(matches!(
        status_error(StatusCode::UNAUTHORIZED, None, b""),
        Error::Unauthorized
    ));
    assert!(matches!(
        status_error(StatusCode::FORBIDDEN, None, b""),
        Error::Unauthorized
    ));
}

#[test]
fn a_429_carries_the_instances_delay() {
    match status_error(StatusCode::TOO_MANY_REQUESTS, Some(120), b"") {
        Error::RateLimited { retry_after_s } => assert_eq!(retry_after_s, 120),
        other => panic!("expected a rate limit, got {other:?}"),
    }
}

#[test]
fn a_429_without_a_header_falls_back_to_the_documented_window() {
    match status_error(StatusCode::TOO_MANY_REQUESTS, None, b"") {
        Error::RateLimited { retry_after_s } => assert_eq!(retry_after_s, 300),
        other => panic!("expected a rate limit, got {other:?}"),
    }
}

#[test]
fn an_error_body_is_read_from_the_error_field() {
    assert_eq!(
        error_message(br#"{"error":"Record not found"}"#),
        "Record not found"
    );
}

#[test]
fn an_html_error_body_is_trimmed() {
    let body = format!("<html>{}</html>", "x".repeat(500));
    let message = error_message(body.as_bytes());
    // Counted in chars, not bytes: the ellipsis is three bytes of UTF-8, and a
    // byte length would report a body that is already short enough.
    assert_eq!(message.chars().count(), super::MAX_ERROR_BODY + 1);
    assert!(message.ends_with('…'));
}

#[test]
fn a_503_becomes_a_status_error() {
    match status_error(StatusCode::SERVICE_UNAVAILABLE, None, b"down") {
        Error::Status { status, message } => {
            assert_eq!(status, 503);
            assert_eq!(message, "down");
        }
        other => panic!("expected a status error, got {other:?}"),
    }
}
