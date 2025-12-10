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

### examples

```bash
# using environment variables
PORT=8080 cargo run

# using command-line arguments
cargo run -- --port 8080 --pokeapi-host pokeapi.co --pokeapi-secure true

# mixed (CLI takes precedence)
PORT=5000 cargo run -- --port 8080  # server will use port 8080
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
  *  reverse proxy capabilities;</ul>
with declarative policies.

**Implemented features:**
* ✅ OpenAPI 3.0 specification with automatic schema generation via `utoipa`
* ✅ Built-in Swagger UI for API documentation and testing
* ✅ HTTP content negotiation via `Accept-Language` header
* ✅ `Content-Language` header in responses
* ✅ Proper HTTP status codes (406 Not Acceptable for unsupported languages)
* ✅ Comprehensive unit test coverage (15 tests) plus ignored integration test against real API
* ✅ Docker multi-service setup with nginx and Swagger UI

Some TODOs:
* Add OpenTelemetry instrumentation
* Add a dashboard with dedicated metrics (p99/p95 by route)

## License

MIT
