use crate::anime::*;
use data::get_file;
use directories::ProjectDirs;
use inquire::*;
use spinners::{Spinner, Spinners};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use threadpool::ThreadPool;

mod anime;
mod data;

fn parse_range(input: &str) -> Result<(u32, u32), String> {
    let mut split = input.split('-');
    let first = match split.next().unwrap().parse::<u32>() {
        Ok(x) => x,
        Err(_) => return Err("Erreur ! Veuillez respecter le format.".to_string()),
    };
    let second = match split.next().unwrap().parse::<u32>() {
        Ok(x) => x,
        Err(_) => return Err("Erreur ! Veuillez respecter le format".to_string()),
    };
    Ok((first, second))
}

fn download(mut anime: Media) {
    match PathBuf::from(&anime.name).exists() {
        true => (),
        false => std::fs::create_dir(&anime.name).unwrap(),
    }
    std::env::set_current_dir(&anime.name).unwrap();
    let pool = ThreadPool::new(12);

    let ep_count = anime.episodes.len();

    if ep_count > 25 {
        println!("Plus de 25 épisodes!");
        println!(
            "Sélectionnez les épisodes à télécharger (ex: 0-{})",
            ep_count
        );
        let mut input = String::default();
        std::io::stdin().read_line(&mut input).unwrap();
        let (start, end) = parse_range(input.trim()).unwrap();
        anime.episodes = anime.episodes[start as usize..end as usize].to_vec();
        println!("Downloading episodes {} to {}", start, end);
    }

    let count = Arc::new(Mutex::new(0));
    let total = anime.episodes.len();

    for chunk in anime.episodes.chunks(12) {
        for episode in chunk {
            let episode = episode.clone();
            let count = Arc::clone(&count);
            pool.execute(move || {
                let output = std::process::Command::new("yt-dlp")
                    .arg(&episode)
                    .status()
                    .expect("Failed to execute command");
                if output.success() {
                    let mut num = count.lock().unwrap();
                    *num += 1;
                    println!("\nTéléchargement {}/{} terminé", *num, total);
                } else {
                    eprintln!("Échec du téléchargement de {}", episode);
                }
            });
        }
    }
    pool.join();
}

fn watch(link: &str) {
    std::process::Command::new("mpv")
        .arg(link)
        .output()
        .expect("Failed to execute command");
}

fn main() {
    let file_path = ProjectDirs::from("", "B0SE", "ani-dl")
        .expect("Failed to get project directory")
        .data_dir()
        .join("anime_data.json");

    get_file(false);

    let mut sp = Spinner::new(Spinners::Moon, String::from("Chargement des animes"));

    let file = std::fs::File::open(file_path).unwrap();
    let animes: Medias = match serde_json::from_reader(&file) {
        Ok(v) => v,
        Err(_e) => {
            get_file(true);
            eprintln!("\nNouvelle base de données téléchargée, veuillez relancer le programme. Si le problème persiste, veuillez ouvrir une issue sur GitHub.");
            std::process::exit(0);
        }
    };

    sp.stop_with_symbol(" ✔️ ");

    let ans = match Select::new("Sélectionnez les animes: ", animes.get_name()).prompt() {
        Ok(v) => v,
        Err(InquireError::OperationInterrupted) => std::process::exit(0),
        Err(InquireError::OperationCanceled) => std::process::exit(0),
        Err(e) => panic!("{}", e),
    };

    let animes2 = animes.get_seasons_from_str(&ans);

    let vf = animes2.iter().any(|x| x.lang == "vf");

    loop {
        loop {
            let mut ans2 = "vostfr";

            if vf {
                ans2 = match Select::new("VF ou VOSTFR?", vec!["VF", "VOSTFR"]).prompt() {
                    Ok(v) => v,
                    Err(InquireError::OperationInterrupted) => std::process::exit(0),
                    Err(InquireError::OperationCanceled) => std::process::exit(0),
                    Err(e) => panic!("{}", e),
                }
            } else {
                println!("Pas de VF disponible");
            }

            let mut animes3: Vec<Media> = animes2
                .clone() // only keep the selected language
                .into_iter()
                .filter(|x| x.lang == ans2.to_lowercase())
                .collect();

            animes3.sort_by(|a, b| a.season.partial_cmp(&b.season).unwrap());

            let ans3 = match Select::new("Sélectionnez la saison: ", animes3).prompt() {
                Ok(v) => v,
                Err(InquireError::OperationInterrupted) => std::process::exit(0),
                Err(InquireError::OperationCanceled) => break,
                Err(e) => panic!("{}", e),
            };

            let options = vec!["Télécharger", "Regarder"];

            let ans4 = match Select::new("Voulez-vous télécharger ou regarder l'anime ?", options)
                .prompt()
            {
                Ok(v) => v,
                Err(InquireError::OperationInterrupted) => std::process::exit(0),
                Err(InquireError::OperationCanceled) => break,
                Err(e) => panic!("{}", e),
            };

            if ans4 == "Télécharger" {
                download(ans3);
            } else {
                let mut episode_numbers = vec![];
                for i in 1..=ans3.episodes.len() {
                    episode_numbers.push(i);
                }
                loop {
                    let ans5 =
                        match Select::new("Sélectionnez l'épisode: ", episode_numbers.clone())
                            .prompt()
                        {
                            Ok(v) => v,
                            Err(InquireError::OperationInterrupted) => std::process::exit(0),
                            Err(InquireError::OperationCanceled) => break,
                            Err(e) => panic!("{}", e),
                        };

                    watch(&ans3.episodes[ans5 - 1]);
                }
            }
        }
    }
}
