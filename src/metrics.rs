use axum::{extract::Request, middleware::Next, response::Response};
use once_cell::sync::Lazy;
use prometheus::{Counter, CounterVec, HistogramVec, Registry};
use std::time::Instant;

pub static REGISTRY: Lazy<Registry> = Lazy::new(Registry::new);

pub static HTTP_REQUESTS_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    CounterVec::new(
        prometheus::Opts::new("http_requests_total", "Total HTTP requests"),
        &["method", "path", "status"],
    )
    .expect("Failed to create HTTP_REQUESTS_TOTAL metric")
});

pub static HTTP_REQUEST_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    HistogramVec::new(
        prometheus::HistogramOpts::new(
            "http_request_duration_seconds",
            "HTTP request duration in seconds",
        ),
        &["method", "path"],
    )
    .expect("Failed to create HTTP_REQUEST_DURATION_SECONDS metric")
});

pub static POKEMON_REQUESTS_TOTAL: Lazy<Counter> = Lazy::new(|| {
    Counter::new("pokemon_requests_total", "Total requests to get Pokemon")
        .expect("Failed to create POKEMON_REQUESTS_TOTAL metric")
});

pub static POKEMON_REQUESTS_FOUND: Lazy<Counter> = Lazy::new(|| {
    Counter::new(
        "pokemon_requests_found",
        "Pokemon requests that returned a result",
    )
    .expect("Failed to create POKEMON_REQUESTS_FOUND metric")
});

pub static POKEMON_REQUESTS_NOT_FOUND: Lazy<Counter> = Lazy::new(|| {
    Counter::new(
        "pokemon_requests_not_found",
        "Pokemon requests that returned 404",
    )
    .expect("Failed to create POKEMON_REQUESTS_NOT_FOUND metric")
});

pub static TRANSLATIONS_TOTAL: Lazy<Counter> = Lazy::new(|| {
    Counter::new("translations_total", "Total translation requests")
        .expect("Failed to create TRANSLATIONS_TOTAL metric")
});

pub static TRANSLATIONS_SUCCEEDED: Lazy<Counter> = Lazy::new(|| {
    Counter::new("translations_succeeded", "Successful translations")
        .expect("Failed to create TRANSLATIONS_SUCCEEDED metric")
});

pub static TRANSLATIONS_FAILED: Lazy<Counter> = Lazy::new(|| {
    Counter::new("translations_failed", "Failed translation requests")
        .expect("Failed to create TRANSLATIONS_FAILED metric")
});

pub static SERVICE_UNAVAILABLE_ERRORS: Lazy<Counter> = Lazy::new(|| {
    Counter::new(
        "service_unavailable_errors_total",
        "Total service unavailable errors (503)",
    )
    .expect("Failed to create SERVICE_UNAVAILABLE_ERRORS metric")
});

pub static RATE_LIMITED_ERRORS: Lazy<Counter> = Lazy::new(|| {
    Counter::new(
        "rate_limited_errors_total",
        "Total rate limited errors (429)",
    )
    .expect("Failed to create RATE_LIMITED_ERRORS metric")
});

/// Initializes the Prometheus metrics registry.
///
/// Registers all defined metrics with the global registry. Should be called once
/// during application startup before any metrics are recorded.
///
/// # Panics
///
/// This function uses `.expect()` on registration failures since metrics
/// initialization is critical for observability and should fail fast if
/// there are issues (e.g., duplicate metric names).
pub fn init() {
    REGISTRY
        .register(Box::new(HTTP_REQUESTS_TOTAL.clone()))
        .expect("Failed to register HTTP_REQUESTS_TOTAL");
    REGISTRY
        .register(Box::new(HTTP_REQUEST_DURATION_SECONDS.clone()))
        .expect("Failed to register HTTP_REQUEST_DURATION_SECONDS");
    REGISTRY
        .register(Box::new(POKEMON_REQUESTS_TOTAL.clone()))
        .expect("Failed to register POKEMON_REQUESTS_TOTAL");
    REGISTRY
        .register(Box::new(POKEMON_REQUESTS_FOUND.clone()))
        .expect("Failed to register POKEMON_REQUESTS_FOUND");
    REGISTRY
        .register(Box::new(POKEMON_REQUESTS_NOT_FOUND.clone()))
        .expect("Failed to register POKEMON_REQUESTS_NOT_FOUND");
    REGISTRY
        .register(Box::new(TRANSLATIONS_TOTAL.clone()))
        .expect("Failed to register TRANSLATIONS_TOTAL");
    REGISTRY
        .register(Box::new(TRANSLATIONS_SUCCEEDED.clone()))
        .expect("Failed to register TRANSLATIONS_SUCCEEDED");
    REGISTRY
        .register(Box::new(TRANSLATIONS_FAILED.clone()))
        .expect("Failed to register TRANSLATIONS_FAILED");
    REGISTRY
        .register(Box::new(SERVICE_UNAVAILABLE_ERRORS.clone()))
        .expect("Failed to register SERVICE_UNAVAILABLE_ERRORS");
    REGISTRY
        .register(Box::new(RATE_LIMITED_ERRORS.clone()))
        .expect("Failed to register RATE_LIMITED_ERRORS");
}

