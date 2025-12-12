//! # Pokemon REST API
//!
//! A web service that enriches Pokémon data from [PokéAPI](https://pokeapi.co/) by applying
//! fun translations based on Pokémon characteristics using the [Fun Translations API](https://funtranslations.com/api/).
//!
//! ## Features
//!
//! - **Content Negotiation**: Supports HTTP `Accept-Language` header for multi-language descriptions
//! - **OpenAPI Integration**: Auto-generated API documentation with Swagger UI
//! - **Prometheus Metrics**: Built-in metrics endpoint for monitoring
//! - **Distributed Tracing**: Structured logging with tracing spans for observability
//! - **Health Checks**: Dedicated `/health` endpoint for service availability checks
//!
//! ## Architecture
//!
//! The application uses a layered architecture:
//! - **HTTP Layer** (`http::client`): HTTP client wrapper for external APIs
//! - **Pokemon API Layer** (`pokemon_api::client`): PokéAPI integration with language negotiation
//! - **Translator Layer** (`translator::client`): Fun Translations API integration
//! - **Metrics Layer** (`metrics`): Prometheus metrics collection
//! - **Configuration Layer** (`config`): CLI/env configuration management
//!
//! ## Request Flow
//!
//! 1. Client sends request to `/pokemon/{name}` with optional `Accept-Language` header
//! 2. Handler creates a tracing span for request tracking
//! 3. Pokemon API client fetches base data and species information
//! 4. Language negotiation selects best available language
//! 5. Description is returned with `Content-Language` header
//! 6. Metrics are incremented for monitoring

use accept_language::parse;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{AppendHeaders, IntoResponse, Json, Response},
};
use hyper::{header::CONTENT_LANGUAGE, HeaderMap};
use std::{process::exit, sync::Arc};
use tracing::{debug, info, warn};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_swagger_ui::{Config, SwaggerUi};

mod config;
mod constants;
mod http;
mod metrics;
mod pokemon_api;
mod translator;

use pokemon_api::client::{
    PokeApiClient, Pokemon, PokemonApi, PokemonApiProxy, PokemonApiProxyClient,
};
use translator::client::{FunTranslator, Translator};

use crate::{config::ConfigDescriptor, constants::DEFAULT_LANGUAGE, http::client::HttpClientError};

/// Extension trait for parsing `Accept-Language` HTTP headers with quality values.
///
/// Supports RFC 7231 language tags with quality preferences and wildcard matching.
/// Example: `"es;q=0.9,en;q=0.8,*"` returns `(["es", "en"], true)`
trait AcceptLanguageExt {
    /// Parses the `Accept-Language` header into a tuple of (languages, has_wildcard).
    ///
    /// Returns empty language list with wildcard=true if no header is present.
    fn parse_accept_language(&self) -> (Vec<String>, bool);
}

impl AcceptLanguageExt for HeaderMap {
    fn parse_accept_language(&self) -> (Vec<String>, bool) {
        self.get("accept-language")
            .and_then(|h| h.to_str().ok())
            .map(|header_value| {
                let langs = parse(header_value);
                let has_wildcard: bool = langs.iter().any(|l| l == "*");
                let filtered_langs: Vec<String> = langs.into_iter().filter(|l| l != "*").collect();
                (filtered_langs, has_wildcard)
            })
            .unwrap_or_else(|| (vec![], true))
    }
}

/// OpenAPI documentation schema for the Pokémon API.
///
/// Automatically generates Swagger UI from this definition and serves it at `/swagger-ui`.
#[derive(OpenApi)]
#[openapi(
    paths(
        get_pokemon,
        get_pokemon_translation,
        health,
        metrics_endpoint
    ),
    components(
        schemas(Pokemon)
    ),
    tags(
        (name = "pokemon", description = "Pokemon API endpoints"),
        (name = "system", description = "Service health and metrics endpoints")
    ),
    info(
        title = "Pokemon API",
        version = "0.1.0",
        description = "API for fetching Pokemon information and translations"
    )
)]
struct ApiDoc;

/// Application state containing shared dependencies.
///
/// This is passed to all request handlers and contains:
/// - `pokemon_api`: Client for fetching Pokémon data with language negotiation
/// - `fun_translator`: Client for translating descriptions via Fun Translations API
#[derive(Clone)]
struct AppState {
    pokemon_api: std::sync::Arc<dyn PokemonApi>,
    fun_translator: std::sync::Arc<dyn Translator>,
}

