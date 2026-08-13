mod action;
mod puzzle;
mod turns;

use std::io::{self, IsTerminal as _, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{
    BeginSynchronizedUpdate, Clear, ClearType, DisableLineWrap, EnableLineWrap,
    EndSynchronizedUpdate, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode, size,
};
use crossterm::{execute, queue};

use crate::{VERSION, json_string, use_color};

const HELP: &str = "Native clean-room terminal arcade\n\nUsage:\n  games\n  games list [--plain | --json]\n  games info NAME\n  games run NAME [--seed NUMBER]\n\nOptions:\n  --plain       Stable text without decoration\n  --json        Machine-readable game list\n  --seed NUMBER Reproducible randomized setup\n  -h, --help    Show this help\n  -V, --version Show the version\n\nInteractive controls: arrows or WASD; Esc returns to the menu. Most modes use r to restart and q to quit; text modes use Ctrl-R and Ctrl-C.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Input {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Backspace,
    Char(char),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Playing,
    Won(String),
    Lost(String),
}

pub(crate) struct Scene {
    pub title: &'static str,
    pub status: String,
    pub lines: Vec<String>,
    pub help: &'static str,
}

pub(crate) trait Game {
    fn input(&mut self, input: Input);
    fn tick(&mut self) {}
    fn tick_rate(&self) -> Option<Duration> {
        None
    }
    fn scene(&self, width: u16, height: u16) -> Scene;
    fn outcome(&self) -> &Outcome;
    fn accepts_text(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    pub fn index(&mut self, length: usize) -> usize {
        if length == 0 {
            0
        } else {
            (self.next() % length as u64) as usize
        }
    }

    pub fn chance(&mut self, numerator: u64, denominator: u64) -> bool {
        denominator != 0 && self.next() % denominator < numerator
    }
}

struct ModeSpec {
    name: &'static str,
    title: &'static str,
    summary: &'static str,
    factory: fn(u64) -> Box<dyn Game>,
}

const MODES: &[ModeSpec] = &[
    ModeSpec {
        name: "delve",
        title: "DELVE",
        summary: "Escape a compact three-floor dungeon",
        factory: action::delve,
    },
    ModeSpec {
        name: "orbit",
        title: "ORBIT COURT",
        summary: "Win a twelve-possession space match",
        factory: turns::orbit,
    },
    ModeSpec {
        name: "serpent",
        title: "SERPENT RUN",
        summary: "Eat twelve sparks without hitting the wall",
        factory: action::serpent,
    },
    ModeSpec {
        name: "merge",
        title: "NUMBER MERGE",
        summary: "Slide matching tiles to reach 256",
        factory: puzzle::merge,
    },
    ModeSpec {
        name: "vector",
        title: "EXIT VECTOR",
        summary: "Find an access card and escape a raycast maze",
        factory: action::vector,
    },
    ModeSpec {
        name: "cards",
        title: "HAND THRESHOLD",
        summary: "Build scoring card hands across three rounds",
        factory: puzzle::cards,
    },
    ModeSpec {
        name: "seedling",
        title: "SEEDLING",
        summary: "Guide a plant through a seven-day cycle",
        factory: turns::seedling,
    },
    ModeSpec {
        name: "sprint",
        title: "TYPE SPRINT",
        summary: "Complete an original prompt with five mistakes or fewer",
        factory: puzzle::sprint,
    },
    ModeSpec {
        name: "keybed",
        title: "KEYBED",
        summary: "Repeat an eight-note visual melody",
        factory: turns::keybed,
    },
    ModeSpec {
        name: "glyphs",
        title: "FIVE GLYPHS",
        summary: "Find a five-letter answer in six guesses",
        factory: puzzle::glyphs,
    },
    ModeSpec {
        name: "scout",
        title: "FIELD SCOUT",
        summary: "Recruit a creature and reach the field gate",
        factory: turns::scout,
    },
];

pub fn run(arguments: Vec<String>) -> Result<i32, String> {
    let mut arguments = arguments.into_iter();
    match arguments.next().as_deref() {
        None => {
            if io::stdin().is_terminal() && io::stdout().is_terminal() {
                interactive(None, session_seed())
            } else {
                list(Vec::new())
            }
        }
        Some("list") => list(arguments.collect()),
        Some("info") => {
            let name = arguments
                .next()
                .ok_or_else(|| "info needs a game name".to_owned())?;
            if arguments.next().is_some() {
                return Err("info accepts one game name".into());
            }
            info(&name)
        }
        Some("run") => {
            let name = arguments
                .next()
                .ok_or_else(|| "run needs a game name".to_owned())?;
            let seed = parse_seed(arguments.collect())?;
            find_mode(&name)?;
            if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                return Err("interactive games need a terminal; run this command directly".into());
            }
            interactive(Some(&name), seed.unwrap_or_else(session_seed))
        }
        Some("-h" | "--help") => {
            println!("{HELP}");
            Ok(0)
        }
        Some("-V" | "--version") => {
            println!("games {VERSION}");
            Ok(0)
        }
        Some(command) => Err(format!("unknown command {command:?}; try 'games --help'")),
    }
}

