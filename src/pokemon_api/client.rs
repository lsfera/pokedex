use crate::{
    constants::DEFAULT_LANGUAGE,
    http::client::{HttpClientError, TranslatorType},
};
use async_trait::async_trait;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pokemon {
    pub id: i32,
    pub name: String,
    pub habitat: Option<String>,
    #[serde(rename = "isLegendary")]
    pub is_legendary: bool,
    pub description: Option<String>,
}

impl Pokemon {
    pub fn get_translator(&self) -> TranslatorType {
        match (self.habitat.as_deref(), self.is_legendary) {
            (Some("cave"), _) => TranslatorType::Yoda,
            (_, true) => TranslatorType::Yoda,
            _ => TranslatorType::Shakespeare,
        }
    }
}

pub type PokemonResult = Result<Pokemon, HttpClientError>;

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

#[async_trait]
pub trait PokemonApi: Send + Sync {
    async fn get_pokemon(&self, name: &str) -> PokemonResult;
}

#[async_trait]
pub trait PokemonApiProxy: Send + Sync {
    async fn get_base_pokemon(&self, name: &str) -> Result<BasePokemonResponse, HttpClientError>;
    async fn get_species(&self, species_url: &str) -> Result<SpeciesResponse, HttpClientError>;
}

pub struct PokemonApiProxyClient {
    client: reqwest::Client,
    base_url: String,
}

impl PokemonApiProxyClient {
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
                // NOTE: by default redirects followed automatically by reqwest::Client: https://docs.rs/reqwest/latest/reqwest/#redirect-policies
                _ => Ok(r),
            })?
            .json::<BasePokemonResponse>()
            .await
            .map_err(|_| HttpClientError::ParseError)
    }
}

pub struct PokeApiClient {
    client: Box<dyn PokemonApiProxy + Send + Sync>,
}

impl PokeApiClient {
    pub fn new(client: Box<dyn PokemonApiProxy + Send + Sync>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl PokemonApi for PokeApiClient {
    async fn get_pokemon(&self, name: &str) -> PokemonResult {
        let BasePokemonResponse { id, name, species } = self.client.get_base_pokemon(name).await?;
        let SpeciesResponse {
            habitat,
            is_legendary,
            flavor_text_entries,
        } = self.client.get_species(&species.url).await?;
        let flavor_texts: HashMap<&str, &str> = flavor_text_entries
            .iter()
            .map(|entry| (entry.language.name.as_str(), entry.flavor_text.as_str()))
            .collect();
        let description = flavor_texts
            .get(DEFAULT_LANGUAGE)
            .map(|text| text.to_string());
        match flavor_text_entries.first() {
            // descriptions are empty
            None => Err(HttpClientError::NotFound),
            // no description found from requested languages
            Some(first) => Ok(Pokemon {
                id,
                name,
                habitat: habitat.map(|h| h.name),
                is_legendary,
                description: description.or_else(|| Some(first.flavor_text.to_string())),
            }),
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

        let pokemon = client.get_pokemon("pikachu").await.unwrap();

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

        let pokemon = client.get_pokemon("pikachu").await.unwrap();

        assert_eq!(
            pokemon.description.as_deref(),
            Some("Descripcion por defecto.")
        );
    }

    #[tokio::test]
    async fn returns_not_found_when_no_descriptions() {
        let client = make_client(vec![]);

        let result = client.get_pokemon("pikachu").await;

        assert!(matches!(result, Err(HttpClientError::NotFound)));
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