/// Axum middleware that tracks HTTP request metrics.
///
/// Records metrics for each HTTP request including:
/// - Total request count by method, path, and status code
/// - Request duration histogram by method and path
///
/// Excludes internal endpoints from tracking:
/// - `/health` - health check endpoint
/// - `/metrics` - metrics endpoint (avoid recursive tracking)
/// - `/swagger-ui` - documentation UI
/// - `/api-docs` - OpenAPI spec
///
/// # Example
///
/// ```no_run
/// use axum::{Router, middleware};
/// use crate::metrics::track_metrics;
///
/// let app = Router::new()
///     .layer(middleware::from_fn(track_metrics));
/// ```
pub async fn track_metrics(req: Request, next: Next) -> Response {
    let path = req.uri().path();

    // Skip tracking for internal/monitoring endpoints
    if should_skip_tracking(path) {
        return next.run(req).await;
    }

    let method = req.method().to_string();
    let normalized_path = normalize_path(path);
    let start = Instant::now();

    let response = next.run(req).await;

    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    // Record metrics
    HTTP_REQUESTS_TOTAL
        .with_label_values(&[&method, &normalized_path, &status])
        .inc();

    HTTP_REQUEST_DURATION_SECONDS
        .with_label_values(&[&method, &normalized_path])
        .observe(duration);

    response
}

/// Determines if a path should be excluded from metrics tracking.
///
/// Returns true for internal endpoints that don't need to be tracked:
/// - Health checks (`/health`)
/// - Metrics endpoint (`/metrics`)
/// - Swagger UI (`/swagger-ui/*`)
/// - API documentation (`/api-docs/*`)
fn should_skip_tracking(path: &str) -> bool {
    // Fast path: check exact matches first (most common)
    if path == "/health" || path == "/metrics" {
        return true;
    }

    // Check prefixes only if needed
    let path_bytes = path.as_bytes();

    // Check /swagger-ui prefix (12 bytes)
    if path_bytes.len() >= 11 && &path_bytes[..11] == b"/swagger-ui" {
        return true;
    }

    // Check /api-docs prefix (9 bytes)
    if path_bytes.len() >= 9 && &path_bytes[..9] == b"/api-docs" {
        return true;
    }

    false
}

/// Normalizes request paths to avoid creating too many unique metrics labels.
///
/// Converts dynamic path segments (like Pokemon names) to generic placeholders
/// to keep cardinality manageable in the metrics system.
///
/// # Examples
///
/// - `/pokemon/pikachu` → `/pokemon/{name}`
/// - `/pokemon/charizard/translation/` → `/pokemon/{name}/translation/`
fn normalize_path(path: &str) -> String {
    // Split path into segments
    let segments: Vec<&str> = path.split('/').collect();

    match segments.as_slice() {
        // Root
        ["", ""] | [""] => "/".to_string(),

        // Pokemon endpoints
        ["", "pokemon", _name] => "/pokemon/{name}".to_string(),
        ["", "pokemon", _name, "translation", ""] | ["", "pokemon", _name, "translation"] => {
            "/pokemon/{name}/translation/".to_string()
        }

        // Default: return as-is for unknown paths
        _ => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_skip_tracking_health() {
        assert!(should_skip_tracking("/health"));
    }

    #[test]
    fn test_should_skip_tracking_metrics() {
        assert!(should_skip_tracking("/metrics"));
    }

    #[test]
    fn test_should_skip_tracking_swagger() {
        assert!(should_skip_tracking("/swagger-ui"));
        assert!(should_skip_tracking("/swagger-ui/"));
        assert!(should_skip_tracking("/swagger-ui/index.html"));
    }

    #[test]
    fn test_should_skip_tracking_api_docs() {
        assert!(should_skip_tracking("/api-docs"));
        assert!(should_skip_tracking("/api-docs/openapi.json"));
    }

    #[test]
    fn test_should_not_skip_tracking_pokemon() {
        assert!(!should_skip_tracking("/pokemon/pikachu"));
        assert!(!should_skip_tracking("/pokemon/charizard/translation/"));
    }

    #[test]
    fn test_normalize_path_pokemon() {
        assert_eq!(normalize_path("/pokemon/pikachu"), "/pokemon/{name}");
        assert_eq!(normalize_path("/pokemon/charizard"), "/pokemon/{name}");
        assert_eq!(normalize_path("/pokemon/ditto"), "/pokemon/{name}");
    }

    #[test]
    fn test_normalize_path_translation() {
        assert_eq!(
            normalize_path("/pokemon/pikachu/translation/"),
            "/pokemon/{name}/translation/"
        );
        assert_eq!(
            normalize_path("/pokemon/mewtwo/translation"),
            "/pokemon/{name}/translation/"
        );
    }

    #[test]
    fn test_normalize_path_root() {
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path(""), "/");
    }

    #[test]
    fn test_normalize_path_unknown() {
        assert_eq!(normalize_path("/unknown/path"), "/unknown/path");
    }
}
