//! Resilient base-URL resolution for anime-sama.
//!
//! anime-sama rotates its domain regularly (`.to`, `.xyz`, `.org`, `.fr`, ...),
//! so hardcoding a single host breaks the tool every time the domain moves.
//! Instead we resolve a live base URL at startup from, in order of precedence:
//!
//!   1. `ANIME_SAMA_BASE_URL` — explicit override, used as-is (no probing).
//!   2. `ANIME_SAMA_MIRRORS` (comma/newline separated) or the user config file
//!      `~/.config/ani-dl/mirrors.txt` — probed, first reachable wins.
//!   3. The built-in default mirror list — probed, first reachable wins.
//!
//! The config file is auto-created (self-documented) on first run, so users can
//! react to a domain change by editing a text file instead of recompiling.

use anyhow::{anyhow, Result};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// Built-in fallback mirrors, tried in order when no config is provided.
const DEFAULT_MIRRORS: &[&str] = &[
    "https://anime-sama.to",
    "https://anime-sama.xyz",
    "https://anime-sama.org",
    "https://anime-sama.fr",
];

const ENV_BASE_URL: &str = "ANIME_SAMA_BASE_URL";
const ENV_MIRRORS: &str = "ANIME_SAMA_MIRRORS";

/// Per-mirror health-check timeout.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Resolve a live anime-sama base URL (without trailing slash).
pub fn resolve_base_url() -> Result<String> {
    // 1. Explicit override wins — trust the user, skip the network probe.
    if let Ok(url) = env::var(ENV_BASE_URL) {
        let url = url.trim();
        if !url.is_empty() {
            return Ok(normalize(url));
        }
    }

    // 2/3. Build the ordered candidate list and probe it.
    let candidates = candidate_mirrors();

    let client = reqwest::blocking::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .user_agent("ani-dl")
        .build()?;

    for mirror in &candidates {
        if is_alive(&client, mirror) {
            return Ok(mirror.clone());
        }
    }

    Err(anyhow!(
        "Aucun miroir anime-sama joignable.\n\
         Miroirs testés : {}\n\
         → Édite {} pour ajouter/réordonner un domaine à jour,\n\
         → ou force-le : ANIME_SAMA_BASE_URL=https://nouveau-domaine ani-dl",
        candidates.join(", "),
        config_path().display()
    ))
}

/// Build the ordered candidate list from env, config file, or defaults.
fn candidate_mirrors() -> Vec<String> {
    // Env list takes priority over the file.
    if let Ok(raw) = env::var(ENV_MIRRORS) {
        let list = parse_mirror_list(&raw);
        if !list.is_empty() {
            return list;
        }
    }

    // Then the user config file (auto-created on first run).
    if let Some(list) = read_config_mirrors()
        && !list.is_empty()
    {
        return list;
    }

    // Finally the built-in defaults.
    DEFAULT_MIRRORS.iter().map(|m| m.to_string()).collect()
}

/// Parse mirrors separated by newlines or commas, ignoring blanks and
/// `#` comments.
fn parse_mirror_list(raw: &str) -> Vec<String> {
    raw.split(['\n', ','])
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(normalize)
        .collect()
}

/// Read mirrors from the config file, creating a documented default on first
/// run. Returns `None` if the file can't be read or created.
fn read_config_mirrors() -> Option<Vec<String>> {
    let path = config_path();
    if !path.exists() {
        // Best-effort: write a self-documenting default file for discovery.
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, default_config_contents());
    }
    let contents = fs::read_to_string(&path).ok()?;
    Some(parse_mirror_list(&contents))
}

/// Path to the user mirror config: `$XDG_CONFIG_HOME/ani-dl/mirrors.txt`
/// (falls back to `$HOME/.config/ani-dl/mirrors.txt`).
fn config_path() -> PathBuf {
    let base = env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".config")
        });
    base.join("ani-dl").join("mirrors.txt")
}

/// Self-documenting default contents for `mirrors.txt`.
fn default_config_contents() -> String {
    let mut s = String::from(
        "# ani-dl — miroirs anime-sama\n\
         #\n\
         # Un domaine par ligne, testé de haut en bas : le premier qui répond est utilisé.\n\
         # anime-sama change régulièrement de domaine — ajoute ou réordonne les lignes ici\n\
         # sans avoir à recompiler. Lignes vides et lignes commençant par # ignorées.\n\
         #\n\
         # Override ponctuel : ANIME_SAMA_BASE_URL=https://mon-domaine ani-dl\n\
         \n",
    );
    for m in DEFAULT_MIRRORS {
        s.push_str(m);
        s.push('\n');
    }
    s
}

/// Health check: GET the root (following redirects); healthy if status < 400.
fn is_alive(client: &reqwest::blocking::Client, url: &str) -> bool {
    match client.get(url).send() {
        Ok(resp) => resp.status().as_u16() < 400,
        Err(_) => false,
    }
}

/// Normalize a mirror: ensure an `https://` scheme, drop any trailing slash.
fn normalize(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{}", url)
    }
}
