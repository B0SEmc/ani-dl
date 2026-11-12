use super::{Anime, Episode, EpisodeList, Link, Player, Season, Source};
use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use reqwest::Url;
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use std::time::Duration;

const KNOWN_DOMAINS: &[&str] = &[
    "https://anime-sama.to",
    "https://anime-sama.si",
    "https://anime-sama.org",
    "https://anime-sama.fr",
];

const ENV_URL: &str = "ANIME_SAMA_URL";

pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:132.0) Gecko/20100101 Firefox/132.0";

pub struct AnimeSama {
    client: Client,
    base: String,
}

impl AnimeSama {
    pub fn connect() -> Result<Self> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .cookie_store(true)
            .timeout(Duration::from_secs(20))
            .build()?;

        if let Ok(url) = std::env::var(ENV_URL)
            && !url.trim().is_empty()
        {
            let base = url.trim().trim_end_matches('/').to_string();
            return Ok(Self { client, base });
        }

        for domain in KNOWN_DOMAINS {
            let Ok(resp) = client.get(*domain).send() else {
                continue;
            };
            if resp.status().is_success() {
                let base = resp.url().origin().ascii_serialization();
                return Ok(Self { client, base });
            }
        }

        Err(anyhow!(
            "Impossible de joindre anime-sama (essayé : {}).\n\
             Si le site a changé de domaine : {}=https://nouveau-domaine ani-dl",
            KNOWN_DOMAINS.join(", "),
            ENV_URL
        ))
    }

    fn get(&self, url: &str, referer: Option<&str>) -> Result<String> {
        let mut req = self.client.get(url);
        if let Some(r) = referer {
            req = req.header("Referer", r);
        }
        let resp = req
            .send()
            .with_context(|| format!("Requête échouée : {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            bail!("{url} a répondu {status}");
        }
        Ok(resp.text()?)
    }
}

impl Source for AnimeSama {
    fn search(&self, query: &str) -> Result<Vec<Anime>> {
        let html = self
            .client
            .post(format!("{}/template-php/defaut/fetch.php", self.base))
            .form(&[("query", query)])
            .send()
            .context("Impossible de contacter anime-sama")?
            .error_for_status()?
            .text()?;
        Ok(parse_search(&html, &self.base))
    }

    fn seasons(&self, anime: &Anime) -> Result<Vec<Season>> {
        let html = self.get(&anime.url, None)?;
        parse_seasons(&html, &anime.url)
    }

    fn episodes(&self, season: &Season, lang: &str) -> Result<EpisodeList> {
        let url = format!("{}{}/", season.url, lang);
        let html = self.get(&url, None)?;
        let list = parse_season_page(&html, &url, lang);
        if list.episodes.is_empty() {
            bail!("Aucun épisode trouvé pour cette saison");
        }
        Ok(list)
    }

    fn embed_url(&self, player: &Player) -> Result<String> {
        match &player.link {
            Link::EmbedUrl(url) => Ok(absolute(url)),
            Link::Id { id, token, referer } => {
                let mut api = Url::parse(&format!("{}/api/lien.php", self.base))?;
                api.query_pairs_mut()
                    .append_pair("j", token)
                    .append_pair("l", id);

                let mut last_err = anyhow!("réponse vide");
                for attempt in 0..2 {
                    if attempt > 0 {
                        std::thread::sleep(Duration::from_secs(1));
                    }
                    let body = match self.get(api.as_str(), Some(referer)) {
                        Ok(b) => b,
                        Err(e) => {
                            last_err = e;
                            continue;
                        }
                    };
                    match serde_json::from_str::<Value>(&body) {
                        Ok(v) => {
                            if let Some(u) = v.get("u").and_then(Value::as_str) {
                                return Ok(absolute(u));
                            }
                            let err = v.get("erreur").and_then(Value::as_str).unwrap_or("?");
                            last_err = anyhow!("anime-sama a refusé le lien ({err})");
                            if err == "absent" || err == "expire" {
                                break;
                            }
                        }
                        Err(_) => last_err = anyhow!("réponse inattendue d'anime-sama"),
                    }
                }
                Err(last_err)
            }
        }
    }
}

