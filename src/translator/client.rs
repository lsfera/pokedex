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
        let response = self
            .client
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
            })?;

        let mut bytes = response
            .bytes()
            .await
            .map_err(|_| HttpClientError::ParseError)?
            .to_vec();

        simd_json::from_slice::<TranslationResponse>(&mut bytes)
            .map_err(|_| HttpClientError::ParseError)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use rstest::rstest;

    /// Parameterized test for successful translations with different translator types
    #[rstest]
    #[case::shakespeare(
        TranslatorType::Shakespeare,
        "/shakespeare.json",
        "Hello",
        r#"{"success":{"total":1},"contents":{"translation":"shakespeare","text":"Hello","translated":"Hark, Hello"}}"#,
        "Hark, Hello"
    )]
    #[case::yoda(
        TranslatorType::Yoda,
        "/yoda.json",
        "Hello",
        r#"{"success":{"total":1},"contents":{"translation":"yoda","text":"Hello","translated":"Hello, you say must"}}"#,
        "Hello, you say must"
    )]
    #[tokio::test]
    async fn translates_text_successfully(
        #[case] translator_type: TranslatorType,
        #[case] endpoint: &str,
        #[case] input_text: &str,
        #[case] response_body: &str,
        #[case] expected_translation: &str,
    ) {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", endpoint)
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body(format!("text={}", input_text).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(response_body)
            .create_async()
            .await;

        let translator = FunTranslator::new(reqwest::Client::new(), server.url());

        let result = translator.translate(input_text, translator_type).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.contents.translated, expected_translation);
        mock.assert_async().await;
    }

    /// Parameterized test for HTTP error status codes
    #[rstest]
    #[case::not_found(404, HttpClientError::NotFound)]
    #[case::rate_limited(429, HttpClientError::RateLimited)]
    #[case::service_unavailable(503, HttpClientError::ServiceUnavailable)]
    #[case::server_error(500, HttpClientError::ServerError)]
    #[tokio::test]
    async fn returns_error_on_http_status(
        #[case] status_code: usize,
        #[case] expected_error: HttpClientError,
    ) {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/shakespeare.json")
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body("text=TestInput")
            .with_status(status_code)
            .create_async()
            .await;

        let translator = FunTranslator::new(reqwest::Client::new(), server.url());

        let result = translator
            .translate("TestInput", TranslatorType::Shakespeare)
            .await;

        assert!(matches!(&result, Err(e) if *e == expected_error));
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
