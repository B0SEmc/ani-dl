pub mod anime_sama;

/// A search result from a source catalog.
#[derive(Debug, Clone)]
pub struct AnimeResult {
    pub name: String,
    pub slug: String,
    pub url: String,
}

/// Season info with available languages.
#[derive(Debug, Clone)]
pub struct SeasonInfo {
    pub number: i8,
    pub langs: Vec<String>,
}

/// Trait that any anime source must implement.
pub trait Source {
    /// Search for animes by name. Returns matching results.
    fn search(&self, query: &str) -> anyhow::Result<Vec<AnimeResult>>;

    /// Get available seasons and their languages for a given anime.
    fn get_seasons(&self, anime: &AnimeResult) -> anyhow::Result<Vec<SeasonInfo>>;

    /// Get episode URLs for a specific anime, season and language.
    fn get_episodes(
        &self,
        anime: &AnimeResult,
        season: i8,
        lang: &str,
    ) -> anyhow::Result<Vec<String>>;
}

impl std::fmt::Display for AnimeResult {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl std::fmt::Display for SeasonInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let langs = self.langs.join(", ");
        write!(f, "Saison {} ({})", self.number, langs)
    }
}
