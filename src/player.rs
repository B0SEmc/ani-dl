use crate::hosts::Stream;
use crate::sources::USER_AGENT;
use colored::Colorize;
use std::process::Command;

pub fn watch(stream: &Stream, title: &str) {
    let mut mpv = Command::new("mpv");
    mpv.arg(format!("--force-media-title={title}"))
        .arg(format!("--user-agent={USER_AGENT}"));
    if let Some(referer) = &stream.referer {
        mpv.arg(format!("--referrer={referer}"));
    }
    mpv.arg(&stream.url);

    match mpv.status() {
        Ok(status) if status.success() => {}
        Ok(_) => eprintln!("{}", "mpv s'est arrêté avec une erreur.".yellow()),
        Err(e) => eprintln!("{}", format!("Impossible de lancer mpv : {e}").red()),
    }
}