fn parse_seed(arguments: Vec<String>) -> Result<Option<u64>, String> {
    match arguments.as_slice() {
        [] => Ok(None),
        [flag, value] if flag == "--seed" => value
            .parse()
            .map(Some)
            .map_err(|_| "--seed needs an unsigned integer".into()),
        _ => Err("usage: games run NAME [--seed NUMBER]".into()),
    }
}

fn list(arguments: Vec<String>) -> Result<i32, String> {
    let mut plain = false;
    let mut json = false;
    for argument in arguments {
        match argument.as_str() {
            "--plain" => plain = true,
            "--json" => json = true,
            _ => return Err(format!("unknown list option {argument:?}")),
        }
    }
    if plain && json {
        return Err("--plain and --json cannot be used together".into());
    }
    if json {
        let values = MODES
            .iter()
            .map(|mode| {
                format!(
                    "{{\"name\":{},\"title\":{},\"summary\":{},\"native\":true}}",
                    json_string(mode.name),
                    json_string(mode.title),
                    json_string(mode.summary),
                )
            })
            .collect::<Vec<_>>();
        println!("[{}]", values.join(","));
    } else if use_color(plain) {
        println!("\x1b[38;2;125;207;255m+-- GAMES \x1b[38;2;86;95;137m// native arcade\x1b[0m");
        for mode in MODES {
            println!(
                "\x1b[38;2;125;207;255m|\x1b[0m \x1b[38;2;158;206;106m*\x1b[0m {:10} {}",
                mode.name, mode.summary
            );
        }
        println!("\x1b[38;2;125;207;255m+--\x1b[0m \x1b[38;2;86;95;137mgames run NAME\x1b[0m");
    } else {
        for mode in MODES {
            println!("{}\tnative\t{}", mode.name, mode.summary);
        }
    }
    Ok(0)
}

fn info(name: &str) -> Result<i32, String> {
    let mode = find_mode(name)?;
    println!("Name\t{}", mode.name);
    println!("Title\t{}", mode.title);
    println!("Implementation\tNative clean-room Rust");
    println!("Summary\t{}", mode.summary);
    Ok(0)
}

fn find_mode(name: &str) -> Result<&'static ModeSpec, String> {
    MODES
        .iter()
        .find(|mode| mode.name == name)
        .ok_or_else(|| format!("unknown game {name:?}; run 'games list'"))
}

fn session_seed() -> u64 {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    time ^ u64::from(std::process::id())
}

