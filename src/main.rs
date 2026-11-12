use colored::Colorize;
use hosts::Stream;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use inquire::{InquireError, MultiSelect, Select, Text};
use sources::anime_sama::{AnimeSama, USER_AGENT};
use sources::{Anime, Episode, EpisodeList, Season, Source};
use spinners::{Spinner, Spinners};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};
use threadpool::ThreadPool;

mod hosts;
mod sources;

const PARALLEL_DOWNLOADS: usize = 6;

fn answer<T>(res: Result<T, InquireError>) -> Option<T> {
    match res {
        Ok(v) => Some(v),
        Err(InquireError::OperationCanceled) => None,
        Err(InquireError::OperationInterrupted) => std::process::exit(0),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn with_spinner<T>(msg: &str, f: impl FnOnce() -> anyhow::Result<T>) -> Option<T> {
    let mut sp = Spinner::new(Spinners::Moon, msg.to_string());
    match f() {
        Ok(v) => {
            sp.stop_with_symbol(" ✔️ ");
            Some(v)
        }
        Err(e) => {
            sp.stop_with_symbol(" ❌ ");
            eprintln!("{}", format!("{e:#}").red());
            None
        }
    }
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

fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    cleaned.trim_end_matches(['.', ' ']).to_string()
}

fn lang_label(lang: &str) -> String {
    lang.to_uppercase()
}

fn find_stream(source: &dyn Source, list: &EpisodeList, ep: &Episode) -> anyhow::Result<Stream> {
    let mut players = ep.players.clone();
    players.sort_by_key(|p| hosts::priority(&p.host));

    let mut errors = Vec::new();
    for player in &players {
        let res = source
            .embed_url(player)
            .and_then(|embed| hosts::resolve(&embed, &list.referer));
        match res {
            Ok(stream) => return Ok(stream),
            Err(e) => errors.push(format!("{}: {e:#}", player.host)),
        }
    }
    if errors.is_empty() {
        anyhow::bail!("aucun lecteur disponible pour cet épisode");
    }
    anyhow::bail!("aucun lecteur ne fonctionne ({})", errors.join(" ; "))
}

fn download(
    source: Arc<dyn Source>,
    anime: &Anime,
    season: &Season,
    list: &Arc<EpisodeList>,
    selected: Vec<usize>,
) -> anyhow::Result<()> {
    let anime_name = sanitize(&to_title_case(&anime.name));
    let season_name = sanitize(&season.name);
    let mut dir_name = season_name.clone();
    if list.lang != "vostfr" {
        dir_name = format!("{dir_name} ({})", lang_label(&list.lang));
    }
    let season_dir: PathBuf = Path::new(&anime_name).join(&dir_name);
    fs::create_dir_all(&season_dir)?;

    let pool = ThreadPool::new(PARALLEL_DOWNLOADS);
    let m = MultiProgress::new();
    let style = ProgressStyle::with_template(
        "{spinner:.blue} [{elapsed_precise}] [{bar:40.green/white}] {percent:>3}% {msg}",
    )?
    .progress_chars("=>-");

    for index in selected {
        let source = Arc::clone(&source);
        let list = Arc::clone(list);
        let m = m.clone();
        let style = style.clone();
        let output_template = season_dir
            .join(format!(
                "{anime_name} - {season_name} E{:02}.%(ext)s",
                index + 1
            ))
            .display()
            .to_string();

        pool.execute(move || {
            let ep = &list.episodes[index];
            let label = format!("| Épisode {:02}", ep.number);
            let pb = m.add(ProgressBar::new(100));
            pb.set_style(style);
            pb.set_message(format!("{label} | recherche d'un lecteur..."));

            let stream = match find_stream(source.as_ref(), &list, ep) {
                Ok(s) => s,
                Err(e) => {
                    pb.abandon_with_message(format!("{label} | {}", format!("{e:#}").red()));
                    return;
                }
            };
            pb.set_message(label.clone());

            let mut cmd = Command::new("yt-dlp");
            cmd.args(["--newline", "--progress", "--user-agent", USER_AGENT]);
            if let Some(r) = &stream.referer {
                cmd.args(["--referer", r]);
            }
            cmd.arg("-o")
                .arg(&output_template)
                .arg(&stream.url)
                .stdout(Stdio::piped())
                .stderr(Stdio::null());

            let mut child = match cmd.spawn() {
                Ok(child) => child,
                Err(err) => {
                    pb.abandon_with_message(format!("Erreur lancement yt-dlp: {err}"));
                    return;
                }
            };

            if let Some(stdout) = child.stdout.take() {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if !line.contains("[download]") {
                        continue;
                    }
                    if let Some(percent) = extract_percent(&line) {
                        pb.set_position(percent as u64);
                    }
                    if let Some(speed) = extract_speed(&line) {
                        pb.set_message(format!("{label} | {}", speed.yellow()));
                    }
                }
            }

            match child.wait() {
                Ok(status) if status.success() => {
                    pb.finish_with_message(format!("{label} | {}", "terminé".cyan()))
                }
                _ => pb.abandon_with_message(format!("{label} | {}", "échec".red())),
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
    line.get(at..eta).map(str::trim)
}

fn watch(stream: &Stream, title: &str) {
    let mut cmd = Command::new("mpv");
    cmd.arg(format!("--force-media-title={title}"))
        .arg(format!("--user-agent={USER_AGENT}"));
    if let Some(r) = &stream.referer {
        cmd.arg(format!("--referrer={r}"));
    }
    cmd.arg(&stream.url);
    match cmd.status() {
        Ok(s) if s.success() => {}
        Ok(_) => eprintln!("{}", "mpv s'est arrêté avec une erreur.".yellow()),
        Err(e) => eprintln!("{}", format!("Impossible de lancer mpv : {e}").red()),
    }
}

fn episode_labels(list: &EpisodeList) -> Vec<String> {
    list.episodes
        .iter()
        .map(|e| {
            if e.players.is_empty() {
                format!("Épisode {} (indisponible)", e.number)
            } else {
                format!("Épisode {}", e.number)
            }
        })
        .collect()
}

fn watch_loop(source: &dyn Source, anime: &Anime, season: &Season, list: &EpisodeList) {
    let labels = episode_labels(list);
    let mut cursor = 0;
    while let Some(choice) = answer(
        Select::new("Épisode à regarder (Échap pour retour) :", labels.clone())
            .with_starting_cursor(cursor)
            .raw_prompt(),
    ) {
        let ep = &list.episodes[choice.index];
        let msg = format!("Recherche d'un lecteur pour l'épisode {}", ep.number);
        if let Some(stream) = with_spinner(&msg, || find_stream(source, list, ep)) {
            let title = format!("{} - {} - Épisode {}", anime.name, season.name, ep.number);
            watch(&stream, &title);
            cursor = (choice.index + 1).min(labels.len() - 1);
        } else {
            cursor = choice.index;
        }
    }
}

fn season_menu(source: &Arc<dyn Source>, anime: &Anime, season: &Season) {
    let msg = "Chargement des épisodes...";
    let Some(mut list) = with_spinner(msg, || source.episodes(season, &season.default_lang)) else {
        return;
    };

    'lang: loop {
        if list.langs.len() > 1 {
            let labels: Vec<String> = list.langs.iter().map(|l| lang_label(l)).collect();
            let current = list.langs.iter().position(|l| *l == list.lang).unwrap_or(0);
            let Some(choice) = answer(
                Select::new("Langue (Échap pour retour) :", labels)
                    .with_starting_cursor(current)
                    .raw_prompt(),
            ) else {
                return;
            };
            let lang = list.langs[choice.index].clone();
            if lang != list.lang {
                match with_spinner(msg, || source.episodes(season, &lang)) {
                    Some(l) => list = l,
                    None => continue 'lang,
                }
            }
        }

        let list = Arc::new(list.clone());
        while let Some(action) = answer(
            Select::new(
                &format!(
                    "{} ({}) : {} épisodes. Que faire ? (Échap pour retour)",
                    season.name,
                    lang_label(&list.lang),
                    list.episodes.len()
                ),
                vec!["Regarder", "Télécharger"],
            )
            .prompt(),
        ) {
            if action == "Regarder" {
                watch_loop(source.as_ref(), anime, season, &list);
                continue;
            }

            let Some(selected) = answer(
                MultiSelect::new(
                    "Épisodes à télécharger (Espace pour choisir, → pour tout, Échap pour retour) :",
                    episode_labels(&list),
                )
                .raw_prompt(),
            ) else {
                continue;
            };
            if selected.is_empty() {
                println!("{}", "Aucun épisode sélectionné.".yellow());
                continue;
            }
            let indices = selected.iter().map(|o| o.index).collect();
            if let Err(e) = download(Arc::clone(source), anime, season, &list, indices) {
                eprintln!("{}", format!("Erreur lors du téléchargement : {e:#}").red());
            }
        }

        if list.langs.len() <= 1 {
            return;
        }
    }
}

fn main() {
    let Some(source) = with_spinner("Connexion à anime-sama", AnimeSama::connect) else {
        std::process::exit(1);
    };
    let source: Arc<dyn Source> = Arc::new(source);

    while let Some(query) = answer(Text::new("Rechercher un anime (Échap pour quitter) :").prompt())
    {
        let query = query.trim();
        if query.is_empty() {
            continue;
        }

        let Some(results) = with_spinner(&format!("Recherche de \"{query}\""), || {
            source.search(query)
        }) else {
            continue;
        };
        if results.is_empty() {
            println!("{}", "Aucun résultat.".yellow());
            continue;
        }

        while let Some(anime) = answer(
            Select::new(
                "Sélectionnez un anime (Échap pour retour) :",
                results.clone(),
            )
            .prompt(),
        ) {
            let Some(seasons) =
                with_spinner("Chargement des saisons...", || source.seasons(&anime))
            else {
                continue;
            };
            if seasons.is_empty() {
                println!("{}", "Aucune saison disponible pour cet anime.".yellow());
                continue;
            }

            while let Some(season) = answer(
                Select::new(
                    "Sélectionnez la saison (Échap pour retour) :",
                    seasons.clone(),
                )
                .prompt(),
            ) {
                season_menu(&source, &anime, &season);
            }
        }
    }
}
