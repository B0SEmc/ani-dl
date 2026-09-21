# Scraper Architecture

ani-dl uses a modular scraper system to fetch anime data directly from catalog sites, instead of relying on a static JSON file.

## How it works

The scraper follows a simple 3-step pipeline:

```
1. Search     user query  -->  list of matching animes
2. Seasons    anime page  -->  available seasons + languages
3. Episodes   season page -->  playable video URLs
```

Each step returns structured data that the CLI uses to build its interactive menus.

## The `Source` trait

Every catalog site implements the `Source` trait defined in `src/sources/mod.rs`:

```rust
pub trait Source {
    fn search(&self, query: &str) -> Result<Vec<AnimeResult>>;
    fn get_seasons(&self, anime: &AnimeResult) -> Result<Vec<SeasonInfo>>;
    fn get_episodes(&self, anime: &AnimeResult, season: i8, lang: &str) -> Result<Vec<String>>;
}
```

- `search` takes a text query, returns anime names + URLs
- `get_seasons` takes an anime, returns numbered seasons with their available languages
- `get_episodes` takes an anime + season + language, returns a list of embed URLs that yt-dlp can resolve

## Shared types

```rust
AnimeResult { name, slug, url }   // A search result
SeasonInfo  { number, langs }     // A season with its available languages
```

## Current source: anime-sama.to

Implementation: `src/sources/anime_sama.rs`

### Pipeline details

| Step | Method | How |
|------|--------|-----|
| Search | `POST /template-php/defaut/fetch.php` | Form body `query=...`, returns HTML fragments with `a.asn-search-result` links |
| Seasons | `GET /catalogue/{slug}/` | Parse `panneauAnime("label", "path/lang")` JS calls + HEAD probe for additional languages (VF, VA) |
| Episodes | `GET /catalogue/{slug}/{path}/{lang}/episodes.js` | Parse `var epsN = [...]` arrays, pick best supported host |

### Multi-source video hosts

The `episodes.js` file contains multiple arrays (`eps1`, `eps2`, `eps3`...), each pointing to a different video host. The scraper picks the best one based on yt-dlp compatibility:

| Priority | Host | Status |
|----------|------|--------|
| 1 | sibnet.ru | Reliable, well supported by yt-dlp |
| 2 | vidmoly.to | Good fallback |
| 3 | sendvid.com | Works |
| - | embed4me.com | Not supported by yt-dlp, skipped |
| - | dingtezuni.com | Not supported by yt-dlp, skipped |

To update this list, edit `SUPPORTED_HOSTS` in `anime_sama.rs`.

### Language detection

The anime pages (`panneauAnime()` calls) only list VOSTFR paths. VF and VA versions exist but aren't listed in the HTML. The scraper probes for them with HEAD requests on `episodes.js` for each season path.

## Domain resolution & mirrors

anime-sama rotates its domain regularly (`.to`, `.xyz`, `.org`, `.fr`, ...), so
the base URL is **not** hardcoded. It is resolved at startup by `src/config.rs`,
in order of precedence:

1. **`ANIME_SAMA_BASE_URL`** — explicit override, used as-is, no probing.
   Handy for testing/CI or forcing a brand-new domain on the spot.
2. **`ANIME_SAMA_MIRRORS`** (comma/newline separated) **or** the user config
   file `~/.config/ani-dl/mirrors.txt` — every entry is probed (`GET`, follows
   redirects, healthy if status `< 400`) and the first reachable one wins.
3. **Built-in default list** — used when no env var / config file is present.

`mirrors.txt` is auto-created (self-documented) on first run, so when a domain
moves you just edit a text file — no recompile needed.

```
# ~/.config/ani-dl/mirrors.txt — one domain per line, first reachable wins
https://anime-sama.to
https://anime-sama.xyz
```

`AnimeSama::new(base_url)` receives the resolved URL; all requests are built
from `self.base_url`.

## Adding a new source

To add support for a new catalog site:

1. Create `src/sources/your_site.rs`
2. Implement the `Source` trait
3. Register the module in `src/sources/mod.rs`: `pub mod your_site;`
4. Wire it up in `main.rs` (or add a CLI flag to select the source)

### Example skeleton

```rust
use super::{AnimeResult, SeasonInfo, Source};

pub struct YourSite;

impl YourSite {
    pub fn new() -> Self { Self }
}

impl Source for YourSite {
    fn search(&self, query: &str) -> anyhow::Result<Vec<AnimeResult>> {
        // 1. Hit the site's search endpoint
        // 2. Parse results into AnimeResult structs
        todo!()
    }

    fn get_seasons(&self, anime: &AnimeResult) -> anyhow::Result<Vec<SeasonInfo>> {
        // 1. Load the anime page
        // 2. Extract season list + available languages
        todo!()
    }

    fn get_episodes(&self, anime: &AnimeResult, season: i8, lang: &str) -> anyhow::Result<Vec<String>> {
        // 1. Load the episodes page/API for this season+lang
        // 2. Return embed URLs that yt-dlp can handle
        todo!()
    }
}
```

The key constraint: `get_episodes` must return URLs that yt-dlp can resolve into playable streams. Test with `yt-dlp --simulate <url>` before integrating.
