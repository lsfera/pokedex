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

Some TODOs:
* Add OpenTelemetry instrumentation
* Add a dashboard with dedicated metrics (p99/p95 by route)

## License

MIT
