//! # Pokemon API Client
//!
//! This module handles integration with the [PokéAPI](https://pokeapi.co/) including:
//! - Fetching base Pokémon data
//! - Retrieving species information with flavor text descriptions
//! - Language negotiation with fallback support
//! - Automatic translator type selection based on Pokémon characteristics
//!
//! ## Language Negotiation
//!
//! The module supports RFC 7231 language negotiation with the following behavior:
//! 1. Attempts to find a description in requested languages (in order)
//! 2. Falls back to English if available and wildcard is present
//! 3. Falls back to first available language if no match and wildcard is present
//! 4. Returns `NotAcceptable` error if no suitable language found and no wildcard
//!
//! ## Translator Selection
//!
//! Translator type is automatically determined by the Pokémon's characteristics:
//! - **Yoda translator**: Legendary Pokémon or cave habitat
//! - **Shakespeare translator**: All other Pokémon

use crate::{
    constants::DEFAULT_LANGUAGE,
    http::client::{HttpClientError, TranslatorType},
};
use async_trait::async_trait;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, instrument};
use utoipa::ToSchema;

/// A Pokémon with enriched data including descriptions and characteristics.
///
/// This struct represents a Pokémon fetched from PokéAPI with additional metadata
/// used for determining translation style.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Pokemon {
    /// Pokemon ID
    pub id: i32,
    /// Pokemon name
    pub name: String,
    /// Pokemon habitat (e.g., cave, forest)
    pub habitat: Option<String>,
    /// Whether the Pokemon is legendary
    #[serde(rename = "isLegendary")]
    pub is_legendary: bool,
    /// Pokemon description
    pub description: Option<String>,
}

impl Pokemon {
    /// Determines the appropriate translator type based on Pokémon characteristics.
    ///
    /// # Returns
    ///
    /// - `TranslatorType::Yoda` if the Pokémon is legendary or lives in caves
    /// - `TranslatorType::Shakespeare` for all other Pokémon
    pub fn get_translator(&self) -> TranslatorType {
        match (self.habitat.as_deref(), self.is_legendary) {
            (Some("cave"), _) => TranslatorType::Yoda,
            (_, true) => TranslatorType::Yoda,
            _ => TranslatorType::Shakespeare,
        }
    }
}

/// Result type for Pokémon API operations.
///
/// Returns a tuple of (language, Pokemon) on success, containing the language
/// of the returned Pokémon description.
pub type PokemonResult = Result<(String, Pokemon), HttpClientError>;

/// Response from PokéAPI `/pokemon/{name}` endpoint.
#[derive(Debug, Deserialize)]
pub struct BasePokemonResponse {
    id: i32, // NOTE: i32 should be enough: there are many pokemon out there, but not that many!
    name: String,
    species: SpeciesReference,
}

#[derive(Debug, Deserialize)]
struct SpeciesReference {
    url: String,
}

/// Response from PokéAPI `/pokemon-species/{id}` endpoint.
///
/// Contains species-level metadata including habitat, legendary status,
/// and multilingual flavor text descriptions.
#[derive(Debug, Deserialize)]
pub struct SpeciesResponse {
    habitat: Option<HabitatReference>,
    is_legendary: bool,
    flavor_text_entries: Vec<FlavorTextEntry>,
}

#[derive(Debug, Deserialize)]
struct HabitatReference {
    name: String,
}

#[derive(Debug, Deserialize)]
struct FlavorTextEntry {
    flavor_text: String,
    language: LanguageReference,
}

#[derive(Debug, Deserialize)]
pub struct LanguageReference {
    name: String,
}

