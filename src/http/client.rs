use std::fmt::{self, Formatter};

#[derive(Debug, PartialEq)]
pub enum TranslatorType {
    Shakespeare,
    Yoda,
}

impl fmt::Display for TranslatorType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TranslatorType::Shakespeare => write!(f, "shakespeare"),
            TranslatorType::Yoda => write!(f, "yoda"),
        }
    }
}

#[derive(Debug)]
pub enum HttpClientError {
    NotAcceptable,
    NotFound,
    RateLimited,
    RequestFailed,
    ParseError,
    ServiceUnavailable,
}

impl std::fmt::Display for HttpClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpClientError::NotAcceptable => write!(f, "not acceptable"),
            HttpClientError::NotFound => write!(f, "resource not found"),
            HttpClientError::RequestFailed => write!(f, "request failed"),
            HttpClientError::ParseError => write!(f, "failed to parse response"),
            HttpClientError::RateLimited => write!(f, "rate limited by the server"),
            HttpClientError::ServiceUnavailable => write!(f, "service unavailable"),
        }
    }
}

impl std::error::Error for HttpClientError {}
