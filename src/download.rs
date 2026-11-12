use crate::hosts::{Stream, best_stream};
use crate::names::DownloadTarget;
use crate::sources::{Anime, Episode, EpisodeList, Season, Source, USER_AGENT};
use anyhow::{Result, anyhow, bail};
use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::{fs, thread};
use threadpool::ThreadPool;

const PARALLEL_DOWNLOADS: usize = 6;
const PROGRESS_TEMPLATE: &str =
    "download:%(progress.downloaded_bytes)s %(progress.total_bytes,progress.total_bytes_estimate)s";

pub fn download(
    source: Arc<dyn Source>,
    anime: &Anime,
    season: &Season,
    list: Arc<EpisodeList>,
    selected: Vec<usize>,
) -> Result<()> {
    let target = Arc::new(DownloadTarget::new(anime, season, &list.lang));
    fs::create_dir_all(&target.dir)?;

    let bars = MultiProgress::new();
    let style = ProgressStyle::with_template(
        "{spinner:.blue} [{elapsed_precise}] [{bar:40.green/white}] {percent:>3}% {msg} {binary_bytes_per_sec:.yellow}",
    )?
    .progress_chars("=>-");

    let pool = ThreadPool::new(PARALLEL_DOWNLOADS);
    for index in selected {
        let bar = bars.add(ProgressBar::new(0).with_style(style.clone()));
        let (source, list, target) = (Arc::clone(&source), Arc::clone(&list), Arc::clone(&target));
        pool.execute(move || {
            download_episode(source.as_ref(), &list.episodes[index], &target, bar)
        });
    }
    pool.join();
    Ok(())
}

fn download_episode(
    source: &dyn Source,
    episode: &Episode,
    target: &DownloadTarget,
    bar: ProgressBar,
) {
    let label = format!("| Épisode {:0>2}", episode.label);
    bar.set_message(format!("{label} | recherche d'un lecteur..."));

    let result = best_stream(source, episode).and_then(|stream| {
        bar.set_message(match stream.height {
            Some(height) => format!("{label} | {height}p"),
            None => label.clone(),
        });
        run_ytdlp(&stream, &target.output_template(&episode.label), &bar)
    });

    match result {
        Ok(()) => bar.finish_with_message(format!("{label} | {}", "terminé".cyan())),
        Err(e) => bar.abandon_with_message(format!("{label} | {}", format!("{e:#}").red())),
    }
}

fn run_ytdlp(stream: &Stream, output_template: &str, bar: &ProgressBar) -> Result<()> {
    let mut ytdlp = Command::new("yt-dlp");
    ytdlp
        .args(["--newline", "--quiet", "--progress", "--no-warnings"])
        .args(["--progress-template", PROGRESS_TEMPLATE])
        .args(["--user-agent", USER_AGENT]);
    if let Some(referer) = &stream.referer {
        ytdlp.args(["--referer", referer]);
    }
    let mut child = ytdlp
        .args(["-o", output_template, &stream.url])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("impossible de lancer yt-dlp : {e}"))?;

    let last_error = collect_last_line(&mut child);
    if let Some(stdout) = child.stdout.take() {
        for (done, total) in BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
            .filter_map(|l| parse_progress(&l))
        {
            bar.set_length(total.max(done));
            bar.set_position(done);
        }
    }

    if child.wait()?.success() {
        return Ok(());
    }
    match last_error.join().ok().flatten() {
        Some(line) => bail!("{}", line.trim_start_matches("ERROR: ")),
        None => bail!("échec"),
    }
}

fn collect_last_line(child: &mut Child) -> thread::JoinHandle<Option<String>> {
    let stderr = child.stderr.take();
    thread::spawn(move || {
        let mut text = String::new();
        stderr?.read_to_string(&mut text).ok()?;
        text.lines()
            .rfind(|l| !l.trim().is_empty())
            .map(str::to_string)
    })
}

fn parse_progress(line: &str) -> Option<(u64, u64)> {
    let (done, total) = line.split_once(' ')?;
    let done: f64 = done.parse().ok()?;
    let total: f64 = total.parse().unwrap_or(0.0);
    Some((done as u64, total as u64))
}
