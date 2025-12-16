use once_cell::sync::Lazy;
use regex::Regex;
use std::env;
use tracing_subscriber::EnvFilter;

use crate::constants::{DEFAULT_PORT, DEFAULT_RUST_LOG};

// NOTE: unwrap() is acceptable here because the regex pattern is a compile-time constant
// and we assume it's correct.
// Validates proper hostname format: alphanumeric labels separated by dots, each label 1-63 chars
static HOST_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$").unwrap()
});

#[derive(Debug, Clone)]
pub struct ConfigDescriptor {
    pub cli_arg_name: &'static str,
    pub env_var_name: &'static str,
    pub description: &'static str,
    pub name: &'static str,
    pub mandatory: Option<bool>,
    pub default_value: Option<&'static str>,
}

impl ConfigDescriptor {
    const POKEAPI_HOST: Self = Self {
        cli_arg_name: "--pokeapi-host",
        env_var_name: "POKEAPI_HOST",
        description: "PokéAPI hostname (e.g., \"pokeapi.co\")",
        name: "pokeapi host",
        mandatory: Some(true),
        default_value: None,
    };
    const POKEAPI_SECURE: Self = Self {
        cli_arg_name: "--pokeapi-secure",
        env_var_name: "POKEAPI_SECURE",
        description: "use secure connection for PokéAPI (true/false)",
        name: "pokeapi secure",
        mandatory: None,
        default_value: Some("true"),
    };
    const FUN_TRANSLATIONS_HOST: Self = Self {
        cli_arg_name: "--fun-translations-host",
        env_var_name: "FUN_TRANSLATIONS_HOST",
        description: "fun translations API hostname (e.g., \"api.funtranslations.com\")",
        name: "fun translations host",
        mandatory: Some(true),
        default_value: None,
    };
    const FUN_TRANSLATIONS_SECURE: Self = Self {
        cli_arg_name: "--fun-translations-secure",
        env_var_name: "FUN_TRANSLATIONS_SECURE",
        description: "use secure connection for fun translations API (true/false)",
        name: "fun translations secure",
        mandatory: None,
        default_value: Some("true"),
    };
    const PORT: Self = Self {
        cli_arg_name: "--port",
        env_var_name: "PORT",
        description: "server listening port (1-65535)",
        name: "port",
        mandatory: None,
        default_value: Some(DEFAULT_PORT),
    };

    const RUST_LOG: Self = Self {
        cli_arg_name: "--rust-log",
        env_var_name: "RUST_LOG",
        description: "tracing log level (e.g., \"info\", \"debug\", etc.)",
        name: "rust log",
        mandatory: None,
        default_value: Some(DEFAULT_RUST_LOG),
    };

    const ALL: [Self; 6] = [
        Self::POKEAPI_HOST,
        Self::FUN_TRANSLATIONS_HOST,
        Self::PORT,
        Self::POKEAPI_SECURE,
        Self::FUN_TRANSLATIONS_SECURE,
        Self::RUST_LOG,
    ];

    pub fn print_usage() {
        eprintln!("\nconfiguration options:");
        eprintln!("======================\n");
        for descriptor in &Self::ALL {
            eprintln!("  {}:", descriptor.name.to_uppercase());
            eprintln!("    description: {}", descriptor.description);
            eprintln!("    cli arg: {}", descriptor.cli_arg_name);
            eprintln!("    env var: {}", descriptor.env_var_name);
            if let Some(m) = descriptor.mandatory {
                eprintln!("    mandatory: {}", m);
            }
            if let Some(d) = descriptor.default_value {
                eprintln!("    default value: {}", d);
            }
            eprintln!();
        }
    }
}

pub trait ConfigParser {
    fn parse(&self, descriptor: &ConfigDescriptor) -> Option<String>;
}

pub struct CliParser {
    args: Vec<String>,
}

impl CliParser {
    pub fn new() -> Self {
        Self {
            args: env::args().collect(),
        }
    }
}

impl ConfigParser for CliParser {
    fn parse(&self, descriptor: &ConfigDescriptor) -> Option<String> {
        self.args.windows(2).find_map(|window| match window {
            [key, value] if key == descriptor.cli_arg_name => Some(value.clone()),
            _ => None,
        })
    }
}

pub struct EnvParser;

