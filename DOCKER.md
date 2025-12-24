#!/bin/bash
set -e

cat << 'EOF'
Docker Compose Setup for Pokemon API
=====================================

This setup uses the official Swagger UI Docker image.
No manual download required!

Commands
--------
Start services:       docker-compose up -d
Stop services:        docker-compose down
Rebuild after changes: docker-compose up -d --build
View logs:            docker-compose logs -f
View API logs only:   docker-compose logs -f api

Access Points
-------------
Swagger UI:  http://localhost:8080/swagger-ui/
Health:      http://localhost:8080/health
Metrics:     http://localhost:8080/metrics

API Endpoints
-------------
GET /pokemon/{name}
GET /pokemon/{name}/translation/

Examples
--------
# Get Pokemon data (English by default)
curl http://localhost:8080/pokemon/pikachu

# Get translated description
curl http://localhost:8080/pokemon/ditto/translation/

# Request Spanish description
curl -H 'Accept-Language: es' http://localhost:8080/pokemon/mewtwo

# Request with language priority
curl -H 'Accept-Language: fr;q=0.9,en;q=0.8' http://localhost:8080/pokemon/charizard

Architecture
------------
┌─────────────┐
│   Client    │
└──────┬──────┘
       │
       │ :8080
       ▼
┌─────────────┐
│    Nginx    │ (CORS, routing)
└──────┬──────┘
       │
       ├─────────► Swagger UI :8080 (swaggerapi/swagger-ui)
       │
       └─────────► Pokemon API :5050 (Rust/Axum)
                   ├─► PokéAPI (pokeapi.co)
                   └─► Fun Translations API

Features
--------
✓ CORS headers for cross-origin requests
✓ OpenAPI 3.0 specification served at /api-docs/openapi.json
✓ Interactive API documentation via Swagger UI
✓ Health checks and readiness probes
✓ Prometheus metrics at /metrics
✓ RFC 7231 language negotiation
✓ Automatic translator selection (Yoda/Shakespeare)
✓ Rate limit graceful degradation

EOF
