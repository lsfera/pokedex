use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use std::{process::exit, sync::Arc};

mod config;
mod constants;
mod http;
mod pokemon_api;
mod translator;

use pokemon_api::client::{
    PokeApiClient, Pokemon, PokemonApi, PokemonApiProxy, PokemonApiProxyClient,
};
use translator::client::{FunTranslator, Translator};

use crate::{config::ConfigDescriptor, http::client::HttpClientError};

#[derive(Clone)]
struct AppState {
    pokemon_api: std::sync::Arc<dyn PokemonApi>,
    fun_translator: std::sync::Arc<dyn Translator>,
}

enum HttpResponse<T> {
    Success(T),
    NotFound,
    InternalError,
}

struct JsonResponse<T>(T);

impl<T: serde::Serialize> IntoResponse for HttpResponse<JsonResponse<T>> {
    fn into_response(self) -> Response {
        match self {
            HttpResponse::Success(JsonResponse(data)) => {
                (StatusCode::OK, Json(data)).into_response()
            }
            HttpResponse::NotFound => StatusCode::NOT_FOUND.into_response(),
            HttpResponse::InternalError => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

impl IntoResponse for HttpResponse<String> {
    fn into_response(self) -> Response {
        match self {
            HttpResponse::Success(data) => (StatusCode::OK, data).into_response(),
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
    let app = Router::new()
        .route("/pokemon/:name", get(get_pokemon))
        .route("/pokemon/:name/translation/", get(get_pokemon_translation))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port)).await?;
    axum::serve(listener, app).await.unwrap();
    Ok(())
}

async fn get_pokemon(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> HttpResponse<JsonResponse<Pokemon>> {
    state
        .pokemon_api
        .get_pokemon(&name)
        .await
        .map(|p| HttpResponse::Success(JsonResponse(p)))
        .unwrap_or_else(Into::into)
}

async fn get_pokemon_translation(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> HttpResponse<String> {
    match state
        .pokemon_api
        .get_pokemon(&name)
        .await
        .and_then(|p| {
            let translator = p.get_translator();
            p.description
                .map(|d| (d, translator))
                .ok_or(HttpClientError::NotFound)
        })
        .map(|(d, t)| async move {
            state
                .fun_translator
                .translate(&d, t)
                .await
                .map(|tr| tr.contents.translated)
        }) {
        Ok(f) => f
            .await
            .map(HttpResponse::Success)
            .unwrap_or_else(Into::into),
        Err(e) => e.into(),
    }
}
