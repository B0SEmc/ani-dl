use colored::Colorize;
use inquire::{Select, Text};
use menu::{Hit, anime_menu, answer, with_spinner};
use sources::Source;
use sources::anime_sama::AnimeSama;
use sources::voir_anime::VoirAnime;
use std::sync::Arc;

mod download;
mod hosts;
mod menu;
mod names;
mod player;
mod sources;

fn connect_sources() -> Vec<Arc<dyn Source>> {
    let mut sources: Vec<Arc<dyn Source>> = Vec::new();
    if let Some(s) = with_spinner("Connexion à voiranime", VoirAnime::connect) {
        sources.push(Arc::new(s));
    }
    if let Some(s) = with_spinner("Connexion à anime-sama", AnimeSama::connect) {
        sources.push(Arc::new(s));
    }
    sources
}

fn search_all(sources: &[Arc<dyn Source>], query: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    for source in sources {
        let msg = format!("Recherche de \"{query}\" sur {}", source.name());
        if let Some(results) = with_spinner(&msg, || source.search(query)) {
            hits.extend(results.into_iter().map(|anime| Hit {
                source: Arc::clone(source),
                anime,
            }));
        }
    }
    hits
}

fn main() {
    let sources = connect_sources();
    if sources.is_empty() {
        std::process::exit(1);
    }

    while let Some(query) = answer(Text::new("Rechercher un anime (Échap pour quitter) :").prompt())
    {
        let query = query.trim();
        if query.is_empty() {
            continue;
        }
        let hits = search_all(&sources, query);
        if hits.is_empty() {
            println!("{}", "Aucun résultat.".yellow());
            continue;
        }
        while let Some(hit) = answer(
            Select::new("Sélectionnez un anime (Échap pour retour) :", hits.clone()).prompt(),
        ) {
            anime_menu(&hit);
        }
    }
}
