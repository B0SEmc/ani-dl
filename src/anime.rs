use colored::*;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Serialize, Deserialize, Debug)]
pub struct Medias {
    pub media: Vec<Media>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Media {
    pub name: String,
    pub lang: String,
    pub season: i8,
    media_type: String,
    pub episodes: Vec<String>,
}

impl Display for Media {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "saison {}", self.season.to_string().yellow())
    }
}

impl Medias {
    pub fn get_name(&self) -> Vec<String> {
        let mut names = Vec::new();
        for anime in &self.media {
            if !names.contains(&anime.name) {
                names.push(anime.name.clone());
            }
        }
        names
    }
    pub fn get_seasons_from_str(self, name: &str) -> Vec<Media> {
        self.media.into_iter().filter(|x| x.name == name).collect()
    }
}
