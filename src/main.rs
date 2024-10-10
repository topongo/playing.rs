use std::{fmt::{Debug, Display}, process::exit, time::Duration, process::Command};
use mpris::{DBusError, PlaybackStatus, PlayerFinder};
use clap::{Parser,Subcommand,ValueEnum};

#[derive(Debug)]
enum PlayingErrorKind {
    DBus,
    IO,
}

impl Display for PlayingErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

struct PlayingError {
    kind: PlayingErrorKind,
    inner: Box<dyn std::error::Error>,
    code: i32,
}

impl From<DBusError> for PlayingError {
    fn from(value: DBusError) -> Self {
        PlayingError { kind: PlayingErrorKind::DBus, code: 2, inner: Box::new(value) }
    }
}

impl From<std::io::Error> for PlayingError {
    fn from(value: std::io::Error) -> Self {
        PlayingError { kind: PlayingErrorKind::IO, code: 3, inner: Box::new(value) }
    }
}

fn main() {
    let cmd = Cmd::parse();

    if let Err(e) = run(cmd) {
        eprintln!("error: {}: {}", e.kind, e.inner);
        exit(e.code);
    }
}

#[derive(Clone,PartialEq,Eq,PartialOrd,Ord,ValueEnum,Debug)]
enum Mode {
    Single,
    Multiple,
}

#[derive(Subcommand, Debug)]
enum Operation {
    Toggle,
    Play,
    Pause,
    Next,
    Previous,
    Rewind,
    Forward,
}

#[derive(Subcommand,Debug)]
enum Action {
    #[command(subcommand)]
    Operation(Operation),
    Status,
    Favorite,
}

#[derive(Parser,Debug)]
#[command(
    name = "playing.rs",
    about = "Manage your running multimedia players using mpris",
    version = env!("CARGO_PKG_VERSION"),
    author = "topongo"
)]
struct Cmd {
    #[arg(value_enum,short,long,default_value = "single")]
    mode: Mode,
    #[command(subcommand)]
    action: Action,
}

#[derive(PartialEq,Eq,PartialOrd,Ord,Debug)]
enum Player {
    Mpv,
    Vlc,
    Firefox,
    Spotify,
    Chrome,
    Custom(&'static str)
}
use Player::*;

impl Player {
    fn to_str(&self) -> &'static str {
        match self {
            Mpv => "mpv",
            Vlc => "vlc",
            Firefox => "firefox",
            Spotify => "Spotify",
            Chrome => "chrome",
            Custom(s) => s,
        }
    }

    fn parse(s: &str) -> Option<Player> {
        match s {
            "mpv" => Some(Mpv),
            "vlc" => Some(Vlc),
            "firefox" => Some(Firefox),
            "Spotify" => Some(Spotify),
            "chrome" => Some(Chrome),
            _ => None,
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            Mpv => "",
            Vlc => "嗢",
            Firefox => "",
            Spotify => "",
            Chrome => "",
            Custom(_) => "",
        }
    }
}

const PYTHON_EXEC: &'static str = "/home/topongo/documents/python/spotify_utils/venv/bin/python";
const SPOTIFY_FAVOURITE: &'static str = "/home/topongo/documents/python/spotify_utils/toggle_current_song_favourites.py";
const SPOTIFY_FAVOURITE_PATH: &'static str = "/home/topongo/documents/python/spotify_utils/";
const MAX_STATUS_LEN: usize = 70;

fn run(cmd: Cmd) -> Result<(), PlayingError> {
    //eprintln!("{:?}", cmd);
    let finder = match PlayerFinder::new() {
        Ok(f) => f,
        Err(e) => return Err(PlayingError {
            kind: PlayingErrorKind::DBus,
            code: 8,
            inner: e.into(),
        }),
    };

    let ranking = vec![Custom("mpv"), Vlc, Firefox, Spotify, Chrome];

    for id in ranking {
        // println!("Checking for {}", id.to_str());
        for p in finder.find_all().unwrap() {
            // println!("\tFound {}", p.identity());
            if p.identity() == id.to_str() {
                match cmd.action {
                    Action::Operation(ref op) => match op {
                        Operation::Toggle => {
                            if let PlaybackStatus::Playing = p.get_playback_status().unwrap() {
                                p.pause()?
                            } else {
                                p.play()?
                            }
                        },
                        Operation::Play => p.play()?,
                        Operation::Pause => p.pause()?,
                        Operation::Next => p.next()?,
                        Operation::Previous => p.previous()?,
                        Operation::Rewind => {
                            //let pos = p.get_position().unwrap();
                            p.seek_backwards(&Duration::from_secs(1))?
                        }
                        Operation::Forward => {
                            //let pos = p.get_position().unwrap();
                            p.seek_forwards(&Duration::from_secs(1))?
                        }
                    }
                    Action::Status => {
                        if p.get_playback_status()? == PlaybackStatus::Playing {
                            let meta = p.get_metadata()?;
                            let title = meta.title().unwrap_or("Unknown");
                            let album = meta.album_name().unwrap_or("Unknown");
                            let mut artists = meta.album_artists().unwrap_or(vec![]);
                            if artists.len() == 0 {
                                artists.push("Unknown")
                            }

                            let icon = if let Some(pl) = Player::parse(p.identity()) {
                                pl.icon()
                            } else {
                                ""
                            };

                            let line = format!("{}  {} // {} @ {}", icon, title, album, artists[0]);
                            if line.len() > MAX_STATUS_LEN {
                                println!("{}...", line[..MAX_STATUS_LEN-3].to_string());
                            } else {
                                println!("{}", line);
                            }
                            return Ok(())
                        }
                    }
                    Action::Favorite => {
                        if let Some(pl) = Player::parse(p.identity()) {
                            if pl == Player::Spotify {
                                Command::new(PYTHON_EXEC)
                                    .arg(SPOTIFY_FAVOURITE)
                                    .current_dir(SPOTIFY_FAVOURITE_PATH)
                                    .spawn()?
                                    .wait()?;
                            }
                        }
                    }
                }
            }
        }
    }

    if let Action::Status = cmd.action {
        println!("No media");
    }

    Ok(())
}