/// Trait for fetching Pokémon data with language negotiation.
///
/// Implementations handle the complete workflow of fetching base data,
/// species information, and selecting descriptions in requested languages.
#[async_trait]
pub trait PokemonApi: Send + Sync {
    /// Fetches a Pokémon with language negotiation.
    ///
    /// # Arguments
    ///
    /// * `name` - Pokémon name to fetch (case-insensitive)
    /// * `languages` - List of preferred languages in priority order
    /// * `has_wildcard` - Whether `Accept-Language` contains wildcard (`*`)
    ///
    /// # Returns
    ///
    /// Returns `(language, Pokemon)` where language is the language code
    /// of the returned description.
    ///
    /// # Errors
    ///
    /// - `NotFound` if Pokémon doesn't exist or has no descriptions
    /// - `NotAcceptable` if no description in requested languages and no wildcard
    /// - `RequestFailed` or `ParseError` on API communication issues
    async fn get_pokemon(
        &self,
        name: &str,
        languages: &[String],
        has_wildcard: bool,
    ) -> PokemonResult;
}

/// Low-level trait for making HTTP requests to PokéAPI.
///
/// This trait abstracts the HTTP layer, allowing for easy testing with mocks.
#[async_trait]
pub trait PokemonApiProxy: Send + Sync {
    /// Fetches base Pokémon data from the `/pokemon/{name}` endpoint.
    async fn get_base_pokemon(&self, name: &str) -> Result<BasePokemonResponse, HttpClientError>;
    /// Fetches species data from the `/pokemon-species/{id}` endpoint.
    async fn get_species(&self, species_url: &str) -> Result<SpeciesResponse, HttpClientError>;
}

/// HTTP client implementation for PokéAPI requests.
///
/// Handles HTTP communication with PokéAPI including error handling and status code interpretation.
pub struct PokemonApiProxyClient {
    client: reqwest::Client,
    base_url: String,
}

impl PokemonApiProxyClient {
    /// Creates a new PokéAPI HTTP client.
    ///
    /// # Arguments
    ///
    /// * `client` - Configured reqwest client
    /// * `base_url` - Base URL for PokéAPI (e.g., `https://pokeapi.co/api/v2`)
    pub fn new(client: reqwest::Client, base_url: String) -> Self {
        PokemonApiProxyClient { client, base_url }
    }
}
#[async_trait]
impl PokemonApiProxy for PokemonApiProxyClient {
    async fn get_species(&self, species_url: &str) -> Result<SpeciesResponse, HttpClientError> {
        self.client
            .get(species_url)
            .send()
            .await
            .map_err(|_| HttpClientError::RequestFailed)
            .and_then(|r| match r.status() {
                StatusCode::NOT_FOUND => Err(HttpClientError::NotFound),
                StatusCode::SERVICE_UNAVAILABLE => Err(HttpClientError::ServiceUnavailable),
                // NOTE: by default redirects followed automatically by reqwest::Client: https://docs.rs/reqwest/latest/reqwest/#redirect-policies
                _ => Ok(r),
            })?
            .json::<SpeciesResponse>()
            .await
            .map_err(|_| HttpClientError::ParseError)
    }

    async fn get_base_pokemon(&self, name: &str) -> Result<BasePokemonResponse, HttpClientError> {
        self.client
            .get(format!("{}/pokemon/{}", self.base_url, name))
            .send()
            .await
            .map_err(|_| HttpClientError::RequestFailed)
            .and_then(|r| match r.status() {
                StatusCode::NOT_FOUND => Err(HttpClientError::NotFound),
                StatusCode::SERVICE_UNAVAILABLE => Err(HttpClientError::ServiceUnavailable),
                // NOTE: by default redirects followed automatically by reqwest::Client: https://docs.rs/reqwest/latest/reqwest/#redirect-policies
                _ => Ok(r),
            })?
            .json::<BasePokemonResponse>()
            .await
            .map_err(|_| HttpClientError::ParseError)
    }
}

/// High-level Pokémon API client with language negotiation.
///
/// Coordinates fetching base Pokémon data, species information, and selecting
/// descriptions based on requested languages with intelligent fallback.
pub struct PokeApiClient {
    client: Box<dyn PokemonApiProxy + Send + Sync>,
}