/// HTTP response enum supporting multiple content types and language headers.
///
/// Variants:
/// - `Success(lang, T)`: 200 OK with Content-Language header
/// - `NotFound`: 404 Not Found
/// - `InternalError`: 500 Internal Server Error
enum HttpResponse<T> {
    Success(String, T),
    NotFound,
    InternalError,
    ServiceUnavailable,
}

struct JsonResponse<T>(T);

/// Helper wrapper for JSON responses with transparent serialization.
impl<T: serde::Serialize> IntoResponse for HttpResponse<JsonResponse<T>> {
    /// Converts HttpResponse to axum Response with appropriate HTTP status and headers.
    fn into_response(self) -> Response {
        match self {
            HttpResponse::Success(lang, JsonResponse(data)) => (
                StatusCode::OK,
                AppendHeaders([(CONTENT_LANGUAGE, lang)]),
                Json(data),
            )
                .into_response(),
            HttpResponse::NotFound => StatusCode::NOT_FOUND.into_response(),
            HttpResponse::InternalError => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            HttpResponse::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        }
    }
}

/// Helper wrapper for plain text responses.
impl IntoResponse for HttpResponse<String> {
    /// Converts HttpResponse to axum Response with appropriate HTTP status and headers.
    fn into_response(self) -> Response {
        match self {
            HttpResponse::Success(lang, data) => (
                StatusCode::OK,
                AppendHeaders([(CONTENT_LANGUAGE, lang)]),
                data,
            )
                .into_response(),
            HttpResponse::NotFound => StatusCode::NOT_FOUND.into_response(),
            HttpResponse::InternalError => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            HttpResponse::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        }
    }
}

impl<T> From<HttpClientError> for HttpResponse<T> {
    fn from(error: HttpClientError) -> Self {
        match error {
            HttpClientError::NotFound => HttpResponse::NotFound,
            HttpClientError::ServiceUnavailable => HttpResponse::ServiceUnavailable,
            _ => HttpResponse::InternalError,
        }
    }
}

/// Application entry point.
///
/// Initializes tracing, metrics, and configuration, then starts the HTTP server.
///
/// # Configuration
///
/// Configuration is loaded from environment variables and CLI arguments (CLI takes precedence).
/// See `config::AppConfig::load()` for details.
///
/// # Tracing
///
/// Structured logging is initialized with `tracing-subscriber` using the `RUST_LOG` environment variable.
/// Default level is INFO.
///
/// # Errors
///
/// Returns an error if configuration fails or if the server cannot bind to the configured port.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = match config::AppConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("configuration error: {}\n", e);
            ConfigDescriptor::print_usage();
            exit(1);
        }
    };
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from(config.rust_log.as_str())
                .add_directive(tracing_subscriber::filter::LevelFilter::INFO.into()),
        )
        .init();

    info!("Starting Pokemon API server");

    metrics::init();

    let pokeapi_base_client = Box::new(PokemonApiProxyClient::new(
        reqwest::Client::new(),
        config.pokeapi_base_url(),
    )) as Box<dyn PokemonApiProxy + Send + Sync>;
    let pokemon_api = Arc::new(PokeApiClient::new(pokeapi_base_client)) as Arc<dyn PokemonApi>;
    let fun_translator = Arc::new(FunTranslator::new(
        reqwest::Client::new(),
        config.fun_translations_base_url(),
    )) as Arc<dyn Translator>;
    let state = AppState {
        pokemon_api,
        fun_translator,
    };

    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(get_pokemon))
        .routes(routes!(get_pokemon_translation))
        .routes(routes!(health))
        .routes(routes!(metrics_endpoint))
        .split_for_parts();

    let app = router
        .merge(
            SwaggerUi::new("/swagger-ui")
                .config(Config::default().validator_url("none"))
                .url("/api-docs/openapi.json", api.clone()),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port)).await?;
    info!("Server listening on 0.0.0.0:{}", config.port);
    axum::serve(listener, app).await?;
    Ok(())
}

