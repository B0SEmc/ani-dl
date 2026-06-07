use anyhow::{Context, Result};
use regex::Regex;
use scraper::{Html, Selector};

use super::{AnimeResult, SeasonInfo, Source};

pub struct AnimeSama {
    /// Live base URL, resolved at startup (see `crate::config`).
    base_url: String,
}

impl AnimeSama {
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }

    /// Parse panneauAnime() calls from the anime page JS to extract seasons.
    /// Format: panneauAnime("Saga 1 (East Blue)", "saison1/vostfr");
    fn parse_panneau_calls(html: &str) -> Vec<(String, String, String)> {
        let re = Regex::new(r#"panneauAnime\("([^"]+)",\s*"([^"]+)/([^"]+)"\)"#).unwrap();
        re.captures_iter(html)
            .map(|cap| {
                (
                    cap[1].to_string(), // label
                    cap[2].to_string(), // path (e.g. "saison1")
                    cap[3].to_string(), // lang (e.g. "vostfr")
                )
            })
            .collect()
    }

    /// Known hosts that yt-dlp/mpv can handle, in priority order.
    const SUPPORTED_HOSTS: &[&str] = &["sibnet.ru", "sendvid.com"];

    /// Check if a URL points to a supported host.
    fn is_supported(url: &str) -> bool {
        Self::SUPPORTED_HOSTS.iter().any(|h| url.contains(h))
    }

    /// Parse all epsN arrays from episodes.js, return best URL per episode.
    /// For each episode position, picks the URL from the highest-priority
    /// supported host across all source arrays. This handles cases where
    /// a single source array mixes hosts (e.g. sibnet with vidmoly gaps).
    fn parse_episodes_js(js: &str) -> Vec<String> {
        let re = Regex::new(r#"var\s+eps(\d+)\s*=\s*\[([^\]]+)\]"#).unwrap();
        let url_re = Regex::new(r#"['\"](https?://[^'\"]+)['\"]"#).unwrap();

        let mut all_sources: Vec<Vec<String>> = Vec::new();

        for cap in re.captures_iter(js) {
            let urls: Vec<String> = url_re
                .captures_iter(&cap[2])
                .map(|c| c[1].to_string())
                .collect();
            if !urls.is_empty() {
                all_sources.push(urls);
            }
        }

        if all_sources.is_empty() {
            return Vec::new();
        }

        // Find the max episode count across all sources
        let max_eps = all_sources.iter().map(|s| s.len()).max().unwrap_or(0);

        // For each episode position, pick the best URL by host priority
        let mut best: Vec<String> = Vec::with_capacity(max_eps);
        for i in 0..max_eps {
            let mut picked = None;
            // Try supported hosts in priority order
            for host in Self::SUPPORTED_HOSTS {
                for source in &all_sources {
                    if let Some(url) = source.get(i) {
                        if url.contains(host) {
                            picked = Some(url.clone());
                            break;
                        }
                    }
                }
                if picked.is_some() {
                    break;
                }
            }
            // Fallback: first available URL at this position, prefer supported
            let url = picked.unwrap_or_else(|| {
                all_sources
                    .iter()
                    .filter_map(|s| s.get(i))
                    .find(|u| Self::is_supported(u))
                    .or_else(|| all_sources.iter().filter_map(|s| s.get(i)).next())
                    .cloned()
                    .unwrap_or_default()
            });
            if !url.is_empty() {
                best.push(url);
            }
        }

        best
    }
}

impl Source for AnimeSama {
    fn search(&self, query: &str) -> Result<Vec<AnimeResult>> {
        let url = format!("{}/template-php/defaut/fetch.php", self.base_url);
        let body = reqwest::blocking::Client::new()
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(format!("query={}", query))
            .send()
            .context("Impossible de contacter anime-sama.to")?
            .text()?;

        let document = Html::parse_fragment(&body);
        let link_selector = Selector::parse("a.asn-search-result").unwrap();
        let title_selector = Selector::parse("h3.asn-search-result-title").unwrap();

        let mut results = Vec::new();

        for card in document.select(&link_selector) {
            let href = match card.value().attr("href") {
                Some(h) => h.to_string(),
                None => continue,
            };

            let name = card
                .select(&title_selector)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            if name.is_empty() {
                continue;
            }

            let slug = href
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("")
                .to_string();

            if slug.is_empty() {
                continue;
            }

            let full_url = if href.starts_with("http") {
                href
            } else {
                format!("{}{}", self.base_url, href)
            };

            // Ensure URL ends with /
            let full_url = if full_url.ends_with('/') {
                full_url
            } else {
                format!("{}/", full_url)
            };

            results.push(AnimeResult {
                name,
                slug,
                url: full_url,
            });
        }

        Ok(results)
    }

    fn get_seasons(&self, anime: &AnimeResult) -> Result<Vec<SeasonInfo>> {
        let body = reqwest::blocking::get(&anime.url)
            .context("Impossible de charger la fiche anime")?
            .text()?;

        let entries = Self::parse_panneau_calls(&body);
        let client = reqwest::blocking::Client::new();

        // Collect unique season paths (preserving order)
        let mut unique_paths: Vec<String> = Vec::new();
        for (_label, path, _lang) in &entries {
            if !unique_paths.contains(path) {
                unique_paths.push(path.clone());
            }
        }

        // For each path, collect known langs + probe for additional ones
        let all_langs = ["vostfr", "vf", "va"];
        let mut seasons: Vec<SeasonInfo> = Vec::new();

        for (i, path) in unique_paths.iter().enumerate() {
            // Start with langs found in panneauAnime
            let mut langs: Vec<String> = entries
                .iter()
                .filter(|(_, p, _)| p == path)
                .map(|(_, _, l)| l.clone())
                .collect();

            // Probe for other langs not already found
            for lang in &all_langs {
                if langs.contains(&lang.to_string()) {
                    continue;
                }
                let probe_url = format!(
                    "{}/catalogue/{}/{}/{}/episodes.js",
                    self.base_url, anime.slug, path, lang
                );
                if let Ok(resp) = client.head(&probe_url).send() {
                    if resp.status().is_success() {
                        langs.push(lang.to_string());
                    }
                }
            }

            if !langs.is_empty() {
                seasons.push(SeasonInfo {
                    number: (i + 1) as i8,
                    langs,
                });
            }
        }

        Ok(seasons)
    }

    fn get_episodes(
        &self,
        anime: &AnimeResult,
        season: i8,
        lang: &str,
    ) -> Result<Vec<String>> {
        // Rebuild the path from the anime page to find the right season
        let body = reqwest::blocking::get(&anime.url)
            .context("Impossible de charger la fiche anime")?
            .text()?;

        let entries = Self::parse_panneau_calls(&body);

        // Find the matching season path
        let mut season_paths: std::collections::BTreeMap<String, i8> =
            std::collections::BTreeMap::new();
        let mut counter: i8 = 1;
        for (_label, path, _lang) in &entries {
            season_paths.entry(path.clone()).or_insert_with(|| {
                let n = counter;
                counter += 1;
                n
            });
        }

        let season_path = season_paths
            .iter()
            .find(|&(_, &num)| num == season)
            .map(|(path, _)| path.clone())
            .context("Saison introuvable")?;

        let episodes_url = format!(
            "{}/catalogue/{}/{}/{}/episodes.js",
            self.base_url, anime.slug, season_path, lang
        );

        let js = reqwest::blocking::get(&episodes_url)
            .context("Impossible de charger les épisodes")?
            .text()?;

        let episodes = Self::parse_episodes_js(&js);

        if episodes.is_empty() {
            anyhow::bail!("Aucun épisode trouvé pour cette saison/langue");
        }

        Ok(episodes)
    }
}
