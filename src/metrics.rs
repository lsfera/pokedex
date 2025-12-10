use lazy_static::lazy_static;
use prometheus::{Counter, CounterVec, HistogramVec, Registry};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    
    pub static ref HTTP_REQUESTS_TOTAL: CounterVec = CounterVec::new(
        prometheus::Opts::new("http_requests_total", "Total HTTP requests"),
        &["method", "path", "status"]
    ).unwrap();
    
    pub static ref HTTP_REQUEST_DURATION_SECONDS: HistogramVec = HistogramVec::new(
        prometheus::HistogramOpts::new("http_request_duration_seconds", "HTTP request duration in seconds"),
        &["method", "path"]
    ).unwrap();
    
    pub static ref POKEMON_REQUESTS_TOTAL: Counter = Counter::new(
        "pokemon_requests_total",
        "Total requests to get Pokemon"
    ).unwrap();
    
    pub static ref POKEMON_REQUESTS_FOUND: Counter = Counter::new(
        "pokemon_requests_found",
        "Pokemon requests that returned a result"
    ).unwrap();
    
    pub static ref POKEMON_REQUESTS_NOT_FOUND: Counter = Counter::new(
        "pokemon_requests_not_found",
        "Pokemon requests that returned 404"
    ).unwrap();
    
    pub static ref TRANSLATIONS_TOTAL: Counter = Counter::new(
        "translations_total",
        "Total translation requests"
    ).unwrap();
    
    pub static ref TRANSLATIONS_SUCCEEDED: Counter = Counter::new(
        "translations_succeeded",
        "Successful translations"
    ).unwrap();
    
    pub static ref TRANSLATIONS_FAILED: Counter = Counter::new(
        "translations_failed",
        "Failed translation requests"
    ).unwrap();
    
    pub static ref SERVICE_UNAVAILABLE_ERRORS: Counter = Counter::new(
        "service_unavailable_errors_total",
        "Total service unavailable errors (503)"
    ).unwrap();
    
    pub static ref RATE_LIMITED_ERRORS: Counter = Counter::new(
        "rate_limited_errors_total",
        "Total rate limited errors (429)"
    ).unwrap();
}

pub fn init() {
    REGISTRY.register(Box::new(HTTP_REQUESTS_TOTAL.clone())).ok();
    REGISTRY.register(Box::new(HTTP_REQUEST_DURATION_SECONDS.clone())).ok();
    REGISTRY.register(Box::new(POKEMON_REQUESTS_TOTAL.clone())).ok();
    REGISTRY.register(Box::new(POKEMON_REQUESTS_FOUND.clone())).ok();
    REGISTRY.register(Box::new(POKEMON_REQUESTS_NOT_FOUND.clone())).ok();
    REGISTRY.register(Box::new(TRANSLATIONS_TOTAL.clone())).ok();
    REGISTRY.register(Box::new(TRANSLATIONS_SUCCEEDED.clone())).ok();
    REGISTRY.register(Box::new(TRANSLATIONS_FAILED.clone())).ok();
    REGISTRY.register(Box::new(SERVICE_UNAVAILABLE_ERRORS.clone())).ok();
    REGISTRY.register(Box::new(RATE_LIMITED_ERRORS.clone())).ok();
}
