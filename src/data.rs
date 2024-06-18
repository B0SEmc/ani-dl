use std::io;

pub fn get_file() {
    let mut resp =
        reqwest::blocking::get("https://github.com/S3nda/ani-data/raw/main/anime_data.json")
            .unwrap();
    let mut out = std::fs::File::create("anime_data.json").unwrap();
    io::copy(&mut resp, &mut out).unwrap();
}
