use std::{fmt::{Debug, Display}, process::exit, time::Duration};
use mpris::{DBusError, PlaybackStatus, PlayerFinder};
use clap::{ArgAction, Parser, Subcommand, ValueEnum};

#[derive(Debug)]
enum PlayingErrorKind {
    DBus,
    IO,
    Spotifav,
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

impl From<Box<dyn std::error::Error>> for PlayingError {
    fn from(value: Box<dyn std::error::Error>) -> Self {
        PlayingError { kind: PlayingErrorKind::IO, code: 4, inner: value }
    }
}

impl PlayingError {
    fn from_spotifav(e: Box<dyn std::error::Error>) -> Self {
        PlayingError { kind: PlayingErrorKind::Spotifav, code: 5, inner: e }
    }
}

#[tokio::main]
async fn main() {
    let cmd = Cmd::parse();

    match run(cmd).await {
        Ok(e) => exit(if e { 0 } else { 1 }),
        Err(e) => {
            eprintln!("error: {}: {}", e.kind, e.inner);
            exit(e.code);
        }
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
    Rewind {
        #[arg(default_value = "1")]
        seconds: f32,
    },
    Forward {
        #[arg(default_value = "1")]
        seconds: f32,
    },
    SeekRelative {
        seconds: f32,
    },
    Seek {
        seconds: f32,
    }
}

#[derive(Subcommand,Debug)]
enum Action {
    #[command(subcommand, alias = "op")]
    Operation(Operation),
    Player,
    Status { 
        #[arg(action = ArgAction::SetTrue, long)]
        no_icon: bool,
        #[arg(default_value = "1", long)]
        spaces_after_icon: usize,
        #[arg(action = ArgAction::SetTrue, short)]
        quiet: bool 
    },
    Favorite {
        #[arg(default_value = "false", short, long)]
        poll: bool,
        #[arg(long)]
        always: bool,
    },
    Url,
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
            Firefox => "Mozilla firefox",
            Spotify => "Spotify",
            Chrome => "chrome",
            Custom(s) => s,
        }
    }

    fn parse(s: &str) -> Option<Player> {
        match s {
            "mpv" => Some(Mpv),
            "vlc" => Some(Vlc),
            "Mozilla firefox" => Some(Firefox),
            "Spotify" => Some(Spotify),
            "chrome" => Some(Chrome),
            // c => { println!("{}", c); None },
            _ => None,
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            Mpv => "",
            Vlc => "󰕼",
            Firefox => "",
            Spotify => "",
            Chrome => "",
            Custom(_) => "",
        }
    }
}

const MAX_STATUS_LEN: usize = 70;

async fn run(cmd: Cmd) -> Result<bool, PlayingError> {
    //eprintln!("{:?}", cmd);
    let finder = match PlayerFinder::new() {
        Ok(f) => f,
        Err(e) => return Err(PlayingError {
            kind: PlayingErrorKind::DBus,
            code: 8,
            inner: e.into(),
        }),
    };

    if let Action::Favorite { always, poll } = cmd.action {
        if finder.find_by_name("Spotify").is_ok() || always {
            let cli = spotifav::get_client().await.map_err(PlayingError::from_spotifav)?;
            if poll {
                spotifav::poll(&cli).await.map_err(PlayingError::from_spotifav)?;
            }
            if spotifav::do_toggle(&cli).await.map_err(PlayingError::from_spotifav)? {
                println!("added song to favorites");
            } else {
                println!("removed song from favorites");
            }
            return Ok(true)
        } else {
            eprintln!("spotify is not playing");
            return Ok(false)
        }
    }

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
                        Operation::Rewind { seconds } => {
                            //let pos = p.get_position().unwrap();
                            p.seek_backwards(&Duration::from_secs_f32(*seconds))?
                        }
                        Operation::Forward { seconds } => {
                            //let pos = p.get_position().unwrap();
                            p.seek_forwards(&Duration::from_secs_f32(*seconds))?
                        }
                        Operation::SeekRelative { seconds } => {
                            p.seek((seconds * (1 << 6) as f32) as i64)?
                        },
                        Operation::Seek { seconds } => {
                            if let Some(id) = p.get_metadata()?.track_id() {
                                p.set_position(id, &Duration::from_secs_f32(*seconds))?
                            }
                        }
                    }
                    Action::Status { no_icon, spaces_after_icon, quiet } => {
                        // println!("status: {:?}", p.get_playback_status()?);
                        if p.get_playback_status()? == PlaybackStatus::Playing {
                            if quiet {
                                return Ok(false)
                            }
                            let meta = p.get_metadata()?;
                            let title = meta.title().unwrap_or("Unknown");
                            let album = meta.album_name().unwrap_or("Unknown");
                            let mut artists = meta.album_artists().unwrap_or(vec![]);
                            if artists.is_empty() {
                                artists.push("Unknown")
                            }

                            let icon = match Player::parse(p.identity()) {
                                Some(pl) => pl.icon(),
                                None => ""
                            };

                            let icon = format!("{}", if no_icon {
                                "".to_owned()
                            } else {
                                format!("{}{}", icon, " ".repeat(spaces_after_icon))
                            });

                            let line = format!("{}{} // {} @ {}", icon, title, album, artists[0]);
                            if line.len() > MAX_STATUS_LEN {
                                println!("{}...", &line[..MAX_STATUS_LEN-3].to_string());
                            } else {
                                println!("{}", line);
                            }
                            return Ok(true)
                        }
                    }
                    Action::Favorite { .. } => {}
                    Action::Url => {
                        if let Some(_) = Player::parse(p.identity()) {
                            let meta = p.get_metadata()?;
                            print!("{}", meta.url().unwrap_or(""));
                        }
                    }
                    Action::Player => {
                        println!("{}", p.identity());
                    }
                }
            }
        }
    }

    if let Action::Status { quiet, .. } = cmd.action {
        match quiet {
            true => return Ok(false),
            false => println!("No media")
        }
    }

    Ok(true)
}
