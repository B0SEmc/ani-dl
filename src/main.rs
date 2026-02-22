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

fn to_title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn download(mut anime: Media) -> anyhow::Result<()> {
    // Structure : [nom_anime]/S{saison}/ (nom capitalisé)
    let anime_name_title = to_title_case(&anime.name);
    let season_dir = Path::new(&anime_name_title).join(format!("S{}", anime.season));

    if !season_dir.exists() {
        fs::create_dir_all(&season_dir)?;
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

    let pool = ThreadPool::new(12);
    let m = MultiProgress::new();
    let style = ProgressStyle::with_template(
        "{spinner:.cyan} [{elapsed_precise}] [{bar:40.green/white}] {percent:>3}% {msg}",
    )?
    .progress_chars("=>-");

    let anime_name = anime_name_title.clone();
    let anime_season = anime.season;

    for (index, episode) in anime.episodes.into_iter().enumerate() {
        let m = m.clone();
        let style = style.clone();
        let season_dir = season_dir.clone();
        let anime_name = anime_name.clone();

        pool.execute(move || {
            // Nom de fichier : "[anime] E01S1" (yt-dlp ajoute l'extension automatiquement)
            let output_template = format!(
                "{}/{} E{:02}S{}.%(ext)s",
                season_dir.display(),
                anime_name,
                index + 1,
                anime_season
            );
            let pb = m.add(ProgressBar::new(100));
            pb.set_style(style);
            pb.set_message(format!("| Épisode {:02}", index + 1));

            let mut child = match Command::new("yt-dlp")
                .arg("--newline")
                .arg("--progress")
                .arg("-o").arg(&output_template)
                .arg(&episode)
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
                            "| Épisode {:02} | {}",
                            index + 1,
                            speed.yellow()
                        ));
                    }
                }
            }

            match child.wait() {
                Ok(status) if status.success() => {
                    pb.finish_with_message(format!(
                        "| Épisode {:02} | {}",
                        index + 1,
                        "terminé".cyan()
                    ));
                }
                _ => {
                    pb.abandon_with_message(format!("| Épisode {:02} échec", index + 1));
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
