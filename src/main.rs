use colored::*;
use inquire::*;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::thread;

// {
//   "anime": [
//     {
//       "name": "2.43-seiin-koukou-danshi-volley-bu",
//       "lang": "vostfr",
//       "season": 1,
//       "episodes": [
//         "https://video.sibnet.ru/shell.php?videoid=4206093",
//         "https://video.sibnet.ru/shell.php?videoid=4209756"
//       ]
//     },

#[derive(Serialize, Deserialize, Debug)]
struct Animes {
    anime: Vec<Anime>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Anime {
    name: String,
    lang: String,
    season: u8,
    episodes: Vec<String>,
}

impl Display for Anime {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.season > 1 {
            write!(
                f,
                "{} saison {} ({})",
                self.name.blue(),
                self.season.to_string().yellow(),
                self.lang.green()
            )
        } else {
            write!(f, "{} ({})", self.name.blue(), self.lang.green())
        }
    }
}

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

fn main() {
    let file = std::fs::File::open("anime_data.json").unwrap();
    let animes: Animes = serde_json::from_reader(file).unwrap();

    let ans: Result<Anime, InquireError> =
        Select::new("Sélectionnez les animes: ", animes.anime).prompt();

    let choice = ans.unwrap();

    let ans2 = Confirm::new("Voulez-vous télécharger les épisodes?")
        .with_help_message("A supprimer pour la release")
        .with_default(false)
        .prompt();

    if ans2.unwrap() {
        download(choice.episodes);
    } else {
        dbg!(&choice.episodes);
    }
}
