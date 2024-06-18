use crate::anime::*;
use inquire::*;
use spinners::{Spinner, Spinners};
use std::thread;

mod anime;

fn download(episodes: Vec<String>) {
    if episodes.len() > 50 {
        println!("Trop d'épisodes à télécharger, ouvrez une issue sur github");
        return;
    }
    let mut handles = vec![];

    for episode in episodes {
        let handle = thread::spawn(move || {
            println!("Downloading: {}", episode);
            let output = std::process::Command::new("yt-dlp")
                .arg(episode)
                .output()
                .expect("Failed to execute command");
            println!("{}", String::from_utf8_lossy(&output.stdout));
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

fn watch(link: &str) {
    let output = std::process::Command::new("mpv")
        .arg(link)
        .output()
        .expect("Failed to execute command");
    println!("{}", String::from_utf8_lossy(&output.stdout));
}

fn main() {
    let mut sp = Spinner::new(Spinners::FingerDance, String::from("Chargement des animes"));

    let file = std::fs::File::open("anime_data.json").unwrap();
    let animes: Animes = serde_json::from_reader(file).unwrap();
    let animes = animes.pretty_names();

    sp.stop_with_symbol("  ");

    let ans = Select::new("Sélectionnez les animes: ", animes.get_name())
        .prompt()
        .unwrap();

    let animes2 = animes.get_seasons_from_str(&ans);

    let ans2 = Select::new("VF ou VOSTFR?", vec!["VF", "VOSTFR"])
        .prompt()
        .unwrap();

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
        download(ans3.episodes);
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
