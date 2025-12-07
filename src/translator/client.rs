use crate::http::client::{HttpClientError, TranslatorType};
use reqwest::StatusCode;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct TranslationResponse {
    pub contents: TranslationContents,
}

#[derive(Debug, Deserialize)]
pub struct TranslationContents {
    pub translated: String,
}

#[async_trait::async_trait]
pub trait Translator: Send + Sync {
    async fn translate(
        &self,
        text: &str,
        translator_type: TranslatorType,
    ) -> Result<TranslationResponse, HttpClientError>;
}
pub struct FunTranslator {
    client: reqwest::Client,
    base_url: String,
}

impl FunTranslator {
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        FunTranslator { client, base_url }
    }
}

#[async_trait::async_trait]
impl Translator for FunTranslator {
    async fn translate(
        &self,
        text: &str,
        translator_type: TranslatorType,
    ) -> Result<TranslationResponse, HttpClientError> {
        self.client
            .get(format!(
                "{}/{}?text={}",
                self.base_url, translator_type, text
            ))
            .send()
            .await
            .map_err(|_| HttpClientError::RequestFailed)
            .and_then(|r| match r.status() {
                StatusCode::NOT_FOUND => Err(HttpClientError::NotFound),
                StatusCode::TOO_MANY_REQUESTS => Err(HttpClientError::RateLimited),
                // NOTE: by default redirects followed automatically by reqwest::Client: https://docs.rs/reqwest/latest/reqwest/#redirect-policies
                _ => Ok(r),
            })?
            .json::<TranslationResponse>()
            .await
            .map_err(|_| HttpClientError::ParseError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn translates_text_successfully_with_shakespeare() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/shakespeare?text=Hello")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"contents": {"translated": "Hark, Hello"}}"#)
            .create_async()
            .await;

        let translator = FunTranslator::new(reqwest::Client::new(), server.url());

        let result = translator
            .translate("Hello", TranslatorType::Shakespeare)
            .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.contents.translated, "Hark, Hello");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn translates_text_successfully_with_yoda() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/yoda?text=Hello")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"contents": {"translated": "Hello, you say must"}}"#)
            .create_async()
            .await;

        let translator = FunTranslator::new(reqwest::Client::new(), server.url());

        let result = translator.translate("Hello", TranslatorType::Yoda).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.contents.translated, "Hello, you say must");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn returns_not_found_on_404() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/shakespeare?text=Unknown")
            .with_status(404)
            .create_async()
            .await;

        let translator = FunTranslator::new(reqwest::Client::new(), server.url());

        let result = translator
            .translate("Unknown", TranslatorType::Shakespeare)
            .await;

        assert!(matches!(result, Err(HttpClientError::NotFound)));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn returns_rate_limited_on_429() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/shakespeare?text=Unknown")
            .with_status(429)
            .create_async()
            .await;

        let translator = FunTranslator::new(reqwest::Client::new(), server.url());

        let result = translator
            .translate("Unknown", TranslatorType::Shakespeare)
            .await;

        assert!(matches!(result, Err(HttpClientError::RateLimited)));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn returns_parse_error_on_invalid_json() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/shakespeare?text=Hello")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("invalid json")
            .create_async()
            .await;

        let translator = FunTranslator::new(reqwest::Client::new(), server.url());

        let result = translator
            .translate("Hello", TranslatorType::Shakespeare)
            .await;

        assert!(matches!(result, Err(HttpClientError::ParseError)));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn returns_parse_error_on_server_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/shakespeare?text=Hello")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let translator = FunTranslator::new(reqwest::Client::new(), server.url());

        let result = translator
            .translate("Hello", TranslatorType::Shakespeare)
            .await;

        assert!(matches!(result, Err(HttpClientError::ParseError)));
        mock.assert_async().await;
    }
}