fn absolute(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        url.to_string()
    }
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid regex")
}

fn quoted_items(body: &str) -> Vec<String> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re(r#""([^"]*)"|'([^']*)'"#));
    RE.captures_iter(body)
        .map(|c| c.get(1).or(c.get(2)).map_or("", |m| m.as_str()).to_string())
        .collect()
}

fn parse_search(html: &str, base: &str) -> Vec<Anime> {
    let doc = Html::parse_fragment(html);
    let card = Selector::parse("a.asn-search-result").unwrap();
    let title = Selector::parse(".asn-search-result-title").unwrap();
    let meta = Selector::parse(".asn-search-meta").unwrap();

    let mut seen = HashSet::new();
    let mut results = Vec::new();

    for el in doc.select(&card) {
        let Some(href) = el.value().attr("href") else {
            continue;
        };
        let name = el
            .select(&title)
            .next()
            .map(|t| t.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }

        if let Some(m) = el.select(&meta).next() {
            let text = m.text().collect::<String>();
            let has_video = text
                .split('·')
                .map(str::trim)
                .any(|p| !p.is_empty() && p != "Scans" && !p.chars().all(|c| c.is_ascii_digit()));
            if !has_video {
                continue;
            }
        }

        let mut url = match Url::parse(base).and_then(|b| b.join(href)) {
            Ok(u) => u.to_string(),
            Err(_) => continue,
        };
        if !url.ends_with('/') {
            url.push('/');
        }
        if seen.insert(url.clone()) {
            results.push(Anime { name, url });
        }
    }
    results
}

// The site hides a season by commenting out its panneauAnime() line.
fn strip_comments(src: &str) -> String {
    static BLOCK: LazyLock<Regex> = LazyLock::new(|| re(r"(?s)/\*.*?\*/|<!--.*?-->"));
    BLOCK
        .replace_all(src, "")
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_seasons(html: &str, anime_url: &str) -> Result<Vec<Season>> {
    static PANNEAU: LazyLock<Regex> = LazyLock::new(|| {
        re(r#"panneauAnime\s*\(\s*(?:"([^"]*)"|'([^']*)')\s*,\s*(?:"([^"]*)"|'([^']*)')\s*\)"#)
    });

    let base = Url::parse(anime_url)?;
    let src = strip_comments(html);
    let mut seen = HashSet::new();
    let mut seasons = Vec::new();

    for c in PANNEAU.captures_iter(&src) {
        let name = c.get(1).or(c.get(2)).map_or("", |m| m.as_str()).trim();
        let path = c.get(3).or(c.get(4)).map_or("", |m| m.as_str()).trim();
        let path = path.trim_end_matches('/');
        let Some((season_path, lang)) = path.rsplit_once('/') else {
            continue;
        };
        if season_path.is_empty() || lang.is_empty() {
            continue;
        }
        let Ok(url) = base.join(&format!("{season_path}/")) else {
            continue;
        };
        let url = url.to_string();
        if seen.insert(url.clone()) {
            seasons.push(Season {
                name: if name.is_empty() {
                    season_path.to_string()
                } else {
                    name.to_string()
                },
                url,
                default_lang: lang.to_string(),
            });
        }
    }
    Ok(seasons)
}

fn dead_hosts(html: &str) -> HashSet<String> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re(r"lecteursInfos\s*=\s*(\{.*?\})\s*;"));
    let Some(json) = RE.captures(html).map(|c| c[1].to_string()) else {
        return HashSet::new();
    };
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&json) else {
        return HashSet::new();
    };
    map.into_iter()
        .filter(|(_, v)| v.get("mort").and_then(Value::as_bool) == Some(true))
        .map(|(k, _)| k)
        .collect()
}

