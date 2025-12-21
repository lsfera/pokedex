//! # Fun Translations API Client
//!
//! This module provides integration with the [Fun Translations API](https://funtranslations.com/api/)
//! for translating Pokémon descriptions into various fun styles:
//! - **Shakespeare**: Elizabethan English translation
//! - **Yoda**: Star Wars Yoda speak translation
//!
//! ## Rate Limiting
//!
//! The Fun Translations API has rate limits. The client handles rate limiting errors
//! gracefully by returning `HttpClientError::RateLimited`.

use crate::http::client::{HttpClientError, TranslatorType};
use reqwest::StatusCode;
use serde::Deserialize;

/// Response from Fun Translations API.
///
/// Contains metadata and the translated text.
#[derive(Debug, Deserialize)]
pub struct TranslationResponse {
    pub contents: TranslationContents,
}

#[derive(Debug, Deserialize)]
pub struct TranslationContents {
    /// The translated text in the requested translator style
    pub translated: String,
}

/// Trait for translating text using various fun styles.
#[async_trait::async_trait]
pub trait Translator: Send + Sync {
    /// Translates text using the specified translator style.
    ///
    /// # Arguments
    ///
    /// * `text` - Text to translate
    /// * `translator_type` - Style to use (Shakespeare or Yoda)
    ///
    /// # Returns
    ///
    /// Returns the translated text on success.
    ///
    /// # Errors
    ///
    /// - `NotFound` if the translator type endpoint doesn't exist (404)
    /// - `RateLimited` if API rate limit is exceeded (429)
    /// - `RequestFailed` on network errors
    /// - `ParseError` on JSON parsing or server errors
    async fn translate(
        &self,
        text: &str,
        translator_type: TranslatorType,
    ) -> Result<TranslationResponse, HttpClientError>;
}
/// HTTP client for the Fun Translations API.
///
/// Handles translation requests using the Fun Translations API endpoints.
pub struct FunTranslator {
    client: reqwest::Client,
    base_url: String,
}

impl FunTranslator {
    /// Creates a new Fun Translator client.
    ///
    /// # Arguments
    ///
    /// * `client` - Configured reqwest client
    /// * `base_url` - Base URL for Fun Translations API (e.g., `https://api.funtranslations.com/translate`)
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        FunTranslator { client, base_url }
    }
}

#[async_trait::async_trait]
impl Translator for FunTranslator {
    /// Translates text using the Fun Translations API.
    ///
    /// Sends a POST request to the appropriate translator endpoint with the text
    /// as form-encoded data.
    ///
    /// # Rate Limiting
    ///
    /// The API allows 5 requests per hour for free tier. Exceeding this returns
    /// a 429 Too Many Requests error.
    async fn translate(
        &self,
        text: &str,
        translator_type: TranslatorType,
    ) -> Result<TranslationResponse, HttpClientError> {
        self.client
            .post(format!("{}/{}.json", self.base_url, translator_type,))
            .form(&[("text", text)])
            .send()
            .await
            .map_err(|_| HttpClientError::RequestFailed)
            .and_then(|r| match r.status() {
                StatusCode::NOT_FOUND => Err(HttpClientError::NotFound),
                StatusCode::SERVICE_UNAVAILABLE => Err(HttpClientError::ServiceUnavailable),
                StatusCode::TOO_MANY_REQUESTS => Err(HttpClientError::RateLimited),
                StatusCode::INTERNAL_SERVER_ERROR => Err(HttpClientError::ServerError),
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
            .mock("POST", "/shakespeare.json")
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body("text=Hello")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"success":{"total":1},"contents":{"translation":"shakespeare","text":"Hello","translated":"Hark, Hello"}}"#)
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
            .mock("POST", "/yoda.json")
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body("text=Hello")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"success":{"total":1},"contents":{"translation":"yoda","text":"Hello","translated":"Hello, you say must"}}"#)
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
            .mock("POST", "/shakespeare.json")
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body("text=Unknown")
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
            .mock("POST", "/shakespeare.json")
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body("text=Unknown")
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
    async fn returns_service_unavailable_on_503() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/shakespeare.json")
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body("text=Hello")
            .with_status(503)
            .create_async()
            .await;

        let translator = FunTranslator::new(reqwest::Client::new(), server.url());

        let result = translator
            .translate("Hello", TranslatorType::Shakespeare)
            .await;

        assert!(matches!(result, Err(HttpClientError::ServiceUnavailable)));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn returns_parse_error_on_invalid_json() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/shakespeare.json")
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body("text=Hello")
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
    async fn returns_internal_server_error_on_server_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/shakespeare.json")
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body("text=Hello")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let translator = FunTranslator::new(reqwest::Client::new(), server.url());

        let result = translator
            .translate("Hello", TranslatorType::Shakespeare)
            .await;

        assert!(result.is_err());
        assert_eq!(
            result.is_err_and(|e| e == HttpClientError::ServerError),
            true
        );
        mock.assert_async().await;
    }

    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored test_translate_with_real_api
    async fn test_translate_with_real_api_shakespeare() {
        let translator = FunTranslator::new(
            reqwest::Client::new(),
            "https://api.funtranslations.com/translate".to_string(),
        );

        let result = translator
            .translate("Hello, how are you?", TranslatorType::Shakespeare)
            .await;

        // This test requires internet connectivity to the real API
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(!response.contents.translated.is_empty());
        // Shakespeare translation should differ from original
        assert_ne!(response.contents.translated, "Hello, how are you?");
    }
}