impl PokeApiClient {
    /// Creates a new Pokémon API client.
    ///
    /// # Arguments
    ///
    /// * `client` - HTTP proxy implementation for making requests
    pub fn new(client: Box<dyn PokemonApiProxy + Send + Sync>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl PokemonApi for PokeApiClient {
    #[instrument(skip(self), fields(pokemon_name = %name))]
    async fn get_pokemon(
        &self,
        name: &str,
        languages: &[String],
        has_wildcard: bool,
    ) -> PokemonResult {
        debug!("Fetching base pokemon data");
        let BasePokemonResponse { id, name, species } = self.client.get_base_pokemon(name).await?;

        debug!(pokemon_id = id, species_url = %species.url, "Fetching species data");
        let SpeciesResponse {
            habitat,
            is_legendary,
            flavor_text_entries,
        } = self.client.get_species(&species.url).await?;
        debug!(
            available_languages = ?flavor_text_entries.iter().map(|e| &e.language.name).collect::<Vec<_>>(),
            "Processing language descriptions"
        );

        let flavor_texts: HashMap<&str, &str> = flavor_text_entries
            .iter()
            .map(|entry| (entry.language.name.as_str(), entry.flavor_text.as_str()))
            .collect();
        let description = languages
            .iter()
            .find_map(|lang| flavor_texts.get_key_value(lang.as_str()))
            .or_else(|| flavor_texts.get_key_value(DEFAULT_LANGUAGE))
            .map(|(lang, text)| (lang.to_string(), text.to_string()));
        let not_acceptable = matches!((&description, has_wildcard), (None, false));
        match (flavor_text_entries.first(), not_acceptable) {
            // descriptions are empty
            (None, _) => {
                debug!("No descriptions available");
                Err(HttpClientError::NotFound)
            }
            // no description found from requested languages and no wildcard to fall back on
            (_, true) => {
                debug!("Requested language not available and no wildcard");
                Err(HttpClientError::NotAcceptable)
            }
            (Some(first), false) => {
                let (lang, desc) = if let Some((l, t)) = description {
                    debug!(selected_language = %l, "Using requested language");
                    (l, t)
                } else {
                    debug!(fallback_language = %first.language.name, "Using fallback language");
                    (first.language.name.clone(), first.flavor_text.clone())
                };
                Ok((
                    lang,
                    Pokemon {
                        id,
                        name,
                        habitat: habitat.map(|h| h.name),
                        is_legendary,
                        description: Some(desc),
                    },
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockBaseClient {
        base: BasePokemonResponse,
        species: SpeciesResponse,
    }

    #[async_trait]
    impl PokemonApiProxy for MockBaseClient {
        async fn get_base_pokemon(
            &self,
            _name: &str,
        ) -> Result<BasePokemonResponse, HttpClientError> {
            Ok(BasePokemonResponse {
                id: self.base.id,
                name: self.base.name.clone(),
                species: SpeciesReference {
                    url: self.base.species.url.clone(),
                },
            })
        }

        async fn get_species(
            &self,
            _species_url: &str,
        ) -> Result<SpeciesResponse, HttpClientError> {
            Ok(SpeciesResponse {
                habitat: self.species.habitat.as_ref().map(|h| HabitatReference {
                    name: h.name.clone(),
                }),
                is_legendary: self.species.is_legendary,
                flavor_text_entries: self
                    .species
                    .flavor_text_entries
                    .iter()
                    .map(|f| FlavorTextEntry {
                        flavor_text: f.flavor_text.clone(),
                        language: LanguageReference {
                            name: f.language.name.clone(),
                        },
                    })
                    .collect(),
            })
        }
    }

    fn make_client(flavor_entries: Vec<FlavorTextEntry>) -> PokeApiClient {
        let base = BasePokemonResponse {
            id: 25,
            name: "pikachu".to_string(),
            species: SpeciesReference {
                url: "https://pokeapi.co/api/v2/pokemon-species/25".to_string(),
            },
        };
        let species = SpeciesResponse {
            habitat: Some(HabitatReference {
                name: "forest".to_string(),
            }),
            is_legendary: false,
            flavor_text_entries: flavor_entries,
        };

        let mock = MockBaseClient { base, species };
        PokeApiClient::new(Box::new(mock))
    }

    #[tokio::test]
    async fn returns_english_description_when_present() {
        let client = make_client(vec![
            FlavorTextEntry {
                flavor_text: "A forest mouse.".to_string(),
                language: LanguageReference {
                    name: DEFAULT_LANGUAGE.to_string(),
                },
            },
            FlavorTextEntry {
                flavor_text: "Una descripcion.".to_string(),
                language: LanguageReference {
                    name: "es".to_string(),
                },
            },
        ]);

        let (_lang, pokemon) = client
            .get_pokemon("pikachu", &["en".to_string()], false)
            .await
            .unwrap();

        assert_eq!(pokemon.name, "pikachu");
        assert_eq!(pokemon.habitat.as_deref(), Some("forest"));
        assert!(!pokemon.is_legendary);
        assert_eq!(pokemon.description.as_deref(), Some("A forest mouse."));
    }

    #[tokio::test]
    async fn falls_back_to_first_description_when_no_english() {
        let client = make_client(vec![FlavorTextEntry {
            flavor_text: "Descripcion por defecto.".to_string(),
            language: LanguageReference {
                name: "es".to_string(),
            },
        }]);

        // Should return NotAcceptable if no wildcard and language not present
        let result = client
            .get_pokemon("pikachu", &["en".to_string()], false)
            .await;
        assert!(matches!(result, Err(HttpClientError::NotAcceptable)));

        // Should fall back to first if wildcard is allowed
        let (_lang, pokemon) = client.get_pokemon("pikachu", &[], true).await.unwrap();
        assert_eq!(
            pokemon.description.as_deref(),
            Some("Descripcion por defecto.")
        );
    }

    #[tokio::test]
    async fn returns_not_found_when_no_descriptions() {
        let client = make_client(vec![]);

        let result = client
            .get_pokemon("pikachu", &["en".to_string()], false)
            .await;

        assert!(matches!(result, Err(HttpClientError::NotFound)));
    }

    #[tokio::test]
    async fn returns_not_acceptable_when_language_not_available_and_no_wildcard() {
        let client = make_client(vec![FlavorTextEntry {
            flavor_text: "Beschreibung auf Deutsch.".to_string(),
            language: LanguageReference {
                name: "de".to_string(),
            },
        }]);

        let result = client
            .get_pokemon("pikachu", &["fr".to_string()], false)
            .await;

        assert!(matches!(result, Err(HttpClientError::NotAcceptable)));
    }

    struct MockServiceUnavailableClient;

    #[async_trait]
    impl PokemonApiProxy for MockServiceUnavailableClient {
        async fn get_base_pokemon(
            &self,
            _name: &str,
        ) -> Result<BasePokemonResponse, HttpClientError> {
            Err(HttpClientError::ServiceUnavailable)
        }

        async fn get_species(
            &self,
            _species_url: &str,
        ) -> Result<SpeciesResponse, HttpClientError> {
            Err(HttpClientError::ServiceUnavailable)
        }
    }

    #[tokio::test]
    async fn returns_service_unavailable_on_base_pokemon_unavailable() {
        let client = PokeApiClient::new(Box::new(MockServiceUnavailableClient));

        let result = client
            .get_pokemon("pikachu", &["en".to_string()], false)
            .await;

        assert!(matches!(result, Err(HttpClientError::ServiceUnavailable)));
    }

    #[tokio::test]
    async fn returns_service_unavailable_on_species_unavailable() {
        let base = BasePokemonResponse {
            id: 25,
            name: "pikachu".to_string(),
            species: SpeciesReference {
                url: "https://pokeapi.co/api/v2/pokemon-species/25".to_string(),
            },
        };

        struct MockPartiallyUnavailableClient {
            base: BasePokemonResponse,
        }

        #[async_trait]
        impl PokemonApiProxy for MockPartiallyUnavailableClient {
            async fn get_base_pokemon(
                &self,
                _name: &str,
            ) -> Result<BasePokemonResponse, HttpClientError> {
                Ok(BasePokemonResponse {
                    id: self.base.id,
                    name: self.base.name.clone(),
                    species: SpeciesReference {
                        url: self.base.species.url.clone(),
                    },
                })
            }

            async fn get_species(
                &self,
                _species_url: &str,
            ) -> Result<SpeciesResponse, HttpClientError> {
                Err(HttpClientError::ServiceUnavailable)
            }
        }

        let client = PokeApiClient::new(Box::new(MockPartiallyUnavailableClient { base }));

        let result = client
            .get_pokemon("pikachu", &["en".to_string()], false)
            .await;

        assert!(matches!(result, Err(HttpClientError::ServiceUnavailable)));
    }

    struct MockRateLimitedClient;

    #[async_trait]
    impl PokemonApiProxy for MockRateLimitedClient {
        async fn get_base_pokemon(
            &self,
            _name: &str,
        ) -> Result<BasePokemonResponse, HttpClientError> {
            Err(HttpClientError::RateLimited)
        }

        async fn get_species(
            &self,
            _species_url: &str,
        ) -> Result<SpeciesResponse, HttpClientError> {
            Err(HttpClientError::RateLimited)
        }
    }

    #[tokio::test]
    async fn returns_rate_limited_on_base_pokemon_rate_limited() {
        let client = PokeApiClient::new(Box::new(MockRateLimitedClient));

        let result = client
            .get_pokemon("pikachu", &["en".to_string()], false)
            .await;

        assert!(matches!(result, Err(HttpClientError::RateLimited)));
    }

    mod get_translator_tests {
        use super::*;

        #[test]
        fn returns_yoda_for_cave_habitat() {
            let pokemon = Pokemon {
                id: 1,
                name: "zubat".to_string(),
                habitat: Some("cave".to_string()),
                is_legendary: false,
                description: None,
            };

            assert_eq!(pokemon.get_translator(), TranslatorType::Yoda);
        }

        #[test]
        fn returns_yoda_for_legendary_pokemon() {
            let pokemon = Pokemon {
                id: 144,
                name: "articuno".to_string(),
                habitat: Some("sky".to_string()),
                is_legendary: true,
                description: None,
            };

            assert_eq!(pokemon.get_translator(), TranslatorType::Yoda);
        }

        #[test]
        fn returns_yoda_for_cave_habitat_and_legendary() {
            let pokemon = Pokemon {
                id: 150,
                name: "mewtwo".to_string(),
                habitat: Some("cave".to_string()),
                is_legendary: true,
                description: None,
            };

            assert_eq!(pokemon.get_translator(), TranslatorType::Yoda);
        }

        #[test]
        fn returns_shakespeare_for_non_cave_non_legendary() {
            let pokemon = Pokemon {
                id: 25,
                name: "pikachu".to_string(),
                habitat: Some("forest".to_string()),
                is_legendary: false,
                description: None,
            };

            assert_eq!(pokemon.get_translator(), TranslatorType::Shakespeare);
        }

        #[test]
        fn returns_shakespeare_for_no_habitat_non_legendary() {
            let pokemon = Pokemon {
                id: 132,
                name: "ditto".to_string(),
                habitat: None,
                is_legendary: false,
                description: None,
            };

            assert_eq!(pokemon.get_translator(), TranslatorType::Shakespeare);
        }
    }
}
