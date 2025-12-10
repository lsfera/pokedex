use accept_language::parse;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{AppendHeaders, IntoResponse, Json, Response},
};
use hyper::{header::CONTENT_LANGUAGE, HeaderMap};
use std::{process::exit, sync::Arc};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_swagger_ui::{Config, SwaggerUi};

mod config;
mod constants;
mod http;
mod pokemon_api;
mod translator;

use pokemon_api::client::{
    PokeApiClient, Pokemon, PokemonApi, PokemonApiProxy, PokemonApiProxyClient,
};
use translator::client::{FunTranslator, Translator};

use crate::{config::ConfigDescriptor, constants::DEFAULT_LANGUAGE, http::client::HttpClientError};

trait AcceptLanguageExt {
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

#[derive(OpenApi)]
#[openapi(
    paths(
        get_pokemon,
        get_pokemon_translation,
        health
    ),
    components(
        schemas(Pokemon)
    ),
    tags(
        (name = "pokemon", description = "Pokemon API endpoints"),
        (name = "system", description = "Service health endpoints")
    ),
    info(
        title = "Pokemon API",
        version = "0.1.0",
        description = "API for fetching Pokemon information and translations"
    )
)]
struct ApiDoc;

#[derive(Clone)]
struct AppState {
    pokemon_api: std::sync::Arc<dyn PokemonApi>,
    fun_translator: std::sync::Arc<dyn Translator>,
}

enum HttpResponse<T> {
    Success(String, T),
    NotFound,
    InternalError,
}

struct JsonResponse<T>(T);

impl<T: serde::Serialize> IntoResponse for HttpResponse<JsonResponse<T>> {
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
        }
    }
}

impl IntoResponse for HttpResponse<String> {
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
        }
    }
}

impl<T> From<HttpClientError> for HttpResponse<T> {
    fn from(error: HttpClientError) -> Self {
        match error {
            HttpClientError::NotFound => HttpResponse::NotFound,
            _ => HttpResponse::InternalError,
        }
    }
}

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
        .split_for_parts();

    let app = router
        .merge(
            SwaggerUi::new("/swagger-ui")
                .config(Config::default().validator_url("none"))
                .url("/api-docs/openapi.json", api.clone()),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[utoipa::path(
    get,
    path = "/pokemon/{name}",
    tag = "pokemon",
    params(
        ("name" = String, Path, description = "Pokemon name"),
        ("accept-language" = Option<String>, Header, description = "Preferred language(s) for Pokemon description (e.g., 'en', 'es', 'fr'). Supports multiple languages with quality values (e.g., 'es;q=0.9,en;q=0.8'). Use '*' to accept any available language.")
    ),
    responses(
        (status = 200, description = "Pokemon found", body = Pokemon, headers(
            ("Content-Language" = String, description = "Language of the returned Pokemon description")
        )),
        (status = 404, description = "Pokemon not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_pokemon(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> HttpResponse<JsonResponse<Pokemon>> {
    if name.trim().is_empty() {
        return HttpResponse::NotFound;
    }
    let (languages, has_wildcard) = headers.parse_accept_language();
    state
        .pokemon_api
        .get_pokemon(&name, &languages, has_wildcard)
        .await
        .map(|(lang, p)| HttpResponse::Success(lang, JsonResponse(p)))
        .unwrap_or_else(Into::into)
}

#[utoipa::path(
    get,
    path = "/pokemon/{name}/translation/",
    tag = "pokemon",
    params(
        ("name" = String, Path, description = "Pokemon name")
    ),
    responses(
        (status = 200, description = "Translated Pokemon description", body = String, headers(
            ("Content-Language" = String, description = "Language of the returned translated description")
        )),
        (status = 404, description = "Pokemon not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_pokemon_translation(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> HttpResponse<String> {
    if name.trim().is_empty() {
        return HttpResponse::NotFound;
    }
    match state
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
            state
                .fun_translator
                .translate(&d, t)
                .await
                .map(|tr| (lang, tr.contents.translated))
        }) {
        Ok(f) => f
            .await
            .map(|(lang, text)| HttpResponse::Success(lang, text))
            .unwrap_or_else(Into::into),
        Err(e) => e.into(),
    }
}

#[utoipa::path(
    get,
    path = "/health",
    tag = "system",
    responses((status = 200, description = "Service is healthy"))
)]
async fn health() -> impl IntoResponse {
    StatusCode::OK
}
