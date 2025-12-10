# Pokémon ReST API

A rust web service built with [axum](https://github.com/tokio-rs/axum) that enriches pokémon data from [PokéAPI](https://pokeapi.co/) by applying fun translations based on pokémon characteristics.

## configuration

The application can be configured via command-line arguments or environment variables. Command-line arguments take precedence over environment variables.

| setting | description | cli arg | env var | default | required |
|---------|-------------|--------------|---------------------|---------|---------|
| **port** | port number the server listens on | `--port` | `PORT` | `5000` | |
| **pokeapi host** | hostname for [PokéAPI](https://pokeapi.co/) | `--pokeapi-host` | `POKEAPI_HOST` | `pokeapi.co` | x |
| **pokeapi secure** | use HTTPS for [PokéAPI](https://pokeapi.co/) communication | `--pokeapi-secure` | `POKEAPI_SECURE` | `true` | |
| **fun translations host** | hostname for [fun translations API](https://funtranslations.com/api/) | `--fun-translations-host` | `FUN_TRANSLATIONS_HOST` | `api.funtranslations.com` | x |
| **fun translations secure** | use HTTPS for [fun translations API](https://funtranslations.com/api/) communication | `--fun-translations-secure` | `FUN_TRANSLATIONS_SECURE` | `true` | |
| **rust log** | tracing log level (e.g., `info`, `debug`, `trace`) | `--rust-log` | `RUST_LOG` | `info` | | 

## api documentation

The API provides interactive documentation via Swagger UI and exposes an OpenAPI 3.0 specification.

**endpoints:**
- `GET /pokemon/{name}` - fetch Pokemon information with language negotiation support
- `GET /pokemon/{name}/translation/` - fetch translated Pokemon description
- `GET /health` - health check (returns 200 OK)
- `GET /metrics` - Prometheus format metrics
- `GET /api-docs/openapi.json` - OpenAPI specification (JSON)
- `GET /swagger-ui` - Interactive Swagger UI documentation

### language negotiation

The `/pokemon/{name}` endpoint supports HTTP content negotiation via the `Accept-Language` header:

```bash
# Request Pokemon description in Spanish
curl -H "Accept-Language: es" http://localhost:5000/pokemon/pikachu

# Request with multiple language preferences (quality values supported)
curl -H "Accept-Language: es;q=0.9,en;q=0.8" http://localhost:5000/pokemon/pikachu

# Accept any available language as fallback
curl -H "Accept-Language: fr,*" http://localhost:5000/pokemon/pikachu
```

**Behavior:**
- Returns `406 Not Acceptable` if requested language is not available and no wildcard (`*`) is provided
- Returns `Content-Language` header indicating the language of the description
- Falls back to English (`en`) if available
- Falls back to first available language if wildcard is present
- Default behavior (no header): accepts any available language

### swagger ui

The application includes built-in Swagger UI for interactive API exploration:
- Access at: `http://localhost:5000/swagger-ui`
- Automatically loads the OpenAPI spec from `/api-docs/openapi.json`
- Test API endpoints directly from your browser

### metrics

Prometheus metrics are exposed at the `/metrics` endpoint in Prometheus text format. Tracked metrics include:
- `pokemon_requests_total` - total Pokemon requests
- `pokemon_requests_found` - successful Pokemon requests
- `pokemon_requests_not_found` - Pokemon not found (404) requests
- `translations_total` - total translation requests
- `translations_succeeded` - successful translations
- `translations_failed` - failed translations

Example:
```bash
curl http://localhost:5000/metrics
```

### tracing and logging

The application uses structured logging with the `tracing` crate. Log verbosity is controlled via the `RUST_LOG` environment variable:

```bash
# Info level logging (default)
RUST_LOG=info cargo run

# Debug level for detailed operation logs
RUST_LOG=debug cargo run

# Trace level for maximum verbosity
RUST_LOG=trace cargo run

# Filter by module
RUST_LOG=pokemon_api::client=debug,info cargo run
```

Logged information includes:
- HTTP request details (Pokemon names, languages, status codes)
- Pokemon API interactions (base Pokemon fetch, species data, language selection)
- Translation operations (translator type, success/failure)
- Error conditions with context

### distributed tracing spans

The application includes distributed tracing spans for request tracking across service boundaries. Each major operation is wrapped in a span containing relevant context:

**Request Spans:**
- `get_pokemon` - Root span for Pokemon data requests with `pokemon_name` field
- `get_pokemon_translation` - Root span for Pokemon translation requests with `pokemon_name` field
- Internal operations (Pokemon API calls, language negotiation) are automatically traced via `#[instrument]` macros

Spans include structured fields that can be used by distributed tracing backends (e.g., Jaeger, Zipkin) to correlate requests across services and trace performance characteristics.

Example with debug logging to see spans:
```bash
RUST_LOG=debug cargo run
# Output includes span information:
# 2024-12-10T10:30:45.123Z  INFO get_pokemon{pokemon_name=pikachu}: pokemon_api::client: Fetching base pokemon data
```

### examples

```bash
# using environment variables
PORT=8080 cargo run

# using command-line arguments
cargo run -- --port 8080 --pokeapi-host pokeapi.co --pokeapi-secure true

# mixed (CLI takes precedence)
PORT=5000 cargo run -- --port 8080  # server will use port 8080
```

## testing

Run unit tests:
```bash
# Run all unit tests
cargo test --lib

# Run tests with output
cargo test --lib -- --nocapture

# Run specific test
cargo test --lib pokemon_requests_found
```

Run integration tests:
```bash
# Run all integration tests (requires network access to real APIs)
cargo test --test '*' -- --include-ignored

# Run specific integration test
cargo test --test integration_test -- --include-ignored
```

Run all tests (unit + integration):
```bash
cargo test -- --include-ignored
```

## docker build

### local single-arch build

```bash
# build
docker build -t pokemon-rest-api:dev .
# run
docker run --rm -p 5000:5000 pokemon-rest-api:dev
```

## Anything I'd do differently for a production API

I changed the route from `/pokemon/translated/:name` to `/pokemon/:name/translation/` because:
* **resource hierarchy**: the translation is a subordinate resource of `/pokemon/:name` and the URL structure should reflect this relationship.
* **content representation**: the endpoint returns `text/plain` instead of `application/json`; since it's just a description, there's no need to waste CPU time serializing and deserializing JSON when plain text suffices.

For demo purpose I provided swagger-ui alongside the code. Before moving to production it would be better to externalize that concern to a different component(developer portal/sidecar).

I identified a few cross cutting concerns that are bettere suited to infrastructure components:
* **Ingress**: our Api would be served as a load balanced workload externally accessible through a gateway that should provide mTls termination;
* **Egress** traffic control: components such Istio can provide virtual services and manage:  
  *  mTls upgrade;
  *  retry policies/circuit breaker;
  *  reverse proxy with **caching** integration capabilities - see [Redis](https://redis.io/);</ul>
with declarative policies.



## License

MIT