impl ConfigParser for EnvParser {
    fn parse(&self, descriptor: &ConfigDescriptor) -> Option<String> {
        env::var(descriptor.env_var_name)
            .ok()
            .and_then(|val| if val.is_empty() { None } else { Some(val) })
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub pokeapi_host: String,
    pub pokeapi_secure: bool,
    pub fun_translations_host: String,
    pub fun_translations_secure: bool,
    pub port: u16,
    pub rust_log: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required configuration: {0}")]
    MissingRequired(String),

    #[error("invalid format: {0}")]
    InvalidFormat(String),

    #[error("multiple configuration errors:\n{}", .0.iter().map(|e| format!("  - {}", e)).collect::<Vec<_>>().join("\n"))]
    Multiple(Vec<ConfigError>),
}

impl AppConfig {
    pub fn pokeapi_base_url(&self) -> String {
        let scheme = if self.pokeapi_secure { "https" } else { "http" };
        format!("{}://{}/api/v2", scheme, self.pokeapi_host)
    }

    pub fn fun_translations_base_url(&self) -> String {
        let scheme = if self.fun_translations_secure {
            "https"
        } else {
            "http"
        };
        format!("{}://{}/translate", scheme, self.fun_translations_host)
    }

    pub fn load() -> Result<Self, ConfigError> {
        let cli_parser = CliParser::new();
        let env_parser = EnvParser;
        let parse = |descriptor: &ConfigDescriptor| {
            cli_parser
                .parse(descriptor)
                .or_else(|| env_parser.parse(descriptor))
        };
        let pokeapi_host = {
            let desc = &ConfigDescriptor::POKEAPI_HOST;
            parse(desc)
                .ok_or_else(|| ConfigError::MissingRequired(desc.name.to_string()))
                .and_then(|host| Self::validate_host(host, desc.name))
        };
        let pokeapi_secure = {
            let desc = &ConfigDescriptor::POKEAPI_SECURE;
            match parse(desc) {
                None => Ok(true),
                Some(s) => parse_bool_config(&s, desc.name),
            }
        };
        let fun_translations_host = {
            let desc = &ConfigDescriptor::FUN_TRANSLATIONS_HOST;
            parse(desc)
                .ok_or_else(|| ConfigError::MissingRequired(desc.name.to_string()))
                .and_then(|host| Self::validate_host(host, desc.name))
        };
        let fun_translations_secure = {
            let desc = &ConfigDescriptor::FUN_TRANSLATIONS_SECURE;
            match parse(desc) {
                None => Ok(true),
                Some(s) => parse_bool_config(&s, desc.name),
            }
        };
        let port = {
            let desc = &ConfigDescriptor::PORT;
            match parse(desc) {
                None => Ok(DEFAULT_PORT.parse::<u16>().unwrap()),
                Some(s) => parse_port_config(&s, desc.name),
            }
        };
        let rust_log = {
            let desc = &ConfigDescriptor::RUST_LOG;
            match parse(desc) {
                None => Ok(DEFAULT_RUST_LOG.to_string()),
                Some(s) => parse_rust_log_config(&s),
            }
        };
        match (
            &pokeapi_host,
            &fun_translations_host,
            &pokeapi_secure,
            &fun_translations_secure,
            &port,
            &rust_log,
        ) {
            (
                Ok(pokeapi_host),
                Ok(fun_translations_host),
                Ok(pokeapi_secure),
                Ok(fun_translations_secure),
                Ok(port),
                Ok(rust_log),
            ) => Ok(AppConfig {
                pokeapi_host: pokeapi_host.clone(),
                fun_translations_host: fun_translations_host.clone(),
                pokeapi_secure: *pokeapi_secure,
                fun_translations_secure: *fun_translations_secure,
                port: *port,
                rust_log: rust_log.clone(),
            }),
            _ => {
                let errors = [
                    pokeapi_host.err(),
                    fun_translations_host.err(),
                    pokeapi_secure.err(),
                    fun_translations_secure.err(),
                    port.err(),
                    rust_log.err(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
                Err(ConfigError::Multiple(errors))
            }
        }
    }

    fn validate_host(host: String, name: &'static str) -> Result<String, ConfigError> {
        match HOST_REGEX.is_match(&host) {
            true => Ok(host),
            false => Err(ConfigError::InvalidFormat(format!(
                "invalid {} format: {}",
                name, host
            ))),
        }
    }
}

/// Parses a boolean configuration value (case-insensitive "true" or "false").
///
/// # Arguments
///
/// * `value` - The string value to parse
/// * `name` - The configuration name for error messages
///
/// # Returns
///
/// Returns `Ok(bool)` on success, or `ConfigError::InvalidFormat` if value is not "true" or "false"
fn parse_bool_config(value: &str, name: &'static str) -> Result<bool, ConfigError> {
    match value.to_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ConfigError::InvalidFormat(format!(
            "invalid boolean value for {}: '{}' (expected 'true' or 'false')",
            name, value
        ))),
    }
}

/// Parses a port configuration value.
///
/// # Arguments
///
/// * `value` - The string value to parse
/// * `name` - The configuration name for error messages
///
/// # Returns
///
/// Returns `Ok(u16)` on success (1-65535), or `ConfigError::InvalidFormat` if:
/// - The value cannot be parsed as a number
/// - The parsed value is 0 or out of valid port range
fn parse_port_config(value: &str, name: &'static str) -> Result<u16, ConfigError> {
    match value.parse::<u16>() {
        Ok(port) if port > 0 => Ok(port),
        Ok(_) => Err(ConfigError::InvalidFormat(format!(
            "invalid {} number: {} (must be 1-65535)",
            name, value
        ))),
        Err(_) => Err(ConfigError::InvalidFormat(format!(
            "port must be a valid number: {} (expected 1-65535)",
            value
        ))),
    }
}

/// Parses a Rust log level configuration value.
///
/// # Arguments
///
/// * `value` - The string value to validate as a log filter
///
/// # Returns
///
/// Returns `Ok(String)` if the value is a valid tracing filter directive, or
/// `ConfigError::InvalidFormat` if the value cannot be parsed as a filter
fn parse_rust_log_config(value: &str) -> Result<String, ConfigError> {
    // Enforce non-empty and non-whitespace-only filters before parsing
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::InvalidFormat(
            "log level filter cannot be empty (e.g., 'info', 'debug', 'trace')".to_string(),
        ));
    }
    // Allowed log levels
    const LEVELS: [&str; 5] = ["trace", "debug", "info", "warn", "error"];

    // Validate each directive segment
    for segment in trimmed.split(',') {
        let seg = segment.trim();
        if seg.is_empty() {
            return Err(ConfigError::InvalidFormat(
                "log level directive segment cannot be empty".to_string(),
            ));
        }

        if let Some(eq_pos) = seg.find('=') {
            let level = &seg[eq_pos + 1..].trim();
            if !LEVELS.contains(level) {
                return Err(ConfigError::InvalidFormat(format!(
                    "invalid log level: '{}' (expected one of: trace, debug, info, warn, error)",
                    level
                )));
            }
        } else if !LEVELS.contains(&seg) {
            // Segment without '=' must be a valid global level
            return Err(ConfigError::InvalidFormat(format!(
                "invalid global log level: '{}' (expected one of: trace, debug, info, warn, error)",
                seg
            )));
        }
    }

    // Finally, ensure the entire filter string parses
    EnvFilter::try_new(trimmed)
        .map(|_| trimmed.to_string())
        .map_err(|_| {
            ConfigError::InvalidFormat(format!(
                "invalid log level filter: '{}' (e.g., 'info', 'debug', 'trace')",
                trimmed
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Boolean Configuration Tests
    #[test]
    fn parse_bool_config_accepts_true_variants() {
        assert!(parse_bool_config("true", "test").unwrap());
        assert!(parse_bool_config("True", "test").unwrap());
        assert!(parse_bool_config("TRUE", "test").unwrap());
        assert!(parse_bool_config("tRuE", "test").unwrap());
    }

    #[test]
    fn parse_bool_config_accepts_false_variants() {
        assert!(!parse_bool_config("false", "test").unwrap());
        assert!(!parse_bool_config("False", "test").unwrap());
        assert!(!parse_bool_config("FALSE", "test").unwrap());
        assert!(!parse_bool_config("fAlSe", "test").unwrap());
    }

    #[test]
    fn parse_bool_config_rejects_invalid_values() {
        assert!(parse_bool_config("yes", "test").is_err());
        assert!(parse_bool_config("no", "test").is_err());
        assert!(parse_bool_config("1", "test").is_err());
        assert!(parse_bool_config("0", "test").is_err());
        assert!(parse_bool_config("on", "test").is_err());
        assert!(parse_bool_config("off", "test").is_err());
        assert!(parse_bool_config("", "test").is_err());
        assert!(parse_bool_config("t", "test").is_err());
        assert!(parse_bool_config("f", "test").is_err());
    }

    #[test]
    fn parse_bool_config_error_message_includes_value() {
        let result = parse_bool_config("invalid", "test_field");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid"));
        assert!(err_msg.contains("test_field"));
        assert!(err_msg.contains("true"));
        assert!(err_msg.contains("false"));
    }

    // Port Configuration Tests
    #[test]
    fn parse_port_config_accepts_valid_ports() {
        assert_eq!(parse_port_config("5000", "test").unwrap(), 5000);
        assert_eq!(parse_port_config("1", "test").unwrap(), 1);
        assert_eq!(parse_port_config("65535", "test").unwrap(), 65535);
        assert_eq!(parse_port_config("8080", "test").unwrap(), 8080);
        assert_eq!(parse_port_config("3000", "test").unwrap(), 3000);
        assert_eq!(parse_port_config("443", "test").unwrap(), 443);
        assert_eq!(parse_port_config("80", "test").unwrap(), 80);
    }

    #[test]
    fn parse_port_config_rejects_zero() {
        let result = parse_port_config("0", "test");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("0"));
        assert!(err_msg.contains("1-65535"));
    }

    #[test]
    fn parse_port_config_rejects_invalid_numbers() {
        assert!(parse_port_config("abc", "test").is_err());
        assert!(parse_port_config("-1", "test").is_err());
        assert!(parse_port_config("99999", "test").is_err());
        assert!(parse_port_config("65536", "test").is_err());
        assert!(parse_port_config("70000", "test").is_err());
        assert!(parse_port_config("", "test").is_err());
        assert!(parse_port_config(" ", "test").is_err());
        assert!(parse_port_config("5000a", "test").is_err());
        assert!(parse_port_config("50.00", "test").is_err());
    }

    #[test]
    fn parse_port_config_error_messages_are_descriptive() {
        let result = parse_port_config("0", "port");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("1-65535"));

        let result = parse_port_config("abc", "port");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("valid number"));
    }

    // Hostname Validation Tests
    #[test]
    fn validate_host_accepts_valid_hostnames() {
        assert!(validate_hostname_for_test("example.com").is_ok());
        assert!(validate_hostname_for_test("sub.example.com").is_ok());
        assert!(validate_hostname_for_test("api.funtranslations.com").is_ok());
        assert!(validate_hostname_for_test("pokeapi.co").is_ok());
        assert!(validate_hostname_for_test("a").is_ok());
        assert!(validate_hostname_for_test("my-server").is_ok());
        assert!(validate_hostname_for_test("test-123").is_ok());
        assert!(validate_hostname_for_test("localhost").is_ok());
        assert!(validate_hostname_for_test("api-v2.example.org").is_ok());
        assert!(validate_hostname_for_test("deep.sub.domain.example.com").is_ok());
    }

    #[test]
    fn validate_host_accepts_hostnames_with_numbers() {
        assert!(validate_hostname_for_test("server1.example.com").is_ok());
        assert!(validate_hostname_for_test("123.456.789.012").is_ok());
        assert!(validate_hostname_for_test("api2.test.co").is_ok());
    }

    #[test]
    fn validate_host_rejects_invalid_hostnames() {
        assert!(validate_hostname_for_test("-example.com").is_err());
        assert!(validate_hostname_for_test("example.com-").is_err());
        assert!(validate_hostname_for_test("example..com").is_err());
        assert!(validate_hostname_for_test("").is_err());
        assert!(validate_hostname_for_test(".example.com").is_err());
        assert!(validate_hostname_for_test("example.com.").is_err());
        assert!(validate_hostname_for_test("exam ple.com").is_err());
        assert!(validate_hostname_for_test("example_.com").is_err());
        assert!(validate_hostname_for_test("exam$ple.com").is_err());
    }

    #[test]
    fn validate_host_error_message_includes_hostname() {
        let result = validate_hostname_for_test("-invalid.com");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("-invalid.com"));
    }

    #[test]
    fn validate_host_returns_original_hostname_on_success() {
        let hostname = "example.com";
        let result = validate_hostname_for_test(hostname);
        assert_eq!(result.unwrap(), hostname);
    }

    // Rust Log Configuration Tests
    #[test]
    fn parse_rust_log_config_accepts_valid_levels() {
        assert!(parse_rust_log_config("info").is_ok());
        assert!(parse_rust_log_config("debug").is_ok());
        assert!(parse_rust_log_config("trace").is_ok());
        assert!(parse_rust_log_config("warn").is_ok());
        assert!(parse_rust_log_config("error").is_ok());
    }

    #[test]
    fn parse_rust_log_config_accepts_module_filters() {
        assert!(parse_rust_log_config("pokemon_api=debug").is_ok());
        assert!(parse_rust_log_config("pokemon_api::client=trace,info").is_ok());
        assert!(parse_rust_log_config("pokemon_api=debug,translator=info").is_ok());
    }

    #[test]
    fn parse_rust_log_config_rejects_invalid_filters() {
        assert!(parse_rust_log_config("invalid_level").is_err());
        assert!(parse_rust_log_config("").is_err());
        assert!(parse_rust_log_config("123").is_err());
    }

    #[test]
    fn parse_rust_log_config_error_message_is_helpful() {
        let result = parse_rust_log_config("invalid");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid"));
        assert!(err_msg.contains("info") || err_msg.contains("debug") || err_msg.contains("trace"));
    }

    #[test]
    fn parse_rust_log_config_returns_original_value() {
        let filter = "info";
        let result = parse_rust_log_config(filter);
        assert_eq!(result.unwrap(), filter);

        let complex_filter = "pokemon_api=debug,info";
        let result = parse_rust_log_config(complex_filter);
        assert_eq!(result.unwrap(), complex_filter);
    }

    // URL Generation Tests
    #[test]
    fn pokeapi_base_url_uses_https_when_secure() {
        let config = AppConfig {
            pokeapi_host: "pokeapi.co".to_string(),
            pokeapi_secure: true,
            fun_translations_host: "api.funtranslations.com".to_string(),
            fun_translations_secure: true,
            port: 5000,
            rust_log: "info".to_string(),
        };
        assert_eq!(config.pokeapi_base_url(), "https://pokeapi.co/api/v2");
    }

    #[test]
    fn pokeapi_base_url_uses_http_when_not_secure() {
        let config = AppConfig {
            pokeapi_host: "localhost".to_string(),
            pokeapi_secure: false,
            fun_translations_host: "localhost".to_string(),
            fun_translations_secure: false,
            port: 5000,
            rust_log: "info".to_string(),
        };
        assert_eq!(config.pokeapi_base_url(), "http://localhost/api/v2");
    }

    #[test]
    fn fun_translations_base_url_uses_https_when_secure() {
        let config = AppConfig {
            pokeapi_host: "pokeapi.co".to_string(),
            pokeapi_secure: true,
            fun_translations_host: "api.funtranslations.com".to_string(),
            fun_translations_secure: true,
            port: 5000,
            rust_log: "info".to_string(),
        };
        assert_eq!(
            config.fun_translations_base_url(),
            "https://api.funtranslations.com/translate"
        );
    }

    #[test]
    fn fun_translations_base_url_uses_http_when_not_secure() {
        let config = AppConfig {
            pokeapi_host: "localhost".to_string(),
            pokeapi_secure: false,
            fun_translations_host: "localhost".to_string(),
            fun_translations_secure: false,
            port: 5000,
            rust_log: "info".to_string(),
        };
        assert_eq!(
            config.fun_translations_base_url(),
            "http://localhost/translate"
        );
    }

    // ConfigDescriptor Tests
    #[test]
    fn config_descriptor_all_array_contains_all_fields() {
        let all = ConfigDescriptor::ALL;
        assert_eq!(all.len(), 6);

        let names: Vec<&str> = all.iter().map(|d| d.name).collect();
        assert!(names.contains(&"pokeapi host"));
        assert!(names.contains(&"pokeapi secure"));
        assert!(names.contains(&"fun translations host"));
        assert!(names.contains(&"fun translations secure"));
        assert!(names.contains(&"port"));
        assert!(names.contains(&"rust log"));
    }

    #[test]
    fn config_descriptor_mandatory_fields_are_marked() {
        assert_eq!(ConfigDescriptor::POKEAPI_HOST.mandatory, Some(true));
        assert_eq!(
            ConfigDescriptor::FUN_TRANSLATIONS_HOST.mandatory,
            Some(true)
        );
        assert_eq!(ConfigDescriptor::PORT.mandatory, None);
        assert_eq!(ConfigDescriptor::POKEAPI_SECURE.mandatory, None);
    }

    #[test]
    fn config_descriptor_optional_fields_have_defaults() {
        assert!(ConfigDescriptor::POKEAPI_SECURE.default_value.is_some());
        assert!(
            ConfigDescriptor::FUN_TRANSLATIONS_SECURE
                .default_value
                .is_some()
        );
        assert!(ConfigDescriptor::PORT.default_value.is_some());
        assert!(ConfigDescriptor::RUST_LOG.default_value.is_some());
    }

    #[test]
    fn config_descriptor_mandatory_fields_have_no_defaults() {
        assert!(ConfigDescriptor::POKEAPI_HOST.default_value.is_none());
        assert!(
            ConfigDescriptor::FUN_TRANSLATIONS_HOST
                .default_value
                .is_none()
        );
    }

    // CliParser Tests
    #[test]
    fn cli_parser_extracts_arguments() {
        // Test with mock args
        let test_parser = CliParser {
            args: vec![
                "program".to_string(),
                "--port".to_string(),
                "8080".to_string(),
            ],
        };

        let result = test_parser.parse(&ConfigDescriptor::PORT);
        assert_eq!(result, Some("8080".to_string()));
    }

    #[test]
    fn cli_parser_returns_none_for_missing_args() {
        let test_parser = CliParser {
            args: vec!["program".to_string()],
        };

        let result = test_parser.parse(&ConfigDescriptor::PORT);
        assert_eq!(result, None);
    }

    #[test]
    fn cli_parser_returns_none_for_wrong_args() {
        let test_parser = CliParser {
            args: vec![
                "program".to_string(),
                "--other".to_string(),
                "value".to_string(),
            ],
        };

        let result = test_parser.parse(&ConfigDescriptor::PORT);
        assert_eq!(result, None);
    }

    // EnvParser Tests
    #[test]
    fn env_parser_extracts_environment_variables() {
        unsafe { std::env::set_var("TEST_PORT", "9000") };

        let descriptor = ConfigDescriptor {
            cli_arg_name: "--test-port",
            env_var_name: "TEST_PORT",
            description: "Test port",
            name: "test port",
            mandatory: None,
            default_value: None,
        };

        let parser = EnvParser;
        let result = parser.parse(&descriptor);

        assert_eq!(result, Some("9000".to_string()));

        unsafe { std::env::remove_var("TEST_PORT") };
    }

    #[test]
    fn env_parser_returns_none_for_empty_values() {
        unsafe { std::env::set_var("TEST_EMPTY", "") };

        let descriptor = ConfigDescriptor {
            cli_arg_name: "--test-empty",
            env_var_name: "TEST_EMPTY",
            description: "Test empty",
            name: "test empty",
            mandatory: None,
            default_value: None,
        };

        let parser = EnvParser;
        let result = parser.parse(&descriptor);

        assert_eq!(result, None);

        unsafe { std::env::remove_var("TEST_EMPTY") };
    }

    #[test]
    fn env_parser_returns_none_for_missing_variables() {
        let descriptor = ConfigDescriptor {
            cli_arg_name: "--test-missing",
            env_var_name: "TEST_MISSING_VAR_THAT_DOES_NOT_EXIST",
            description: "Test missing",
            name: "test missing",
            mandatory: None,
            default_value: None,
        };

        let parser = EnvParser;
        let result = parser.parse(&descriptor);

        assert_eq!(result, None);
    }

    // Helper function for hostname validation tests
    fn validate_hostname_for_test(host: &str) -> Result<String, ConfigError> {
        AppConfig::validate_host(host.to_string(), "test")
    }
}
