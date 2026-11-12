// yt-dlp has no extractor for Vidmoly, Uqload or Sendvid: we read their player
// page ourselves and hand yt-dlp/mpv the stream with the Referer they check.

use crate::sources::{Episode, Source, USER_AGENT, host_of};
use anyhow::{Result, bail};
use regex::Regex;
use reqwest::blocking::Client;
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::time::Duration;

const FULL_HD: u32 = 1080;

#[derive(Debug, Clone)]
pub struct Stream {
    pub url: String,
    pub referer: Option<String>,
    pub height: Option<u32>,
}

const YTDLP_HOSTS: &[&str] = &[
    "mail.ru",
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
    "voembed.net",
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

fn priority(host: &str) -> u8 {
    if host_matches(host, SCRAPED_HOSTS) || host_matches(host, YTDLP_HOSTS) {
        0
    } else {
        1
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

fn hls_max_height(master_url: &str, referer: Option<&str>) -> Option<u32> {
    static RESOLUTION: LazyLock<Regex> = LazyLock::new(|| re(r"RESOLUTION=\d+x(\d+)"));
    let mut req = CLIENT.get(master_url);
    if let Some(r) = referer {
        req = req.header("Referer", r);
    }
    let playlist = req.send().ok()?.error_for_status().ok()?.text().ok()?;
    RESOLUTION
        .captures_iter(&playlist)
        .filter_map(|c| c[1].parse().ok())
        .max()
}

fn ytdlp_height(url: &str) -> Result<Option<u32>> {
    let output = Command::new("yt-dlp")
        .args(["--no-warnings", "--skip-download", "--print", "height", url])
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        bail!("yt-dlp ne trouve pas la vidéo");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().parse().ok())
}

pub fn resolve(embed: &str, referer: &str) -> Result<Stream> {
    let host = host_of(embed);

    if host_matches(&host, YTDLP_HOSTS) {
        return Ok(Stream {
            url: embed.to_string(),
            referer: None,
            height: ytdlp_height(embed)?,
        });
    }

    let resp = CLIENT.get(embed).header("Referer", referer).send()?;
    if !resp.status().is_success() {
        bail!("{host} a répondu {}", resp.status());
    }
    let final_url = resp.url().to_string();
    let body = resp.text()?;

    if let Some(url) = find_video_url(&body) {
        let referer = origin(&final_url);
        let height = if url.contains(".m3u8") {
            hls_max_height(&url, referer.as_deref())
        } else {
            None
        };
        return Ok(Stream {
            url,
            referer,
            height,
        });
    }

    if host_matches(&host, SCRAPED_HOSTS) {
        bail!("vidéo introuvable sur {host} (supprimée ?)");
    }
    Ok(Stream {
        url: embed.to_string(),
        referer: Some(referer.to_string()),
        height: None,
    })
}

pub fn best_stream(source: &dyn Source, episode: &Episode) -> Result<Stream> {
    let mut players = source.players(episode)?;
    players.sort_by_key(|p| priority(&p.host));

    let mut best: Option<Stream> = None;
    let mut errors = Vec::new();
    for player in &players {
        let stream = source
            .embed_url(player)
            .and_then(|embed| resolve(&embed, &player.referer));
        match stream {
            Ok(stream) if stream.height.unwrap_or(0) >= FULL_HD => return Ok(stream),
            Ok(stream) if best.as_ref().is_none_or(|b| stream.height > b.height) => {
                best = Some(stream)
            }
            Ok(_) => {}
            Err(e) => errors.push(format!("{}: {e:#}", player.host)),
        }
    }

    match best {
        Some(stream) => Ok(stream),
        None if errors.is_empty() => bail!("aucun lecteur disponible pour cet épisode"),
        None => bail!("aucun lecteur ne fonctionne ({})", errors.join(" ; ")),
    }
}
