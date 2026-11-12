use crate::sources::{Anime, Season};
use std::path::{Path, PathBuf};

pub fn lang_label(lang: &str) -> String {
    lang.to_uppercase()
}

pub fn with_lang(name: &str, lang: &str) -> String {
    let tag = format!("({})", lang_label(lang));
    if name.to_uppercase().contains(&tag) {
        name.to_string()
    } else {
        format!("{name} {tag}")
    }
}

pub fn full_title(anime: &Anime, season: &Season) -> String {
    if season.name == anime.name {
        anime.name.clone()
    } else {
        format!("{} - {}", anime.name, season.name)
    }
}

pub struct DownloadTarget {
    pub dir: PathBuf,
    file_prefix: String,
}

impl DownloadTarget {
    pub fn new(anime: &Anime, season: &Season, lang: &str) -> Self {
        let anime_name = sanitize(&to_title_case(&anime.name));
        let season_name = sanitize(&season.name);
        let tag = |name: &str| {
            if lang == "vostfr" {
                name.to_string()
            } else {
                with_lang(name, lang)
            }
        };

        if season_name.eq_ignore_ascii_case(&anime_name) {
            Self {
                dir: PathBuf::from(tag(&anime_name)),
                file_prefix: anime_name,
            }
        } else {
            Self {
                dir: Path::new(&anime_name).join(tag(&season_name)),
                file_prefix: format!("{anime_name} - {season_name}"),
            }
        }
    }

    pub fn output_template(&self, episode_label: &str) -> String {
        self.dir
            .join(format!(
                "{} E{:0>2}.%(ext)s",
                self.file_prefix,
                sanitize(episode_label)
            ))
            .display()
            .to_string()
    }
}

fn to_title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn sanitize(name: &str) -> String {
    let spaced: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let collapsed = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim_end_matches(['.', ' ']).to_string()
}