fn host_in(host: &str, set: &HashSet<String>) -> bool {
    let host = host.trim_start_matches("www.");
    let parts: Vec<&str> = host.split('.').collect();
    (0..parts.len().saturating_sub(1)).any(|i| set.contains(&parts[i..].join(".")))
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

fn parse_season_page(html: &str, page_url: &str, lang: &str) -> EpisodeList {
    static LANGS: LazyLock<Regex> = LazyLock::new(|| re(r"languesDispo\s*=\s*\[([^\]]*)\]"));
    static TOKEN: LazyLock<Regex> = LazyLock::new(|| re(r#"epsJeton\s*=\s*["']([^"']*)["']"#));
    static EPS_META: LazyLock<Regex> = LazyLock::new(|| re(r"epsMeta\s*=\s*(\{.*?\})\s*;"));
    static LIENS_HS: LazyLock<Regex> = LazyLock::new(|| re(r"liensHS\s*=\s*\[([^\]]*)\]"));
    static FINAL_ORDER: LazyLock<Regex> = LazyLock::new(|| re(r"epsOrdreFinal\s*=\s*true"));
    static EPS_ARRAYS: LazyLock<Regex> = LazyLock::new(|| re(r"var\s+eps(\d+)\s*=\s*\[([^\]]*)\]"));

    let mut langs: Vec<String> = LANGS
        .captures(html)
        .map(|c| quoted_items(&c[1]))
        .unwrap_or_default();
    if langs.is_empty() {
        langs.push(lang.to_string());
    }

    let token = TOKEN
        .captures(html)
        .map(|c| c[1].to_string())
        .unwrap_or_default();

    let meta: HashMap<String, Vec<Value>> = EPS_META
        .captures(html)
        .and_then(|c| serde_json::from_str(&c[1]).ok())
        .unwrap_or_default();

    let dead_links: HashSet<String> = LIENS_HS
        .captures(html)
        .map(|c| quoted_items(&c[1]).into_iter().collect())
        .unwrap_or_default();
    let dead = dead_hosts(html);

    let mut arrays: Vec<(u32, Vec<String>)> = EPS_ARRAYS
        .captures_iter(html)
        .filter_map(|c| Some((c[1].parse().ok()?, quoted_items(&c[2]))))
        .collect();
    arrays.sort_by_key(|(n, _)| *n);
    // Without epsOrdreFinal, the site itself swaps the first two players.
    if !FINAL_ORDER.is_match(html) && arrays.len() >= 2 {
        arrays.swap(0, 1);
    }

    let count = arrays.iter().map(|(_, a)| a.len()).max().unwrap_or(0);
    let episodes = (0..count)
        .map(|i| {
            let players = arrays
                .iter()
                .filter_map(|(_, a)| a.get(i))
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .filter_map(|entry| {
                    if let Some(id) = entry.strip_prefix("lien:") {
                        let m = meta.get(id);
                        let host = m
                            .and_then(|m| m.first())
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let deleted = m
                            .and_then(|m| m.get(1))
                            .and_then(Value::as_i64)
                            .is_some_and(|s| s != 0);
                        if deleted || host_in(&host, &dead) {
                            return None;
                        }
                        Some(Player {
                            host,
                            link: Link::Id {
                                id: id.to_string(),
                                token: token.clone(),
                                referer: page_url.to_string(),
                            },
                        })
                    } else {
                        let host = host_of(entry);
                        if host.is_empty() || dead_links.contains(entry) || host_in(&host, &dead) {
                            return None;
                        }
                        Some(Player {
                            host,
                            link: Link::EmbedUrl(entry.to_string()),
                        })
                    }
                })
                .collect();
            Episode {
                number: i + 1,
                players,
            }
        })
        .collect();

    EpisodeList {
        langs,
        lang: lang.to_string(),
        referer: page_url.to_string(),
        episodes,
    }
}
