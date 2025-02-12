## Table of Contents

- [Install](#install)
  - [From Repos](#installation)
  - [From Source](#build)
- [Dependencies](#dependencies)
- [Disclaimer](./disclaimer.md)
- [Thanks](#thanks)

## Demo

[![asciicast](https://github.com/B0SEmc/ani-dl/raw/master/demo.svg)](https://asciinema.org/a/tk9KzxVeL42SZaKQ32i3oQQ58)

## Dependencies

[yt-dlp](https://github.com/yt-dlp/yt-dlp)
[mpv](https://mpv.io/)

## Installation

<details>
  <summary>Arch Linux (AUR)</summary>
  
  ```bash
  yay -S ani-dl
  ```
</details>
<details>
  <summary>Scoop (Windows)</summary>
  
  ```bash
  scoop bucket add extras
  scoop bucket add sendus https://github.com/S3nda/Sendus
  scoop install ani-dl
  ```
</details>
<details>
  <summary>Cargo</summary>
  
  ```bash
  cargo install ani-dl
  ```
</details>

## Build
```bash
git clone https://github.com/B0SEmc/ani-dl.git
cd ani-dl
cargo build --release
```
## Thanks

- [ani-cli](https://github.com/pystardust/ani-cli) for the inspiration and their disclaimer
- [@S3nda](https://github.com/S3nda) for making the original scraper
