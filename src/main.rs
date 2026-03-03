use anyhow::Context;
use crate::anime::*;
use colored::Colorize;
use data::get_file;
use directories::ProjectDirs;
use inquire::*;
use spinners::{Spinner, Spinners};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use threadpool::ThreadPool;

mod anime;
mod data;

fn parse_range(input: &str) -> anyhow::Result<(u32, u32)> {
    let mut split = input.split('-');
    let first = split
        .next()
        .context("Format invalide")?
        .parse::<u32>()
        .context("Le premier nombre est invalide")?;
    let second = split
        .next()
        .context("Format invalide (manque le deuxième nombre)")?
        .parse::<u32>()
        .context("Le deuxième nombre est invalide")?;
    Ok((first, second))
}

fn download(mut anime: Media) -> anyhow::Result<()> {
    let download_dir = Path::new(&anime.name);

    if !download_dir.exists() {
        fs::create_dir(download_dir)?;
    }

    let ep_count = anime.episodes.len();

    if ep_count > 25 {
        println!("Plus de 25 épisodes !");
        println!(
            "Sélectionnez les épisodes à télécharger (ex: 0-{})",
            ep_count - 1
        );

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        let (start, end) = parse_range(input.trim())
            .context("Plage d’épisodes invalide")?;

        println!("Téléchargement des épisodes de {} à {}", start, end);

        anime.episodes = anime.episodes[start as usize..=end as usize].to_vec();
    }

    let total = anime.episodes.len();
    let completed = Arc::new(AtomicUsize::new(0));
    let pool = ThreadPool::new(12);
    let m = MultiProgress::new();
    let style = ProgressStyle::with_template(
        "{spinner:.cyan} [{elapsed_precise}] [{bar:40.green/white}] {percent:>3}% {msg}",
    )?
    .progress_chars("=>-");

    for (index, episode) in anime.episodes.into_iter().enumerate() {
        let m = m.clone();
        let style = style.clone();
        let completed = Arc::clone(&completed);
        let download_dir = download_dir.to_path_buf();

        pool.execute(move || {
            let pb = m.add(ProgressBar::new(100));
            pb.set_style(style);
            pb.set_message(format!("Épisode {}", index + 1));

            let mut child = match Command::new("yt-dlp")
                .arg("--newline")
                .arg("--progress")
                .arg(&episode)
                .current_dir(&download_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => child,
                Err(err) => {
                    pb.abandon_with_message(format!("Erreur lancement yt-dlp: {}", err));
                    return;
                }
            };

            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);

                for line in reader.lines().flatten() {
                    if !line.contains("[download]") {
                        continue;
                    }

                    if let Some(percent) = extract_percent(&line) {
                        pb.set_position(percent as u64);
                    }

                    if let Some(speed) = extract_speed(&line) {
                        pb.set_message(format!(
                            "Épisode {} | {}",
                            index + 1,
                            speed.yellow()
                        ));
                    }
                }
            }

            match child.wait() {
                Ok(status) if status.success() => {
                    let done = completed.fetch_add(1, Ordering::SeqCst) + 1;
                    pb.finish_with_message(format!(
                        "Épisode {} terminé ({}/{})",
                        index + 1,
                        done,
                        total
                    ));
                }
                _ => {
                    pb.abandon_with_message(format!("Épisode {} échec", index + 1));
                }
            }
        });
    }

    pool.join();
    Ok(())
}

fn extract_percent(line: &str) -> Option<f32> {
    let percent_pos = line.find('%')?;
    let start = line[..percent_pos].rfind(' ')?;
    line[start..percent_pos].trim().parse().ok()
}

fn extract_speed(line: &str) -> Option<&str> {
    let at = line.find(" at ")? + 4;
    let eta = line.find(" ETA ")?;
    Some(line[at..eta].trim())
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
                if let Err(e) = download(ans3) {
                    eprintln!("Erreur lors du téléchargement: {}", e);
                }
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