/// Fetches Pokémon information with language negotiation.
///
/// # Arguments
///
/// * `state` - Application state containing Pokemon API client and translator
/// * `name` - Pokémon name to fetch
/// * `headers` - HTTP headers including optional `Accept-Language`
///
/// # Returns
///
/// Returns 200 OK with Pokemon data and Content-Language header on success,
/// 404 Not Found if the Pokémon doesn't exist or name is empty,
/// or 500 Internal Server Error on unexpected failures.
///
/// # Language Negotiation
///
/// Respects the `Accept-Language` header (RFC 7231) for selecting response language.
/// Falls back to English if requested language is unavailable and wildcard is present,
/// or returns 406 Not Acceptable if no suitable language is found.
///
/// # Tracing
///
/// Creates a distributed tracing span `get_pokemon` with pokemon_name field.
/// Logs base Pokemon fetch, species fetch, and language selection at debug level.
#[utoipa::path(
    get,
    path = "/pokemon/{name}",
    tag = "pokemon",
    description = "Fetches Pokemon information with language negotiation",
    params(
        ("name" = String, Path, description = "Pokemon name"),
        ("accept-language" = Option<String>, Header, description = "Preferred language(s) for Pokemon description (e.g., 'en', 'es', 'fr'). Supports multiple languages with quality values (e.g., 'es;q=0.9,en;q=0.8'). Use '*' to accept any available language.")
    ),
    responses(
        (status = 200, description = "Pokemon found", body = Pokemon, headers(
            ("Content-Language" = String, description = "Language of the returned Pokemon description")
        )),
        (status = 404, description = "Pokemon not found"),
        (status = 406, description = "No acceptable language found for Pokemon description"),
        (status = 503, description = "Service unavailable"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_pokemon(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> HttpResponse<JsonResponse<Pokemon>> {
    let span = tracing::info_span!("get_pokemon", pokemon_name = %name);
    let _guard = span.enter();

    if name.trim().is_empty() {
        warn!("Empty pokemon name requested");
        return HttpResponse::NotFound;
    }

    debug!("Fetching pokemon: {}", name);
    metrics::POKEMON_REQUESTS_TOTAL.inc();

    let (languages, has_wildcard) = headers.parse_accept_language();
    let result = state
        .pokemon_api
        .get_pokemon(&name, &languages, has_wildcard)
        .await
        .map(|(lang, p)| HttpResponse::Success(lang, JsonResponse(p)))
        .unwrap_or_else(Into::into);

    match &result {
        HttpResponse::Success(lang, _) => {
            metrics::POKEMON_REQUESTS_FOUND.inc();
            info!(
                pokemon = name,
                language = lang,
                "Successfully fetched pokemon"
            );
        }
        HttpResponse::NotFound => {
            metrics::POKEMON_REQUESTS_NOT_FOUND.inc();
            debug!(pokemon = name, "Pokemon not found");
        }
        HttpResponse::ServiceUnavailable => {
            metrics::SERVICE_UNAVAILABLE_ERRORS.inc();
            warn!(pokemon = name, "Pokemon service unavailable");
        }
        _ => {}
    }

    result
}

/// Fetches and translates a Pokémon's description.
///
/// # Arguments
///
/// * `state` - Application state containing Pokemon API client and translator
/// * `name` - Pokémon name to fetch and translate
///
/// # Returns
///
/// Returns 200 OK with translated description and Content-Language header on success,
/// 404 Not Found if the Pokémon doesn't exist, name is empty, or has no description,
/// or 500 Internal Server Error on translation or API failures.
///
/// # Translation Process
///
/// 1. Fetches Pokémon data from PokéAPI (using DEFAULT_LANGUAGE: English)
/// 2. Extracts description text and determines translator type from Pokémon species
/// 3. Sends description to Fun Translations API with appropriate translator
/// 4. Returns translated text as plain text (text/plain)
///
/// # Tracing
///
/// Creates a distributed tracing span `get_pokemon_translation` with pokemon_name field.
/// Logs Pokemon API calls and translation attempts at debug level.
#[utoipa::path(
    get,
    path = "/pokemon/{name}/translation/",
    tag = "pokemon",
    description = "Fetches and translates a Pokemon's description",
    params(
        ("name" = String, Path, description = "Pokemon name")
    ),
    responses(
        (status = 200, description = "Translated Pokemon description", body = String, headers(
            ("Content-Language" = String, description = "Language of the returned translated description")
        )),
        (status = 404, description = "Pokemon not found"),
        (status = 406, description = "No acceptable language found for Pokemon description"),
        (status = 500, description = "Internal server error"),
        (status = 503, description = "Service unavailable"),
    )
)]
async fn get_pokemon_translation(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> HttpResponse<String> {
    let span = tracing::info_span!("get_pokemon_translation", pokemon_name = %name);
    let _guard = span.enter();

    if name.trim().is_empty() {
        warn!("Empty pokemon name requested for translation");
        return HttpResponse::NotFound;
    }

    debug!("Translating pokemon description for: {}", name);
    metrics::TRANSLATIONS_TOTAL.inc();

    let response = match state
        .pokemon_api
        .get_pokemon(&name, &[DEFAULT_LANGUAGE.to_string()], false)
        .await
        .and_then(|(lang, p)| {
            let translator = p.get_translator();
            p.description
                .map(|d| (lang, d, translator))
                .ok_or(HttpClientError::NotFound)
        })
        .map(|(lang, d, t)| async move {
            match state.fun_translator.translate(&d, t).await {
                Ok(tr) => Ok((lang, tr.contents.translated)),
                Err(HttpClientError::RateLimited) => {
                    metrics::RATE_LIMITED_ERRORS.inc();
                    Err(HttpClientError::RateLimited)
                }
                Err(e) => Err(e),
            }
        }) {
        Ok(f) => f
            .await
            .map(|(lang, text)| HttpResponse::Success(lang, text))
            .unwrap_or_else(Into::into),
        Err(e) => e.into(),
    };

    match &response {
        HttpResponse::Success(_, _) => {
            metrics::TRANSLATIONS_SUCCEEDED.inc();
            info!(
                pokemon = name,
                "Successfully translated pokemon description"
            );
        }
        HttpResponse::NotFound => {
            metrics::TRANSLATIONS_FAILED.inc();
            debug!(pokemon = name, "Pokemon not found for translation");
        }
        HttpResponse::ServiceUnavailable => {
            metrics::SERVICE_UNAVAILABLE_ERRORS.inc();
            metrics::TRANSLATIONS_FAILED.inc();
            warn!(pokemon = name, "Translation service unavailable");
        }
        _ => {
            metrics::TRANSLATIONS_FAILED.inc();
            warn!(pokemon = name, "Translation failed");
        }
    }

    response
}

/// Health check endpoint for monitoring and orchestration systems.
///
/// Returns 200 OK immediately without performing any checks.
/// Used by Kubernetes probes, load balancers, and monitoring systems to verify service availability.
///
/// # Example
///
/// ```sh
/// curl http://localhost:5000/health
/// # Response: 200 OK (empty body)
/// ```
#[utoipa::path(
    get,
    path = "/health",
    description = "Health check endpoint",
    tag = "system",
    responses((status = 200, description = "Service is healthy"))
)]
async fn health() -> impl IntoResponse {
    StatusCode::OK
}

/// Prometheus metrics endpoint.
///
/// Exposes all application metrics in Prometheus text format (version 0.0.4).
/// Metrics include request counts, latencies, translation successes/failures, and more.
///
/// # Metrics Exposed
///
/// - `pokemon_requests_total` - Total Pokemon data requests
/// - `pokemon_requests_found` - Successful Pokemon requests
/// - `pokemon_requests_not_found` - Pokemon not found (404) responses
/// - `translations_total` - Total translation requests
/// - `translations_succeeded` - Successful translations
/// - `translations_failed` - Failed translations
/// - `http_requests_total` - Total HTTP requests by endpoint/method
/// - `http_request_duration_seconds` - Request duration histogram
///
/// # Example
///
/// ```sh
/// curl http://localhost:5000/metrics
/// # Response: Prometheus text format metrics
/// ```
#[utoipa::path(
    get,
    path = "/metrics",
    description = "Prometheus metrics endpoint",
    tag = "system",
    responses((status = 200, description = "Prometheus format metrics"))
)]
async fn metrics_endpoint() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("Content-Type", "text/plain; version=0.0.4")],
        prometheus::TextEncoder::new()
            .encode_to_string(&metrics::REGISTRY.gather())
            .unwrap_or_default(),
    )
}
