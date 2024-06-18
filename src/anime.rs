use colored::*;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use titlecase::titlecase;

#[derive(Serialize, Deserialize, Debug)]
pub struct Animes {
    pub anime: Vec<Anime>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Anime {
    name: String,
    pub lang: String,
    season: i8,
    pub episodes: Vec<String>,
}

impl Display for Anime {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "saison {}", self.season.to_string().yellow(),)
    }
}

impl Animes {
    pub fn pretty_names(mut self) -> Self {
        for anime in &mut self.anime {
            anime.name = titlecase(&anime.name.replace('-', " "));
        }
        self
    }
    pub fn get_name(&self) -> Vec<String> {
        let mut names = Vec::new();
        for anime in &self.anime {
            if !names.contains(&anime.name) {
                names.push(anime.name.clone());
            }
        }
        names
    }
    pub fn get_seasons_from_str(&self, name: &str) -> Vec<Anime> {
        let mut seasons = vec![];
        for anime in &self.anime {
            if anime.name == name {
                seasons.push(anime.clone());
            }
        }
        seasons
    }
}
