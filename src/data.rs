use directories::ProjectDirs;
use std::io;
use std::path::Path;

const ANIME_DATA_URL: &str = "https://github.com/S3nda/ani-data/raw/main/anime_data.json";

fn download_file(file_path: &Path) {
    let mut resp = reqwest::blocking::get(ANIME_DATA_URL).expect("Failed to download file");
    let mut out = std::fs::File::create(file_path).expect("Failed to create file");
    io::copy(&mut resp, &mut out).expect("Failed to write to file");
}

pub fn get_file() {
    let dir = ProjectDirs::from("", "B0SE", "ani-dl").expect("Failed to get project directory");
    let data_dir = dir.data_dir();

    if !data_dir.exists() {
        std::fs::create_dir_all(data_dir).expect("Failed to create data directory");
    }

    let file_path = data_dir.join("anime_data.json");
    if !file_path.exists() {
        download_file(&file_path);
    }
}