fn interactive(initial: Option<&str>, mut seed: u64) -> Result<i32, String> {
    let _terminal = TerminalSession::enter().map_err(|error| format!("terminal: {error}"))?;
    let mut stdout = io::stdout();
    let mut selected = initial
        .and_then(|name| MODES.iter().position(|mode| mode.name == name))
        .unwrap_or(0);
    let mut active = initial.map(|_| (MODES[selected].factory)(seed));
    let mut next_tick = schedule_tick(active.as_deref());
    let color = use_color(false);

    loop {
        let terminal_size = size().unwrap_or((80, 24));
        let scene = active.as_ref().map_or_else(
            || menu_scene(selected),
            |game| game.scene(terminal_size.0, terminal_size.1),
        );
        let outcome = active
            .as_ref()
            .map_or(&Outcome::Playing, |game| game.outcome());
        draw(&mut stdout, &scene, outcome, terminal_size, color)
            .map_err(|error| format!("draw: {error}"))?;

        let tick_rate = active.as_ref().and_then(|game| game.tick_rate());
        let input_ready = tick_rate.map_or(Ok(true), |rate| {
            event::poll(
                next_tick
                    .saturating_duration_since(Instant::now())
                    .min(rate),
            )
        });

        if input_ready.map_err(|error| format!("input: {error}"))? {
            match event::read().map_err(|error| format!("input: {error}"))? {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    if quit_key(key, active.as_ref().is_some_and(|game| game.accepts_text())) {
                        break;
                    }
                    if key.code == KeyCode::Esc {
                        if active.is_some() {
                            active = None;
                        } else {
                            break;
                        }
                        continue;
                    }
                    if active.is_some()
                        && restart_key(key, active.as_ref().is_some_and(|game| game.accepts_text()))
                    {
                        active = Some((MODES[selected].factory)(seed));
                        next_tick = schedule_tick(active.as_deref());
                        continue;
                    }
                    if let Some(game) = active.as_mut() {
                        if game.outcome() != &Outcome::Playing && key.code == KeyCode::Enter {
                            seed = seed.wrapping_add(1);
                            active = Some((MODES[selected].factory)(seed));
                            next_tick = schedule_tick(active.as_deref());
                        } else if game.outcome() == &Outcome::Playing {
                            game.input(map_key(key));
                        }
                    } else {
                        match map_key(key) {
                            Input::Up | Input::Char('k') => {
                                selected = selected.checked_sub(1).unwrap_or(MODES.len() - 1);
                            }
                            Input::Down | Input::Char('j') => {
                                selected = (selected + 1) % MODES.len();
                            }
                            Input::Enter => {
                                active = Some((MODES[selected].factory)(seed));
                                next_tick = schedule_tick(active.as_deref());
                            }
                            _ => {}
                        }
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        if let Some(game) = active.as_mut()
            && let Some(rate) = game.tick_rate()
            && Instant::now() >= next_tick
        {
            if game.outcome() == &Outcome::Playing {
                game.tick();
            }
            next_tick = Instant::now() + rate;
        }
    }
    Ok(0)
}

fn schedule_tick(game: Option<&dyn Game>) -> Instant {
    Instant::now() + game.and_then(|game| game.tick_rate()).unwrap_or_default()
}

fn quit_key(key: KeyEvent, accepts_text: bool) -> bool {
    !accepts_text && matches!(key.code, KeyCode::Char('q' | 'Q'))
        || key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c' | 'C'))
}

fn restart_key(key: KeyEvent, accepts_text: bool) -> bool {
    if accepts_text {
        key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('r' | 'R'))
    } else {
        matches!(key.code, KeyCode::Char('r' | 'R'))
    }
}

fn map_key(key: KeyEvent) -> Input {
    match key.code {
        KeyCode::Up => Input::Up,
        KeyCode::Down => Input::Down,
        KeyCode::Left => Input::Left,
        KeyCode::Right => Input::Right,
        KeyCode::Enter => Input::Enter,
        KeyCode::Backspace => Input::Backspace,
        KeyCode::Char(character) => Input::Char(character.to_ascii_lowercase()),
        _ => Input::Char('\0'),
    }
}

fn menu_scene(selected: usize) -> Scene {
    let lines = MODES
        .iter()
        .enumerate()
        .map(|(index, mode)| {
            format!(
                "{} {:10}  {}",
                if index == selected { ">" } else { " " },
                mode.name,
                mode.summary
            )
        })
        .collect();
    Scene {
        title: "ARCADE",
        status: MODES[selected].title.into(),
        lines,
        help: "Up/Down choose  Enter play  q quit",
    }
}

fn draw<W: Write>(
    stdout: &mut W,
    scene: &Scene,
    outcome: &Outcome,
    (width, height): (u16, u16),
    color_enabled: bool,
) -> io::Result<()> {
    queue!(
        stdout,
        BeginSynchronizedUpdate,
        MoveTo(0, 0),
        Clear(ClearType::All)
    )?;
    if width < 60 || height < 20 {
        write_centered(
            stdout,
            height / 2,
            width,
            "Resize to at least 60 x 20",
            color_enabled.then_some(Color::Yellow),
        )?;
        queue!(stdout, EndSynchronizedUpdate)?;
        return stdout.flush();
    }

    write_centered(
        stdout,
        1,
        width,
        &format!("+-- GAMES // {} --+", scene.title),
        color_enabled.then_some(Color::Cyan),
    )?;
    let available = usize::from(height.saturating_sub(7));
    let start = 3 + (available.saturating_sub(scene.lines.len().min(available)) / 2) as u16;
    for (offset, line) in scene.lines.iter().take(available).enumerate() {
        write_centered(
            stdout,
            start + offset as u16,
            width,
            line,
            color_enabled.then_some(Color::White),
        )?;
    }

    let (message, status_color) = match outcome {
        Outcome::Playing => (&scene.status, Color::Grey),
        Outcome::Won(message) => (message, Color::Green),
        Outcome::Lost(message) => (message, Color::Red),
    };
    write_centered(
        stdout,
        height - 3,
        width,
        message,
        color_enabled.then_some(status_color),
    )?;
    write_centered(
        stdout,
        height - 2,
        width,
        scene.help,
        color_enabled.then_some(Color::Grey),
    )?;
    queue!(stdout, EndSynchronizedUpdate)?;
    stdout.flush()
}

fn write_centered<W: Write>(
    stdout: &mut W,
    row: u16,
    width: u16,
    value: &str,
    color: Option<Color>,
) -> io::Result<()> {
    let value = value.chars().take(width as usize).collect::<String>();
    let column = width.saturating_sub(value.chars().count() as u16) / 2;
    queue!(stdout, MoveTo(column, row))?;
    if let Some(color) = color {
        queue!(stdout, SetForegroundColor(color))?;
    }
    queue!(stdout, Print(value))?;
    if color.is_some() {
        queue!(stdout, ResetColor)?;
    }
    Ok(())
}

struct TerminalSession;

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen, Hide, DisableLineWrap) {
            restore_terminal();
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    let _ = execute!(
        io::stdout(),
        EndSynchronizedUpdate,
        ResetColor,
        Show,
        EnableLineWrap,
        LeaveAlternateScreen
    );
    let _ = disable_raw_mode();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_and_mode_registry_are_deterministic() {
        let mut left = Rng::new(7);
        let mut right = Rng::new(7);
        assert_eq!(left.next(), right.next());
        assert_eq!(MODES.len(), 11);
        for mode in MODES {
            let game = (mode.factory)(1);
            let replay = (mode.factory)(1);
            let scene = game.scene(80, 24);
            let replay_scene = replay.scene(80, 24);
            assert_eq!(scene.title, mode.title);
            assert!(!scene.lines.is_empty());
            assert_eq!(scene.status, replay_scene.status);
            assert_eq!(scene.lines, replay_scene.lines);
            assert_eq!(game.outcome(), &Outcome::Playing);
        }
    }

    #[test]
    fn renderer_handles_plain_and_small_terminals() {
        let scene = menu_scene(0);
        let mut output = Vec::new();
        draw(&mut output, &scene, &Outcome::Playing, (80, 24), false).expect("draw menu");
        let output = String::from_utf8(output).expect("ANSI is UTF-8");
        assert!(output.contains("GAMES // ARCADE"));
        assert!(!output.contains("\x1b[38;"));

        let mut small = Vec::new();
        draw(&mut small, &scene, &Outcome::Playing, (40, 10), false).expect("draw resize state");
        assert!(String::from_utf8_lossy(&small).contains("Resize to at least 60 x 20"));
    }

    #[test]
    fn parses_seed_and_rejects_unknown_mode() {
        assert_eq!(parse_seed(vec!["--seed".into(), "42".into()]), Ok(Some(42)));
        assert!(parse_seed(vec!["--seed".into(), "no".into()]).is_err());
        assert!(find_mode("missing").is_err());
        assert!(restart_key(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            false
        ));
        assert!(!restart_key(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            true
        ));
        assert!(restart_key(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            true
        ));
        assert!(!quit_key(
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            true
        ));
    }
}
