use crate::anime::*;
use data::get_file;
use directories::ProjectDirs;
use inquire::*;
use spinners::{Spinner, Spinners};
use std::path::PathBuf;
use threadpool::ThreadPool;

mod anime;
mod data;

fn download(episodes: Vec<String>, name: &str) {
    match PathBuf::from(name).exists() {
        true => (),
        false => std::fs::create_dir(name).unwrap(),
    }
    std::env::set_current_dir(name).unwrap();
    let pool = ThreadPool::new(12);

    for chunk in episodes.chunks(12) {
        for episode in chunk {
            let episode = episode.clone();
            pool.execute(move || {
                let output = std::process::Command::new("yt-dlp")
                    .arg(&episode)
                    .status()
                    .expect("Failed to execute command");
                if output.success() {
                    println!("Téléchargement de {} terminé", episode);
                } else {
                    println!("Échec du téléchargement de {}", episode);
                }
            });
        }
    }

    pool.join();
}

fn watch(link: &str) {
    let output = std::process::Command::new("mpv")
        .arg(link)
        .output()
        .expect("Failed to execute command");
    println!("{}", String::from_utf8_lossy(&output.stdout));
}

fn main() {
    let file_path = ProjectDirs::from("", "B0SE", "ani-dl")
        .expect("Failed to get project directory")
        .data_dir()
        .join("anime_data.json");

    get_file();

    let mut sp = Spinner::new(Spinners::FingerDance, String::from("Chargement des animes"));

    let file = std::fs::File::open(file_path).unwrap();
    let animes: Animes = serde_json::from_reader(file).unwrap();
    let animes = animes.pretty_names();

    sp.stop_with_symbol("  ");

    let ans = Select::new("Sélectionnez les animes: ", animes.get_name())
        .prompt()
        .unwrap();

    let animes2 = animes.get_seasons_from_str(&ans);

    let mut vf = false;

    for anime in &animes2 {
        if anime.lang == "vf" {
            vf = true;
            break;
        }
    }

    let mut ans2 = "vostfr";

    if vf {
        ans2 = Select::new("VF ou VOSTFR?", vec!["VF", "VOSTFR"])
            .prompt()
            .unwrap();
    } else {
        println!("Pas de version française disponible");
    }

    let animes3: Vec<Anime> = animes2
        .into_iter()
        .filter(|x| x.lang == ans2.to_lowercase())
        .collect();

    let ans3 = Select::new("Sélectionnez la saison: ", animes3)
        .prompt()
        .unwrap();

    let options = vec!["Télécharger", "Regarder"];

    let ans4 = Select::new("Voulez-vous télécharger ou regarder l'anime ?", options).prompt();

    if ans4.unwrap() == "Télécharger" {
        download(ans3.episodes, &ans3.name);
    } else {
        let mut episode_numbers = vec![];
        for i in 1..=ans3.episodes.len() {
            episode_numbers.push(i);
        }
        let ans5 = Select::new("Sélectionnez l'épisode: ", episode_numbers)
            .prompt()
            .unwrap();
        watch(&ans3.episodes[ans5 - 1]);
    }
}
