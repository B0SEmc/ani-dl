use crate::download::download;
use crate::hosts::best_stream;
use crate::names::{full_title, lang_label, with_lang};
use crate::player::watch;
use crate::sources::{Anime, EpisodeList, Season, Source};
use colored::Colorize;
use inquire::{InquireError, MultiSelect, Select};
use spinners::{Spinner, Spinners};
use std::fmt;
use std::sync::Arc;

#[derive(Clone)]
pub struct Hit {
    pub source: Arc<dyn Source>,
    pub anime: Anime,
}

impl fmt::Display for Hit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let site = format!("[{}]", self.source.name()).dimmed();
        write!(f, "{} {site}", self.anime.name)
    }
}

pub fn answer<T>(res: Result<T, InquireError>) -> Option<T> {
    match res {
        Ok(v) => Some(v),
        Err(InquireError::OperationCanceled) => None,
        Err(InquireError::OperationInterrupted) => std::process::exit(0),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

pub fn with_spinner<T>(msg: &str, f: impl FnOnce() -> anyhow::Result<T>) -> Option<T> {
    let mut spinner = Spinner::new(Spinners::Moon, msg.to_string());
    match f() {
        Ok(v) => {
            spinner.stop_with_symbol(" ✔️ ");
            Some(v)
        }
        Err(e) => {
            spinner.stop_with_symbol(" ❌ ");
            eprintln!("{}", format!("{e:#}").red());
            None
        }
    }
}

pub fn anime_menu(hit: &Hit) {
    let source = &hit.source;
    let Some(seasons) = with_spinner("Chargement des saisons...", || source.seasons(&hit.anime))
    else {
        return;
    };
    match seasons.as_slice() {
        [] => println!("{}", "Aucune saison disponible pour cet anime.".yellow()),
        [season] => season_menu(source, &hit.anime, season),
        _ => {
            while let Some(season) = answer(
                Select::new(
                    "Sélectionnez la saison (Échap pour retour) :",
                    seasons.clone(),
                )
                .prompt(),
            ) {
                season_menu(source, &hit.anime, &season);
            }
        }
    }
}

fn season_menu(source: &Arc<dyn Source>, anime: &Anime, season: &Season) {
    let Some(mut list) = load_episodes(source.as_ref(), season, &season.default_lang) else {
        return;
    };
    if list.langs.len() == 1 {
        return action_menu(source, anime, season, Arc::new(list));
    }
    while let Some(lang) = choose_lang(&list) {
        if lang != list.lang {
            match load_episodes(source.as_ref(), season, &lang) {
                Some(other) => list = other,
                None => continue,
            }
        }
        action_menu(source, anime, season, Arc::new(list.clone()));
    }
}

fn load_episodes(source: &dyn Source, season: &Season, lang: &str) -> Option<EpisodeList> {
    with_spinner("Chargement des épisodes...", || {
        source.episodes(season, lang)
    })
}

fn choose_lang(list: &EpisodeList) -> Option<String> {
    let labels: Vec<String> = list.langs.iter().map(|l| lang_label(l)).collect();
    let current = list.langs.iter().position(|l| *l == list.lang).unwrap_or(0);
    let choice = answer(
        Select::new("Langue (Échap pour retour) :", labels)
            .with_starting_cursor(current)
            .raw_prompt(),
    )?;
    Some(list.langs[choice.index].clone())
}

fn action_menu(source: &Arc<dyn Source>, anime: &Anime, season: &Season, list: Arc<EpisodeList>) {
    let prompt = format!(
        "{} : {} épisodes. Que faire ? (Échap pour retour)",
        with_lang(&full_title(anime, season), &list.lang),
        list.episodes.len()
    );
    while let Some(action) = answer(Select::new(&prompt, vec!["Regarder", "Télécharger"]).prompt())
    {
        if action == "Regarder" {
            watch_menu(source.as_ref(), anime, season, &list);
        } else if let Some(selected) = choose_downloads(&list)
            && let Err(e) = download(
                Arc::clone(source),
                anime,
                season,
                Arc::clone(&list),
                selected,
            )
        {
            eprintln!("{}", format!("Erreur lors du téléchargement : {e:#}").red());
        }
    }
}

fn choose_downloads(list: &EpisodeList) -> Option<Vec<usize>> {
    let selected = answer(
        MultiSelect::new(
            "Épisodes à télécharger (Espace pour choisir, → pour tout, Échap pour retour) :",
            episode_labels(list),
        )
        .raw_prompt(),
    )?;
    if selected.is_empty() {
        println!("{}", "Aucun épisode sélectionné.".yellow());
        return None;
    }
    Some(selected.into_iter().map(|o| o.index).collect())
}

fn watch_menu(source: &dyn Source, anime: &Anime, season: &Season, list: &EpisodeList) {
    let labels = episode_labels(list);
    let mut cursor = 0;
    while let Some(choice) = answer(
        Select::new("Épisode à regarder (Échap pour retour) :", labels.clone())
            .with_starting_cursor(cursor)
            .raw_prompt(),
    ) {
        let episode = &list.episodes[choice.index];
        let msg = format!(
            "Recherche du meilleur lecteur pour l'épisode {}",
            episode.label
        );
        cursor = choice.index;
        let Some(stream) = with_spinner(&msg, || best_stream(source, episode)) else {
            continue;
        };
        if let Some(height) = stream.height {
            println!("{}", format!("Qualité : {height}p").cyan());
        }
        watch(
            &stream,
            &format!("{} - Épisode {}", full_title(anime, season), episode.label),
        );
        cursor = (choice.index + 1).min(labels.len() - 1);
    }
}

fn episode_labels(list: &EpisodeList) -> Vec<String> {
    list.episodes
        .iter()
        .map(|e| {
            if e.is_available() {
                format!("Épisode {}", e.label)
            } else {
                format!("Épisode {} (indisponible)", e.label)
            }
        })
        .collect()
}
