pub mod anime_sama;

use std::fmt;

#[derive(Debug, Clone)]
pub struct Anime {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct Season {
    pub name: String,
    pub url: String,
    pub default_lang: String,
}

#[derive(Debug, Clone)]
pub struct Player {
    pub host: String,
    pub link: Link,
}

#[derive(Debug, Clone)]
pub enum Link {
    EmbedUrl(String),
    Id {
        id: String,
        token: String,
        referer: String,
    },
}

#[derive(Debug, Clone)]
pub struct Episode {
    pub number: usize,
    pub players: Vec<Player>,
}

#[derive(Debug, Clone)]
pub struct EpisodeList {
    pub langs: Vec<String>,
    pub lang: String,
    pub referer: String,
    pub episodes: Vec<Episode>,
}

pub trait Source: Send + Sync {
    fn search(&self, query: &str) -> anyhow::Result<Vec<Anime>>;

    fn seasons(&self, anime: &Anime) -> anyhow::Result<Vec<Season>>;

    fn episodes(&self, season: &Season, lang: &str) -> anyhow::Result<EpisodeList>;

    fn embed_url(&self, player: &Player) -> anyhow::Result<String>;
}

impl fmt::Display for Anime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl fmt::Display for Season {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}
