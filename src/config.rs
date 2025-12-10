use once_cell::sync::Lazy;
use regex::Regex;
use std::env;

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
        env::var(descriptor.env_var_name).ok().and_then(|val| {
            if val.is_empty() {
                None
            } else {
                Some(val)
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub pokeapi_host: String,
    pub pokeapi_secure: bool,
    pub fun_translations_host: String,
    pub fun_translations_secure: bool,
    pub port: u16,
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
                Some(s) => match s.to_lowercase().as_str() {
                    "true" => Ok(true),
                    "false" => Ok(false),
                    _ => Err(ConfigError::InvalidFormat(s.clone())),
                },
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
                Some(s) => match s.to_lowercase().as_str() {
                    "true" => Ok(true),
                    "false" => Ok(false),
                    _ => Err(ConfigError::InvalidFormat(s)),
                },
            }
        };
        let port = {
            let desc = &ConfigDescriptor::PORT;
            match parse(desc) {
                None => Ok(DEFAULT_PORT.parse::<u16>().unwrap()),
                Some(s) => s
                    .parse::<u16>()
                    .map_err(|_| ConfigError::InvalidFormat(s))
                    .and_then(|p| {
                        if p == 0 {
                            Err(ConfigError::InvalidFormat(desc.name.to_string()))
                        } else {
                            Ok(p)
                        }
                    }),
            }
        };
        match (
            &pokeapi_host,
            &fun_translations_host,
            &pokeapi_secure,
            &fun_translations_secure,
            &port,
        ) {
            (
                Ok(pokeapi_host),
                Ok(fun_translations_host),
                Ok(pokeapi_secure),
                Ok(fun_translations_secure),
                Ok(port),
            ) => Ok(AppConfig {
                pokeapi_host: pokeapi_host.clone(),
                fun_translations_host: fun_translations_host.clone(),
                pokeapi_secure: *pokeapi_secure,
                fun_translations_secure: *fun_translations_secure,
                port: *port,
            }),
            _ => {
                let errors = [
                    pokeapi_host.err(),
                    fun_translations_host.err(),
                    pokeapi_secure.err(),
                    fun_translations_secure.err(),
                    port.err(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
                Err(ConfigError::Multiple(errors))
            }
        }
    }

    fn validate_host(host: String, name: &str) -> Result<String, ConfigError> {
        HOST_REGEX
            .is_match(&host)
            .then_some(host.clone())
            .ok_or_else(|| ConfigError::InvalidFormat(name.to_string()))
    }
}
