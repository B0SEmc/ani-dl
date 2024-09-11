use directories::ProjectDirs;
use std::io;
use std::path::Path;

const ANIME_DATA_URL: &str = "https://github.com/S3nda/ani-data/raw/main/anime_data.json";
const LATEST_VERSION: &str = "https://raw.githubusercontent.com/S3nda/ani-data/main/version.txt";

fn get_last_version() -> u32 {
    let file = reqwest::blocking::get(LATEST_VERSION)
        .unwrap()
        .text()
        .unwrap();
    file.trim().parse().unwrap()
}

fn get_local_version(file_path: &Path) -> u32 {
    if !file_path.exists() {
        let version = 1;
        std::fs::write(file_path, version.to_string()).expect("Failed to write file");
    }
    let file = std::fs::read_to_string(file_path).expect("Failed to read file");
    file.trim().parse().unwrap()
}

fn set_local_version(file_path: &Path, version: u32) {
    std::fs::write(file_path, version.to_string()).expect("Failed to write file");
}

fn download_file(file_path: &Path) {
    let mut resp = reqwest::blocking::get(ANIME_DATA_URL)
        .expect("Echec lors du téléchargement du fichier, vérifiez votre connexion internet");
    if resp.content_length().unwrap() < 500_000 && file_path.exists() {
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
    let ver_file_path = data_dir.join("version.txt");

    let last_version = get_last_version();

    if !data_dir.exists() {
        std::fs::create_dir_all(data_dir).expect("Failed to create data directory");
    }

    if overwrite || !file_path.exists() {
        download_file(&file_path);
    }

    if last_version > get_local_version(&ver_file_path) {
        println!("Mise à jour des données...");
        set_local_version(&ver_file_path, last_version);
        download_file(&file_path);
    }
}
