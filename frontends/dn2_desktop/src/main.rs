mod app;
mod audio;
mod keys;
mod platform;
mod screenshot;

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use dn2_engine::{Buttons, Engine, FRAME_TICKS, Filter, Settings, Start, WIDESCREEN_COLUMNS};
use winit::event_loop::EventLoop;

/// Duke Nukem II in a desktop window.
#[derive(Parser)]
struct Args {
    /// The game folder (with NUKEM2.CMP).
    #[arg(long, default_value = ".")]
    data: PathBuf,
    /// Frame pixels per game pixel, before the picture is fitted to the
    /// window (default 1, or 3 with a smoothing filter or an HD pack).
    #[arg(long)]
    scale: Option<u32>,
    /// How the picture is made larger: nearest (blocks), epx (smoothed
    /// edges, at scales 2 to 4) or xbr (smoothed and blended edges).
    #[arg(long, value_enum, default_value_t = FilterArg::Nearest)]
    filter: FilterArg,
    /// A folder of higher-detail pictures (an HD pack: CZONE1.MNI/solid.png,
    /// ACTORS.MNI/5/0.png, DROP1.MNI.png, ...), shown at scales from 2.
    #[arg(long)]
    hd: Option<PathBuf>,
    /// A wider view for 16:9 displays; the game plays the same.
    #[arg(long)]
    wide: bool,
    /// Start in the level browser at this level (e.g. L1).
    #[arg(long)]
    level: Option<String>,
    /// With --level: the camera's top left map tile, as X,Y.
    #[arg(long, value_parser = parse_camera, requires = "level")]
    camera: Option<(u16, u16)>,
    /// Play this level (e.g. L1).
    #[arg(long, conflicts_with = "level")]
    play: Option<String>,
    /// Start in the asset viewer instead of the game.
    #[arg(long, conflicts_with_all = ["level", "play"])]
    viewer: bool,
    /// Draw one frame into this PNG file and exit, without a window.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// With --screenshot: game frames to play first, as BUTTONS:FRAMES
    /// separated by commas; buttons are U D L R J (jump) F (fire), or - for
    /// none (e.g. "R:20,F:1,-:5").
    #[arg(long, value_parser = parse_input, requires = "screenshot")]
    input: Option<Script>,
    /// No sound or music (the music still times the movies).
    #[arg(long)]
    mute: bool,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum FilterArg {
    Nearest,
    Epx,
    Xbr,
}

/// Buttons to hold, each for a number of frames.
#[derive(Clone)]
struct Script(Vec<(Buttons, u32)>);

fn parse_input(text: &str) -> Result<Script, String> {
    let steps = text
        .split(',')
        .map(|part| {
            let (keys, frames) = part.split_once(':').ok_or("expected BUTTONS:FRAMES")?;
            let mut buttons = Buttons::empty();
            for key in keys.trim().chars() {
                buttons |= match key.to_ascii_uppercase() {
                    'U' => Buttons::UP,
                    'D' => Buttons::DOWN,
                    'L' => Buttons::LEFT,
                    'R' => Buttons::RIGHT,
                    'J' => Buttons::JUMP,
                    'F' => Buttons::FIRE,
                    '-' => Buttons::empty(),
                    other => return Err(format!("unknown button {other}")),
                };
            }
            let frames = frames.trim().parse().map_err(|e| format!("{e}"))?;
            Ok((buttons, frames))
        })
        .collect::<Result<_, String>>()?;
    Ok(Script(steps))
}

fn parse_camera(text: &str) -> Result<(u16, u16), String> {
    let (x, y) = text.split_once(',').ok_or("expected X,Y")?;
    Ok((
        x.trim().parse().map_err(|e| format!("{e}"))?,
        y.trim().parse().map_err(|e| format!("{e}"))?,
    ))
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let platform = platform::FsPlatform::new(&args.data, args.hd.clone());
    let file = |name: String| {
        if name.contains('.') {
            name
        } else {
            format!("{name}.MNI")
        }
    };
    let start = match (args.level, args.play) {
        (Some(name), _) => Start::Level {
            name: file(name),
            camera: args.camera,
        },
        (_, Some(name)) => Start::Play { name: file(name) },
        _ if args.viewer => Start::Viewer,
        _ => Start::Game,
    };
    let filter = match args.filter {
        FilterArg::Nearest => Filter::Nearest,
        FilterArg::Epx => Filter::Epx,
        FilterArg::Xbr => Filter::Xbr,
    };
    let default_scale = if filter == Filter::Nearest && args.hd.is_none() {
        1
    } else {
        3
    };
    let settings = Settings {
        scale: args.scale.unwrap_or(default_scale).max(1),
        filter,
        columns: if args.wide { WIDESCREEN_COLUMNS } else { 40 },
        start,
    };
    let mut engine = Engine::new(Box::new(platform), settings)
        .with_context(|| format!("cannot load the game data from {}", args.data.display()))?;

    if let Some(path) = args.screenshot {
        for (buttons, frames) in args.input.map(|s| s.0).unwrap_or_default() {
            engine.set_buttons(buttons);
            for _ in 0..frames {
                engine.update(FRAME_TICKS);
            }
        }
        return screenshot::save(&mut engine, &path);
    }

    let audio = if args.mute {
        None
    } else {
        audio::AudioOut::open()
            .inspect_err(|e| log::warn!("no sound: {e}"))
            .ok()
    };
    let event_loop = EventLoop::new().context("cannot create the event loop")?;
    event_loop
        .run_app(&mut app::App::new(engine, audio, args.wide))
        .context("event loop failed")?;
    Ok(())
}
