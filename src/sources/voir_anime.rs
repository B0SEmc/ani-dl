use super::{
    Anime, Episode, EpisodeList, Link, Player, Season, Source, absolute, find_live_domain,
    get_page, host_of, http_client,
};
use anyhow::{Context, Result, bail};
use regex::Regex;
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::LazyLock;

const KNOWN_DOMAINS: &[&str] = &["https://voiranime.cx", "https://voir-anime.to"];

pub struct VoirAnime {
    client: Client,
    base: String,
}

#[derive(Deserialize)]
struct SearchResponse {
    success: bool,
    data: Vec<SearchHit>,
}

#[derive(Deserialize)]
struct SearchHit {
    title: String,
    url: String,
    #[serde(rename = "type")]
    kind: String,
}

impl VoirAnime {
    pub fn connect() -> Result<Self> {
        let client = http_client()?;
        let base = find_live_domain(&client, "voiranime", "VOIRANIME_URL", KNOWN_DOMAINS)?;
        Ok(Self { client, base })
    }
}

impl Source for VoirAnime {
    fn name(&self) -> &'static str {
        "VoirAnime"
    }

    fn search(&self, query: &str) -> Result<Vec<Anime>> {
        // The site's search never matches a query containing an apostrophe.
        let fragment = query
            .split(APOSTROPHES)
            .map(str::trim)
            .max_by_key(|part| part.len())
            .unwrap_or(query);
        let wanted = query
            .contains(APOSTROPHES)
            .then(|| without_apostrophes(query));

        let resp: SearchResponse = self
            .client
            .post(format!("{}/wp-admin/admin-ajax.php", self.base))
            .form(&[("action", "wp-manga-search-manga"), ("title", fragment)])
            .send()
            .context("Impossible de contacter voiranime")?
            .error_for_status()?
            .json()?;
        if !resp.success {
            return Ok(Vec::new());
        }
        Ok(resp
            .data
            .into_iter()
            .filter(|hit| hit.kind == "video")
            .map(|hit| Anime {
                name: decode_html(&hit.title),
                url: hit.url,
            })
            .filter(|anime| {
                wanted
                    .as_ref()
                    .is_none_or(|w| without_apostrophes(&anime.name).contains(w))
            })
            .collect())
    }

    fn seasons(&self, anime: &Anime) -> Result<Vec<Season>> {
        let is_vf = anime.url.trim_end_matches('/').ends_with("-vf") || anime.name.contains("(VF)");
        Ok(vec![Season {
            name: anime.name.clone(),
            url: anime.url.clone(),
            default_lang: if is_vf { "vf" } else { "vostfr" }.to_string(),
        }])
    }

    fn episodes(&self, season: &Season, lang: &str) -> Result<EpisodeList> {
        let html = get_page(&self.client, &season.url, None)?;
        let episodes = parse_episode_list(&html);
        if episodes.is_empty() {
            bail!("Aucun épisode trouvé pour cet anime");
        }
        Ok(EpisodeList {
            langs: vec![lang.to_string()],
            lang: lang.to_string(),
            episodes,
        })
    }

    fn players(&self, episode: &Episode) -> Result<Vec<Player>> {
        let Some(page) = &episode.page else {
            return Ok(episode.players.clone());
        };
        let html = get_page(&self.client, page, None)?;
        Ok(parse_players(&html, page))
    }

    fn embed_url(&self, player: &Player) -> Result<String> {
        match &player.link {
            Link::EmbedUrl(url) => Ok(absolute(url)),
            Link::Id { .. } => bail!("lien inattendu pour voiranime"),
        }
    }
}

const APOSTROPHES: [char; 2] = ['\'', '’'];

fn without_apostrophes(text: &str) -> String {
    text.to_lowercase().replace(APOSTROPHES, "")
}

fn decode_html(text: &str) -> String {
    Html::parse_fragment(text)
        .root_element()
        .text()
        .collect::<String>()
        .trim()
        .to_string()
}

fn parse_episode_list(html: &str) -> Vec<Episode> {
    let doc = Html::parse_document(html);
    let link = Selector::parse("li.wp-manga-chapter > a").unwrap();

    let mut episodes: Vec<Episode> = doc
        .select(&link)
        .filter_map(|a| {
            let url = a.value().attr("href")?.to_string();
            let text = a.text().collect::<String>();
            let label = text
                .rsplit(" - ")
                .next()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .unwrap_or(text.trim())
                .to_string();
            Some(Episode {
                label,
                players: Vec::new(),
                page: Some(url),
            })
        })
        .collect();
    episodes.reverse();
    episodes
}

fn parse_players(html: &str, page: &str) -> Vec<Player> {
    static SOURCES: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"thisChapterSources\s*=\s*(\{.*?\})\s*;").unwrap());
    static IFRAME_SRC: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"src=["']([^"']+)["']"#).unwrap());

    let Some(json) = SOURCES.captures(html) else {
        return Vec::new();
    };
    let Ok(sources) = serde_json::from_str::<BTreeMap<String, String>>(&json[1]) else {
        return Vec::new();
    };

    sources
        .values()
        .filter_map(|iframe| IFRAME_SRC.captures(iframe))
        .map(|c| absolute(&c[1]))
        .map(|url| Player {
            host: host_of(&url),
            link: Link::EmbedUrl(url),
            referer: page.to_string(),
        })
        .collect()
}
