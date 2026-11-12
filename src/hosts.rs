// yt-dlp has no extractor for Vidmoly, Uqload or Sendvid: we read their player
// page ourselves and hand yt-dlp/mpv the stream with the Referer they check.

use crate::sources::anime_sama::{USER_AGENT, host_of};
use anyhow::{Result, bail};
use regex::Regex;
use reqwest::blocking::Client;
use std::sync::LazyLock;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Stream {
    pub url: String,
    pub referer: Option<String>,
}

const YTDLP_HOSTS: &[&str] = &[
    "sibnet.ru",
    "vk.com",
    "vkvideo.ru",
    "ok.ru",
    "dailymotion.com",
    "youtube.com",
    "player.vimeo.com",
    "drive.google.com",
];

const SCRAPED_HOSTS: &[&str] = &[
    "vidmoly.to",
    "vidmoly.net",
    "vidmoly.biz",
    "vidmoly.me",
    "ansembed.net",
    "sendvid.com",
    "uqload.vc",
    "uqload.to",
    "uqload.io",
    "uqload.com",
];

fn host_matches(host: &str, list: &[&str]) -> bool {
    list.iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

pub fn priority(host: &str) -> u8 {
    if host_matches(host, SCRAPED_HOSTS) {
        0
    } else if host_matches(host, YTDLP_HOSTS) {
        1
    } else {
        2
    }
}

static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(20))
        .build()
        .expect("http client")
});

fn re(p: &str) -> Regex {
    Regex::new(p).expect("valid regex")
}

static VIDEO_URL_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        re(r#"file\s*:\s*["']([^"']+\.m3u8[^"']*)["']"#),
        re(r#"file\s*:\s*["']([^"']+\.mp4[^"']*)["']"#),
        re(r#"sources\s*:\s*\[\s*["']([^"']+)["']"#),
        re(r#"<source[^>]+src=["']([^"']+)["']"#),
        re(r#"<meta[^>]+property=["']og:video(?::secure_url)?["'][^>]+content=["']([^"']+)["']"#),
    ]
});

fn find_video_url(page: &str) -> Option<String> {
    VIDEO_URL_PATTERNS.iter().find_map(|p| {
        p.captures(page)
            .map(|c| c[1].replace("\\/", "/"))
            .filter(|u| u.starts_with("http") || u.starts_with("//"))
            .map(|u| {
                if u.starts_with("//") {
                    format!("https:{u}")
                } else {
                    u
                }
            })
    })
}

fn origin(url: &str) -> Option<String> {
    reqwest::Url::parse(url)
        .ok()
        .map(|u| format!("{}/", u.origin().ascii_serialization()))
}

pub fn resolve(embed: &str, referer: &str) -> Result<Stream> {
    let host = host_of(embed);

    if host_matches(&host, YTDLP_HOSTS) {
        return Ok(Stream {
            url: embed.to_string(),
            referer: None,
        });
    }

    let resp = CLIENT.get(embed).header("Referer", referer).send()?;
    if !resp.status().is_success() {
        bail!("{host} a répondu {}", resp.status());
    }
    let final_url = resp.url().to_string();
    let body = resp.text()?;

    if let Some(url) = find_video_url(&body) {
        return Ok(Stream {
            url,
            referer: origin(&final_url),
        });
    }

    if host_matches(&host, SCRAPED_HOSTS) {
        bail!("vidéo introuvable sur {host} (supprimée ?)");
    }
    Ok(Stream {
        url: embed.to_string(),
        referer: Some(referer.to_string()),
    })
}
