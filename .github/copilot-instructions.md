# Pokémon REST API - Copilot Instructions

## Project Overview

A Rust-based REST API service that enriches Pokémon data from [PokéAPI](https://pokeapi.co/) with fun translations via [Fun Translations API](https://funtranslations.com/api/). Built with **axum** web framework and **tokio** async runtime.

**Key Technologies**: Axum 0.8, Tokio, Reqwest, Serde, Prometheus metrics, Tracing, OpenAPI/Swagger

## Architecture

### Layered Design (4 Primary Modules)

```
HTTP Handlers (main.rs)
    ↓
Pokemon API Layer (pokemon_api/client.rs)
    ↓
Translation Layer (translator/client.rs)
    ↓
Shared HTTP Client (http/client.rs)
```

**Module Responsibilities**:

- **`pokemon_api::client`**: PokéAPI integration with language negotiation (RFC 7231), automatic translator selection based on Pokémon characteristics (Yoda for legendary/cave, Shakespeare for others)
- **`translator::client`**: Fun Translations API with rate limit handling
- **`http::client`**: Shared async HTTP client wrapper with error handling and status codes
- **`config`**: CLI arguments + environment variable parsing (CLI takes precedence)
- **`metrics`**: Prometheus counter/histogram collection, excludes internal endpoints (`/healthz`, `/metrics`, `/swagger-ui`, `/api-docs`)

### Data Flow

1. Client → `/pokemon/{name}` with optional `Accept-Language` header
2. Handler creates tracing span, calls Pokemon API client
3. PokéAPI client fetches base data + species info
4. Language negotiation selects best available description
5. Translator selection determines Yoda vs Shakespeare based on Pokémon traits
6. Response includes `Content-Language` header + metrics increment

## Key Patterns & Conventions

### Language Negotiation (RFC 7231)

- Behavior: requested langs → English fallback → first available → 406 NotAcceptable (if no wildcard)
- Header example: `Accept-Language: es;q=0.9,en;q=0.8,*` returns Spanish if available, else English, else any
- Code: [pokemon_api/client.rs](../src/pokemon_api/client.rs) - `negotiate_language()` method

### Async Testing

- Use `#[tokio::test]` for async tests (already in use across modules)
- Use `mockito` for HTTP mocking (see translator tests)
- Use `rstest` for parameterized tests (see config tests)

### Error Handling

- Custom error type `HttpClientError` (enum: `NotFound`, `RateLimited`, `RequestFailed`, `ParseError`, `ServerError`)
- Metrics track specific error types (rate limits, service unavailable)
- PokéAPI layer translates HTTP errors to domain errors (e.g., 404 → `NotFound`)

### Configuration

- Dual input: CLI args (`--port`, `--pokeapi-host`, etc.) override env vars
- Validation: hostname regex, port range, boolean parsing
- Mandatory fields: PokéAPI host, Fun Translations host
- See [config.rs](../src/config.rs) ConfigDescriptor pattern for adding new settings

## Critical Workflows

### Build & Run

```bash
# Development build (debug symbols)
cargo build

# Release build (optimized: LTO, single codegen unit, stripped)
cargo build --release

# Run with debug logging
RUST_LOG=debug cargo run

# Run with specific config
cargo run -- --port 3000 --pokeapi-host pokeapi.co --pokeapi-secure true

# Run tests
cargo test
cargo test -- --nocapture  # see println! output
```

### Docker Workflow

```bash
# Build and run full stack (API + Swagger UI + Nginx)
docker-compose up --build

# Access: http://localhost:80 (via nginx proxy)
# Swagger UI: http://localhost:80/swagger-ui
# API: http://localhost:80 (proxied to port 5050)
```

### Testing Strategy

- **Unit tests**: Inline `#[cfg(test)] mod tests` in each module
- **Config tests**: Validate env parsing, CLI arg precedence, validation
- **API client tests**: Mock HTTP responses, test language negotiation logic
- **Translator tests**: Mock rate limit (429) and success responses
- Run: `cargo test --lib` (unit only) or `cargo test` (all)

## API Endpoints

| Endpoint | Purpose |
|----------|---------|
| `GET /pokemon/{name}` | Enriched Pokémon data with language negotiation |
| `GET /pokemon/{name}/translation/` | Translated description only |
| `GET /healthz` | Health check (200 OK) |
| `GET /metrics` | Prometheus metrics |
| `GET /swagger-ui` | Interactive API docs |
| `GET /api-docs/openapi.json` | OpenAPI 3.0 spec |

## Important Files & Their Purpose

- [src/main.rs](../src/main.rs): Axum router setup, HTTP handler, AcceptLanguage parsing, request tracing
- [src/pokemon_api/client.rs](../src/pokemon_api/client.rs): PokéAPI integration, language negotiation, translator selection logic
- [src/translator/client.rs](../src/translator/client.rs): Fun Translations API client, rate limit handling
- [src/config.rs](../src/config.rs): Configuration parsing & validation (large file with 24+ tests)
- [src/metrics.rs](../src/metrics.rs): Prometheus counter/histogram definitions, middleware, path normalization
- [Cargo.toml](../Cargo.toml): Edition 2024, release profile optimization (LTO, size optimization)

## Common Development Tasks

**Adding a new endpoint**: Update Axum router in `main.rs`, create handler function with tracing span, ensure metrics are tracked (use `track_metrics` middleware).

**Adding configuration**: Add new `ConfigDescriptor` constant in `config.rs`, update parsing logic, add tests for validation and defaults.

**Debugging requests**: Set `RUST_LOG=pokemon_api::client=debug,translator::client=debug cargo run` to see request/response logs and language negotiation decisions.

**Integration testing**: Mock PokéAPI responses in tests, verify language negotiation + translator selection using `mockito`.

## External Dependencies & Constraints

- **PokéAPI**: Requires hostname config, supports HTTP/HTTPS, no authentication needed, rate limit is generous
- **Fun Translations API**: Rate limit ~5 requests/hour (429 responses), gracefully handled as `HttpClientError::RateLimited`
- **Docker Compose**: Nginx acts as reverse proxy, provides CORS headers, health check uses internal `/httpget` utility
- **OpenAPI**: Auto-generated from Axum handlers via `utoipa` crate, exposes at `/api-docs/openapi.json`

## Testing Patterns in Use

```rust
// Async test with mockito mock server
#[tokio::test]
async fn test_translation() {
    let mock = mockito::mock("GET", mockito::Matcher::Regex(...))
        .with_status(200)
        .with_body(...)
        .create();
    // test code
    mock.assert();
}

// Parameterized test with rstest
#[rstest]
#[case("valid-hostname", true)]
#[case("invalid..name", false)]
fn test_hostname_validation(#[case] input: &str, #[case] expected: bool) { ... }

// Unit test with shared state
#[test]
fn test_metrics_increment() {
    COUNTER.reset();
    COUNTER.inc();
    assert_eq!(COUNTER.get_value(), 1);
}
```

---

**Last Updated**: January 2026 | **Rust Edition**: 2024
