pub mod anime_sama;
pub mod voir_anime;

use anyhow::{Result, anyhow};
use reqwest::Url;
use reqwest::blocking::Client;
use std::fmt;
use std::time::Duration;

pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:132.0) Gecko/20100101 Firefox/132.0";

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
    pub referer: String,
}

#[derive(Debug, Clone)]
pub enum Link {
    EmbedUrl(String),
    Id { id: String, token: String },
}

#[derive(Debug, Clone)]
pub struct Episode {
    pub label: String,
    pub players: Vec<Player>,
    pub page: Option<String>,
}

impl Episode {
    pub fn is_available(&self) -> bool {
        self.page.is_some() || !self.players.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct EpisodeList {
    pub langs: Vec<String>,
    pub lang: String,
    pub episodes: Vec<Episode>,
}

pub trait Source: Send + Sync {
    fn name(&self) -> &'static str;

    fn search(&self, query: &str) -> Result<Vec<Anime>>;

    fn seasons(&self, anime: &Anime) -> Result<Vec<Season>>;

    fn episodes(&self, season: &Season, lang: &str) -> Result<EpisodeList>;

    fn players(&self, episode: &Episode) -> Result<Vec<Player>> {
        Ok(episode.players.clone())
    }

    fn embed_url(&self, player: &Player) -> Result<String>;
}

impl fmt::Display for Season {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

pub fn http_client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(USER_AGENT)
        .cookie_store(true)
        .timeout(Duration::from_secs(20))
        .build()?)
}

pub fn find_live_domain(
    client: &Client,
    site: &str,
    env_var: &str,
    known_domains: &[&str],
) -> Result<String> {
    if let Ok(url) = std::env::var(env_var)
        && !url.trim().is_empty()
    {
        return Ok(url.trim().trim_end_matches('/').to_string());
    }

    for domain in known_domains {
        if let Ok(resp) = client.get(*domain).send()
            && resp.status().is_success()
        {
            return Ok(resp.url().origin().ascii_serialization());
        }
    }

    Err(anyhow!(
        "Impossible de joindre {site} (essayé : {}).\n\
         Si le site a changé de domaine : {env_var}=https://nouveau-domaine ani-dl",
        known_domains.join(", "),
    ))
}

pub fn get_page(client: &Client, url: &str, referer: Option<&str>) -> Result<String> {
    let mut req = client.get(url);
    if let Some(r) = referer {
        req = req.header("Referer", r);
    }
    let resp = req
        .send()
        .map_err(|e| anyhow!("Requête échouée : {url} ({e})"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("{url} a répondu {status}"));
    }
    Ok(resp.text()?)
}

pub fn absolute(url: &str) -> String {
    match url.strip_prefix("//") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_string(),
    }
}

pub fn host_of(url: &str) -> String {
    Url::parse(&absolute(url))
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.trim_start_matches("www.").to_string())
        })
        .unwrap_or_default()
}
