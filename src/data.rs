use chrono::DateTime;
use directories::ProjectDirs;
use std::fs::metadata;
use std::io;
use std::path::Path;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

const ANIME_DATA_URL: &str = "https://github.com/S3nda/ani-data/raw/main/anime_data.json";
const LATEST_COMMIT_URL: &str =
    "https://raw.githubusercontent.com/S3nda/ani-data/main/last_commit_time.txt";

fn get_last_commit_time() -> SystemTime {
    let file = reqwest::blocking::get(LATEST_COMMIT_URL)
        .unwrap()
        .text()
        .unwrap();
    let date_time = DateTime::parse_from_rfc3339(file.trim()).unwrap();
    UNIX_EPOCH + Duration::from_secs(date_time.timestamp() as u64)
}

fn get_file_modification_time(file_path: &Path) -> SystemTime {
    let meta = metadata(file_path).unwrap();
    meta.modified().unwrap()
}

fn download_file(file_path: &Path) {
    let mut resp = reqwest::blocking::get(ANIME_DATA_URL)
        .expect("Echec lors du téléchargement du fichier, vérifiez votre connexion internet");
    if resp.content_length().unwrap() < 500_000 {
        println!("Ignoring new file, file < 500KB, scrapper probably messed up again (complain to S3nda).");
        return;
    }
    let mut out = std::fs::File::create(file_path).expect("Failed to create file");
    io::copy(&mut resp, &mut out).expect("Failed to write to file");
}

pub fn get_file(overwrite: bool) {
    let dir = ProjectDirs::from("", "B0SE", "ani-dl").expect("Failed to get project directory");
    let data_dir = dir.data_dir();
    let file_path = data_dir.join("anime_data.json");

    let last_commit_time = get_last_commit_time();

    if !data_dir.exists() {
        std::fs::create_dir_all(data_dir).expect("Failed to create data directory");
    }

    if overwrite || !file_path.exists() {
        download_file(&file_path);
    }

    if last_commit_time > get_file_modification_time(&file_path) {
        println!("Mise à jour des données...");
        download_file(&file_path);
    }
}
