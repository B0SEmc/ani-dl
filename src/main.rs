use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use inquire::*;
use spinners::{Spinner, Spinners};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};
use threadpool::ThreadPool;

mod sources;

use sources::anime_sama::AnimeSama;
use sources::Source;

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

fn download(anime_name: &str, season: i8, episodes: &[String], selected_indices: Vec<usize>) -> anyhow::Result<()> {
    let anime_name_title = to_title_case(anime_name);
    let season_dir = Path::new(&anime_name_title).join(format!("S{}", season));

    if !season_dir.exists() {
        fs::create_dir_all(&season_dir)?;
    }

    let pool = ThreadPool::new(12);
    let m = MultiProgress::new();
    let style = ProgressStyle::with_template(
        "{spinner:.blue} [{elapsed_precise}] [{bar:40.green/white}] {percent:>3}% {msg}",
    )?
    .progress_chars("=>-");

    let anime_name = anime_name_title.clone();

    for &index in &selected_indices {
        let episode_url = episodes[index].clone();
        let m = m.clone();
        let style = style.clone();
        let season_dir = season_dir.clone();
        let anime_name = anime_name.clone();
        let episode_num = index + 1;

        pool.execute(move || {
            let output_template = format!(
                "{}/{} S{}E{:02}.%(ext)s",
                season_dir.display(),
                anime_name,
                season,
                episode_num
            );
            let pb = m.add(ProgressBar::new(100));
            pb.set_style(style);
            pb.set_message(format!("| Épisode {:02}", episode_num));

            let mut child = match Command::new("yt-dlp")
                .arg("--newline")
                .arg("--progress")
                .arg("-o")
                .arg(&output_template)
                .arg(&episode_url)
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

                for line in reader.lines().map_while(Result::ok) {
                    if !line.contains("[download]") {
                        continue;
                    }

                    if let Some(percent) = extract_percent(&line) {
                        pb.set_position(percent as u64);
                    }

                    if let Some(speed) = extract_speed(&line) {
                        pb.set_message(format!(
                            "| Épisode {:02} | {}",
                            episode_num,
                            speed.yellow()
                        ));
                    }
                }
            }

            match child.wait() {
                Ok(status) if status.success() => {
                    pb.finish_with_message(format!(
                        "| Épisode {:02} | {}",
                        episode_num,
                        "terminé".cyan()
                    ));
                }
                _ => {
                    pb.abandon_with_message(format!(
                        "| Épisode {:02} | {}",
                        episode_num,
                        "échec".red()
                    ));
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
    // Use yt-dlp to resolve the embed URL, then pipe to mpv
    let status = std::process::Command::new("mpv")
        .arg(format!("ytdl://{}", link))
        .status();

    match status {
        Ok(s) if !s.success() => {
            eprintln!("{}", "mpv a échoué, tentative avec yt-dlp...".yellow());
            let _ = std::process::Command::new("yt-dlp")
                .arg("--quiet")
                .arg("-o")
                .arg("-")
                .arg(link)
                .stdout(std::process::Stdio::piped())
                .spawn()
                .and_then(|child| {
                    std::process::Command::new("mpv")
                        .arg("-")
                        .stdin(child.stdout.unwrap())
                        .status()
                });
        }
        Err(e) => eprintln!("Erreur lancement mpv: {}", e),
        _ => {}
    }
}

fn main() {
    let source = AnimeSama::new();

    'main_loop: loop {
        // Step 1: Search
        let query = match Text::new("Rechercher un anime (Échap pour quitter) :")
            .prompt()
        {
            Ok(q) if !q.trim().is_empty() => q.trim().to_string(),
            Ok(_) => continue 'main_loop,
            Err(InquireError::OperationInterrupted | InquireError::OperationCanceled) => {
                break 'main_loop;
            }
            Err(e) => panic!("{}", e),
        };

        let mut sp = Spinner::new(Spinners::Moon, format!("Recherche de \"{}\"...", query));

        let results = match source.search(&query) {
            Ok(r) if !r.is_empty() => {
                sp.stop_with_symbol(" ✔️ ");
                r
            }
            Ok(_) => {
                sp.stop_with_symbol(" ❌ ");
                println!("{}", "Aucun résultat trouvé.".yellow());
                continue 'main_loop;
            }
            Err(e) => {
                sp.stop_with_symbol(" ❌ ");
                eprintln!("Erreur de recherche: {}", e);
                continue 'main_loop;
            }
        };

        // Step 2: Select anime
        let anime = match Select::new("Sélectionnez un anime (Échap pour retour) :", results)
            .prompt()
        {
            Ok(a) => a,
            Err(InquireError::OperationCanceled) => continue 'main_loop,
            Err(InquireError::OperationInterrupted) => break 'main_loop,
            Err(e) => panic!("{}", e),
        };

        // Step 3: Get seasons
        let mut sp2 = Spinner::new(Spinners::Moon, String::from("Chargement des saisons..."));

        let seasons = match source.get_seasons(&anime) {
            Ok(s) if !s.is_empty() => {
                sp2.stop_with_symbol(" ✔️ ");
                s
            }
            Ok(_) => {
                sp2.stop_with_symbol(" ❌ ");
                println!("{}", "Aucune saison trouvée.".yellow());
                continue 'main_loop;
            }
            Err(e) => {
                sp2.stop_with_symbol(" ❌ ");
                eprintln!("Erreur: {}", e);
                continue 'main_loop;
            }
        };

        'season_loop: loop {
            let season = match Select::new(
                "Sélectionnez la saison (Échap pour retour) :",
                seasons.clone(),
            )
            .prompt()
            {
                Ok(s) => s,
                Err(InquireError::OperationCanceled) => break 'season_loop,
                Err(InquireError::OperationInterrupted) => std::process::exit(0),
                Err(e) => panic!("{}", e),
            };

            // Step 4: Select language
            'lang_loop: loop {
                let lang = if season.langs.len() == 1 {
                    season.langs[0].clone()
                } else {
                    match Select::new(
                        "Langue (Échap pour retour) :",
                        season.langs.clone(),
                    )
                    .prompt()
                    {
                        Ok(l) => l,
                        Err(InquireError::OperationCanceled) => break 'lang_loop,
                        Err(InquireError::OperationInterrupted) => std::process::exit(0),
                        Err(e) => panic!("{}", e),
                    }
                };

                // Step 5: Get episodes
                let mut sp3 =
                    Spinner::new(Spinners::Moon, String::from("Chargement des épisodes..."));

                let episodes = match source.get_episodes(&anime, season.number, &lang) {
                    Ok(eps) => {
                        sp3.stop_with_symbol(" ✔️ ");
                        eps
                    }
                    Err(e) => {
                        sp3.stop_with_symbol(" ❌ ");
                        eprintln!("Erreur: {}", e);
                        continue 'lang_loop;
                    }
                };

                // Step 6: Download or Watch
                'action_loop: loop {
                    let action = match Select::new(
                        "Télécharger ou regarder ? (Échap pour retour)",
                        vec!["Télécharger", "Regarder"],
                    )
                    .prompt()
                    {
                        Ok(v) => v,
                        Err(InquireError::OperationCanceled) => break 'action_loop,
                        Err(InquireError::OperationInterrupted) => std::process::exit(0),
                        Err(e) => panic!("{}", e),
                    };

                    if action == "Télécharger" {
                        let mut ep_choices = vec![];
                        for i in 1..=episodes.len() {
                            ep_choices.push(format!("Épisode {}", i));
                        }

                        let selected_eps = match MultiSelect::new(
                            "Sélectionnez les épisodes (Espace pour choisir, Échap pour retour) :",
                            ep_choices,
                        )
                        .prompt()
                        {
                            Ok(v) => v,
                            Err(InquireError::OperationCanceled) => continue 'action_loop,
                            Err(InquireError::OperationInterrupted) => std::process::exit(0),
                            Err(e) => panic!("{}", e),
                        };

                        if selected_eps.is_empty() {
                            println!("{}", "Aucun épisode sélectionné.".yellow());
                            continue 'action_loop;
                        }

                        let indices: Vec<usize> = selected_eps
                            .iter()
                            .map(|s| s.replace("Épisode ", "").parse::<usize>().unwrap() - 1)
                            .collect();

                        if let Err(e) = download(&anime.name, season.number, &episodes, indices) {
                            eprintln!("Erreur lors du téléchargement: {}", e);
                        }
                    } else {
                        let mut episode_numbers = vec![];
                        for i in 1..=episodes.len() {
                            episode_numbers.push(format!("Épisode {}", i));
                        }

                        loop {
                            let ans = match Select::new(
                                "Sélectionnez l'épisode à regarder (Échap pour retour) :",
                                episode_numbers.clone(),
                            )
                            .prompt()
                            {
                                Ok(v) => v,
                                Err(InquireError::OperationCanceled) => break,
                                Err(InquireError::OperationInterrupted) => std::process::exit(0),
                                Err(e) => panic!("{}", e),
                            };

                            let ep_idx =
                                ans.replace("Épisode ", "").parse::<usize>().unwrap() - 1;
                            watch(&episodes[ep_idx]);
                        }
                    }
                } // 'action_loop

                if season.langs.len() == 1 {
                    break 'lang_loop;
                }
            } // 'lang_loop
        } // 'season_loop
    }
}
