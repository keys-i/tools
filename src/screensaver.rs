use std::fs;
use std::io::{self, BufWriter, IsTerminal as _, Read as _, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::queue;
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, size};

use crate::games::Rng;
use crate::lazybox::{clean, command_output};
use crate::terminal::Session;
use crate::{VERSION, json_string};

const HELP: &str = "Native terminal scenes and local media playback\n\nUsage:\n  screensaver\n  screensaver open [mix|rain|pipes|pond|weather|git|gif] [INPUT] [OPTIONS]\n  screensaver snapshot [mix|rain|pipes|pond|weather|git|gif] [INPUT] [OPTIONS]\n\nInputs:\n  git [PATH]    Read at most 64 commits from a local Git repository\n  gif FILE      Decode a bounded local GIF87a or GIF89a file\n\nOptions:\n  --seed NUMBER       Reproduce generated scenes\n  --frame NUMBER      Snapshot a later animation frame (0..10000)\n  --condition NAME    clear, rain, snow, storm, or fog\n  --night             Render the weather scene at night\n  --fps NUMBER        Interactive animation rate (1..60)\n  --plain             Stable snapshot text without decoration\n  --json              Machine-readable snapshot and rendered lines\n  -h, --help          Show this help\n  -V, --version       Show the version\n\nControls: q or Esc quits; Space pauses; r resets; n changes the mixed scene; Left/Right moves through Git or GIF frames; Up/Down changes speed.";

const SNAPSHOT_WIDTH: u16 = 80;
const SNAPSHOT_HEIGHT: u16 = 24;
const MAX_GIF_BYTES: u64 = 64 * 1024 * 1024;
const MAX_GIF_PIXELS: usize = 16 * 1024 * 1024;
const MAX_GIF_DECODED_PIXELS: usize = 128 * 1024 * 1024;
const MAX_GIF_FRAMES: usize = 512;
const MAX_GIF_RENDER_WIDTH: usize = 240;
const MAX_GIF_RENDER_HEIGHT: usize = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Open,
    Snapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Plain,
    Json,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Mix,
    Rain,
    Pipes,
    Pond,
    Weather,
    Git,
    Gif,
}

impl Mode {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "mix" => Ok(Self::Mix),
            "rain" => Ok(Self::Rain),
            "pipes" => Ok(Self::Pipes),
            "pond" => Ok(Self::Pond),
            "weather" => Ok(Self::Weather),
            "git" => Ok(Self::Git),
            "gif" => Ok(Self::Gif),
            _ => Err(format!(
                "unknown scene {value:?}; expected mix, rain, pipes, pond, weather, git, or gif"
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Mix => "mix",
            Self::Rain => "rain",
            Self::Pipes => "pipes",
            Self::Pond => "pond",
            Self::Weather => "weather",
            Self::Git => "git",
            Self::Gif => "gif",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Mix => "MIX",
            Self::Rain => "RAIN",
            Self::Pipes => "PIPES",
            Self::Pond => "POND",
            Self::Weather => "WEATHER",
            Self::Git => "GIT",
            Self::Gif => "GIF",
        }
    }
}

const AMBIENT: [Mode; 4] = [Mode::Rain, Mode::Pipes, Mode::Pond, Mode::Weather];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Condition {
    Clear,
    Rain,
    Snow,
    Storm,
    Fog,
}

impl Condition {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "clear" => Ok(Self::Clear),
            "rain" => Ok(Self::Rain),
            "snow" => Ok(Self::Snow),
            "storm" => Ok(Self::Storm),
            "fog" => Ok(Self::Fog),
            _ => Err(format!(
                "unknown condition {value:?}; expected clear, rain, snow, storm, or fog"
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Rain => "rain",
            Self::Snow => "snow",
            Self::Storm => "storm",
            Self::Fog => "fog",
        }
    }
}

#[derive(Clone, Debug)]
struct Options {
    action: Action,
    requested: Mode,
    input: Option<PathBuf>,
    seed: u64,
    frame: u64,
    format: Format,
    fps: Option<u64>,
    condition: Condition,
    night: bool,
}

pub fn run(arguments: Vec<String>) -> Result<i32, String> {
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "-h" | "--help"))
    {
        println!("{HELP}");
        return Ok(0);
    }
    if arguments.len() == 1 && matches!(arguments[0].as_str(), "-V" | "--version") {
        println!("screensaver {VERSION}");
        return Ok(0);
    }

    let terminal = io::stdin().is_terminal() && io::stdout().is_terminal();
    let options = parse_options(arguments, terminal)?;
    match options.action {
        Action::Open => open(options),
        Action::Snapshot => snapshot(options),
    }
}

fn parse_options(arguments: Vec<String>, terminal: bool) -> Result<Options, String> {
    let mut positionals = Vec::new();
    let mut seed = None;
    let mut frame = None;
    let mut fps = None;
    let mut condition = None;
    let mut night = false;
    let mut plain = false;
    let mut json = false;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--seed" => {
                let value = arguments.next().ok_or("--seed needs a number")?;
                seed = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| "--seed needs an unsigned integer")?,
                );
            }
            "--frame" => {
                let value = arguments.next().ok_or("--frame needs a number")?;
                let value = value
                    .parse::<u64>()
                    .map_err(|_| "--frame needs an unsigned integer")?;
                if value > 10_000 {
                    return Err("--frame must be between 0 and 10000".into());
                }
                frame = Some(value);
            }
            "--fps" => {
                let value = arguments.next().ok_or("--fps needs a number")?;
                let value = value
                    .parse::<u64>()
                    .map_err(|_| "--fps needs an integer from 1 to 60")?;
                if !(1..=60).contains(&value) {
                    return Err("--fps must be between 1 and 60".into());
                }
                fps = Some(value);
            }
            "--condition" => {
                condition = Some(Condition::parse(
                    &arguments.next().ok_or("--condition needs a name")?,
                )?);
            }
            "--night" => night = true,
            "--plain" => plain = true,
            "--json" => json = true,
            "-V" | "--version" => {
                return Err("--version cannot be combined with other arguments".into());
            }
            value if value.starts_with('-') => {
                return Err(format!(
                    "unknown option {value:?}; try 'screensaver --help'"
                ));
            }
            value => positionals.push(value.to_string()),
        }
    }
    if plain && json {
        return Err("--plain and --json cannot be used together".into());
    }

    let mut positionals = positionals.into_iter();
    let first = positionals.next();
    let (action, mode_value) = match first.as_deref() {
        Some("open") => (Action::Open, positionals.next()),
        Some("snapshot") => (Action::Snapshot, positionals.next()),
        Some(value) => (Action::Open, Some(value.to_string())),
        None if terminal => (Action::Open, None),
        None => (Action::Snapshot, None),
    };
    let requested = Mode::parse(mode_value.as_deref().unwrap_or("mix"))?;
    let input = positionals.next().map(PathBuf::from);
    if let Some(extra) = positionals.next() {
        return Err(format!("unexpected argument {extra:?}"));
    }

    if action == Action::Open && !terminal {
        return Err("interactive scenes need a terminal; use 'screensaver snapshot'".into());
    }
    if action == Action::Open && (plain || json || frame.is_some()) {
        return Err(
            "snapshot options --plain, --json, and --frame cannot be used with open".into(),
        );
    }
    if action == Action::Snapshot && fps.is_some() {
        return Err("--fps applies only to interactive scenes".into());
    }
    if requested == Mode::Gif && input.is_none() {
        return Err("gif needs a local GIF file".into());
    }
    if !matches!(requested, Mode::Git | Mode::Gif) && input.is_some() {
        return Err(format!(
            "{} does not accept an input path",
            requested.name()
        ));
    }
    if !matches!(requested, Mode::Weather | Mode::Mix) && (condition.is_some() || night) {
        return Err("--condition and --night apply only to weather or mix".into());
    }

    Ok(Options {
        action,
        requested,
        input,
        seed: seed.unwrap_or_else(|| {
            if action == Action::Snapshot {
                1
            } else {
                system_seed()
            }
        }),
        frame: frame.unwrap_or(24),
        format: if json { Format::Json } else { Format::Plain },
        fps,
        condition: condition.unwrap_or(Condition::Clear),
        night,
    })
}

fn system_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |duration| duration.as_nanos() as u64)
}

fn snapshot(options: Options) -> Result<i32, String> {
    let active = if options.requested == Mode::Mix {
        AMBIENT[(options.frame as usize / 240) % AMBIENT.len()]
    } else {
        options.requested
    };
    let mut scene = Scene::load(active, &options)?;
    let mut screen = Screen::new(SNAPSHOT_WIDTH, SNAPSHOT_HEIGHT);
    let area = scene_area(SNAPSHOT_WIDTH, SNAPSHOT_HEIGHT);
    scene.seek(options.frame, area);
    draw(&mut screen, &scene, options.frame, false, 0);
    let lines = screen.lines();
    match options.format {
        Format::Plain => {
            for line in &lines {
                println!("{line}");
            }
        }
        Format::Json => {
            let rendered = lines
                .iter()
                .map(|line| json_string(line))
                .collect::<Vec<_>>()
                .join(",");
            println!(
                "{{\"mode\":{},\"seed\":{},\"frame\":{},\"width\":{},\"height\":{},\"detail\":{},\"lines\":[{}]}}",
                json_string(active.name()),
                options.seed,
                options.frame,
                SNAPSHOT_WIDTH,
                SNAPSHOT_HEIGHT,
                json_string(&scene.detail()),
                rendered
            );
        }
    }
    Ok(0)
}

fn open(options: Options) -> Result<i32, String> {
    let mut active_index = 0;
    let mut active = if options.requested == Mode::Mix {
        AMBIENT[active_index]
    } else {
        options.requested
    };
    let mut scene = Scene::load(active, &options)?;
    let _session = Session::enter().map_err(|error| format!("cannot prepare terminal: {error}"))?;
    let mut stdout = BufWriter::new(io::stdout());
    let mut previous = Vec::new();
    let mut previous_size = (0, 0);
    let mut tick = 0_u64;
    let mut paused = false;
    let mut fps = options.fps.unwrap_or(20);
    let color = std::env::var_os("NO_COLOR").is_none();

    loop {
        let dimensions = size().unwrap_or((80, 24));
        let mut screen = Screen::new(dimensions.0.min(240), dimensions.1.min(80));
        draw(&mut screen, &scene, tick, paused, fps);
        present(
            &mut stdout,
            &screen,
            &mut previous,
            &mut previous_size,
            color,
        )
        .map_err(|error| format!("cannot draw terminal: {error}"))?;

        let wait = Duration::from_millis(1000 / fps.max(1));
        if event::poll(wait).map_err(|error| format!("cannot read terminal: {error}"))? {
            let event = event::read().map_err(|error| format!("cannot read terminal: {error}"))?;
            if let Event::Key(key) = event {
                if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    continue;
                }
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('q' | 'Q'))
                    || key.modifiers.contains(KeyModifiers::CONTROL)
                        && matches!(key.code, KeyCode::Char('c' | 'C'))
                {
                    break;
                }
                match key.code {
                    KeyCode::Char(' ') => paused = !paused,
                    KeyCode::Up => fps = (fps + 1).min(60),
                    KeyCode::Down => fps = fps.saturating_sub(1).max(1),
                    KeyCode::Char('r' | 'R') => {
                        scene = Scene::load(active, &options)?;
                        tick = 0;
                    }
                    KeyCode::Char('n' | 'N') if options.requested == Mode::Mix => {
                        active_index = (active_index + 1) % AMBIENT.len();
                        active = AMBIENT[active_index];
                        scene = Scene::load(active, &options)?;
                        tick = 0;
                    }
                    KeyCode::Left => scene.previous(),
                    KeyCode::Right => scene.next(),
                    _ => {}
                }
            }
        }

        if !paused {
            tick = tick.saturating_add(1);
            let area = scene_area(screen.width, screen.height);
            scene.advance(area, 1000 / fps.max(1), options.fps);
            if options.requested == Mode::Mix && tick >= fps.saturating_mul(15) {
                active_index = (active_index + 1) % AMBIENT.len();
                active = AMBIENT[active_index];
                scene = Scene::load(active, &options)?;
                tick = 0;
            }
        }
    }
    Ok(0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cell {
    character: char,
    foreground: Option<Color>,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            character: ' ',
            foreground: None,
        }
    }
}

struct Screen {
    width: u16,
    height: u16,
    cells: Vec<Cell>,
}

impl Screen {
    fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); usize::from(width) * usize::from(height)],
        }
    }

    fn set(&mut self, x: u16, y: u16, character: char, foreground: Option<Color>) {
        if x < self.width && y < self.height {
            self.cells[usize::from(y) * usize::from(self.width) + usize::from(x)] = Cell {
                character,
                foreground,
            };
        }
    }

    fn text(&mut self, x: u16, y: u16, value: &str, color: Option<Color>) {
        for (offset, character) in value.chars().enumerate() {
            let Ok(offset) = u16::try_from(offset) else {
                break;
            };
            self.set(x.saturating_add(offset), y, character, color);
        }
    }

    fn centered(&mut self, y: u16, value: &str, color: Option<Color>) {
        let value = value
            .chars()
            .take(usize::from(self.width))
            .collect::<String>();
        let width = value.chars().count() as u16;
        self.text(self.width.saturating_sub(width) / 2, y, &value, color);
    }

    fn lines(&self) -> Vec<String> {
        let mut lines = self
            .cells
            .chunks(usize::from(self.width).max(1))
            .map(|row| {
                row.iter()
                    .map(|cell| cell.character)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines
    }
}

#[derive(Clone, Copy)]
struct Rect {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

fn scene_area(width: u16, height: u16) -> Rect {
    Rect {
        x: 1,
        y: 4,
        width: width.saturating_sub(2),
        height: height.saturating_sub(8),
    }
}

enum Scene {
    Rain {
        seed: u64,
    },
    Pipes(Pipes),
    Pond {
        seed: u64,
    },
    Weather {
        seed: u64,
        condition: Condition,
        night: bool,
    },
    Git(GitScene),
    Gif(GifScene),
}

impl Scene {
    fn load(mode: Mode, options: &Options) -> Result<Self, String> {
        match mode {
            Mode::Mix | Mode::Rain => Ok(Self::Rain { seed: options.seed }),
            Mode::Pipes => Ok(Self::Pipes(Pipes::new(options.seed))),
            Mode::Pond => Ok(Self::Pond { seed: options.seed }),
            Mode::Weather => Ok(Self::Weather {
                seed: options.seed,
                condition: options.condition,
                night: options.night,
            }),
            Mode::Git => Ok(Self::Git(GitScene::load(
                options.input.as_deref().unwrap_or_else(|| Path::new(".")),
            )?)),
            Mode::Gif => Ok(Self::Gif(GifScene::load(
                options
                    .input
                    .as_deref()
                    .ok_or("gif needs a local GIF file")?,
            )?)),
        }
    }

    fn mode(&self) -> Mode {
        match self {
            Self::Rain { .. } => Mode::Rain,
            Self::Pipes(_) => Mode::Pipes,
            Self::Pond { .. } => Mode::Pond,
            Self::Weather { .. } => Mode::Weather,
            Self::Git(_) => Mode::Git,
            Self::Gif(_) => Mode::Gif,
        }
    }

    fn detail(&self) -> String {
        match self {
            Self::Rain { .. } => "bounded digital rain".into(),
            Self::Pipes(pipes) => format!("{} pipe cells", pipes.drawn),
            Self::Pond { .. } => "lilies, ripples, and three wandering frogs".into(),
            Self::Weather {
                condition, night, ..
            } => format!(
                "offline {} {} scene",
                condition.name(),
                if *night { "night" } else { "day" }
            ),
            Self::Git(git) => format!("{} local commits from {}", git.commits.len(), git.path),
            Self::Gif(gif) => format!(
                "{}x{} GIF, {} frame{}",
                gif.image.width,
                gif.image.height,
                gif.image.frames.len(),
                if gif.image.frames.len() == 1 { "" } else { "s" }
            ),
        }
    }

    fn seek(&mut self, frame: u64, area: Rect) {
        match self {
            Self::Pipes(pipes) => {
                pipes.ensure(area.width, area.height);
                for _ in 0..frame {
                    pipes.step();
                }
            }
            Self::Git(git) => git.seek(frame),
            Self::Gif(gif) => gif.seek(frame),
            _ => {}
        }
    }

    fn advance(&mut self, area: Rect, elapsed_ms: u64, fps_override: Option<u64>) {
        match self {
            Self::Pipes(pipes) => {
                pipes.ensure(area.width, area.height);
                pipes.step();
            }
            Self::Git(git) => git.advance(),
            Self::Gif(gif) => gif.advance(elapsed_ms, fps_override),
            _ => {}
        }
    }

    fn previous(&mut self) {
        match self {
            Self::Git(git) => git.previous(),
            Self::Gif(gif) => gif.previous(),
            _ => {}
        }
    }

    fn next(&mut self) {
        match self {
            Self::Git(git) => git.next(),
            Self::Gif(gif) => gif.next(),
            _ => {}
        }
    }
}

fn draw(screen: &mut Screen, scene: &Scene, tick: u64, paused: bool, fps: u64) {
    if screen.width < 50 || screen.height < 16 {
        screen.centered(
            screen.height / 2,
            "Resize to at least 50 x 16",
            Some(Color::Yellow),
        );
        return;
    }
    screen.centered(
        1,
        &format!("+-- SCREENSAVER // {} --+", scene.mode().title()),
        Some(Color::Cyan),
    );
    screen.centered(2, &scene.detail(), Some(Color::DarkGrey));
    let area = scene_area(screen.width, screen.height);
    match scene {
        Scene::Rain { seed } => draw_rain(screen, area, *seed, tick),
        Scene::Pipes(pipes) => pipes.draw(screen, area),
        Scene::Pond { seed } => draw_pond(screen, area, *seed, tick),
        Scene::Weather {
            seed,
            condition,
            night,
        } => draw_weather(screen, area, *seed, tick, *condition, *night),
        Scene::Git(git) => git.draw(screen, area),
        Scene::Gif(gif) => gif.draw(screen, area),
    }
    screen.centered(
        screen.height - 2,
        if paused {
            "PAUSED  Space resume  r reset  q quit"
        } else {
            "Space pause  r reset  n next mix  arrows navigate/speed  q quit"
        },
        Some(Color::DarkGrey),
    );
    screen.text(
        1,
        screen.height - 1,
        &if fps == 0 {
            format!("frame {tick}  static snapshot")
        } else {
            format!("frame {tick}  {fps} fps")
        },
        Some(Color::DarkGrey),
    );
}

fn present<W: Write>(
    output: &mut W,
    screen: &Screen,
    previous: &mut Vec<Cell>,
    previous_size: &mut (u16, u16),
    color: bool,
) -> io::Result<()> {
    if *previous_size != (screen.width, screen.height) {
        previous.clear();
        previous.resize(screen.cells.len(), Cell::default());
        *previous_size = (screen.width, screen.height);
        queue!(output, Clear(ClearType::All))?;
    }
    queue!(output, BeginSynchronizedUpdate)?;
    let width = usize::from(screen.width);
    let mut foreground = None;
    for y in 0..usize::from(screen.height) {
        let row = y * width;
        let Some(first) = (0..width).find(|x| screen.cells[row + x] != previous[row + x]) else {
            continue;
        };
        let last = (first..width)
            .rfind(|x| screen.cells[row + x] != previous[row + x])
            .unwrap_or(first);
        queue!(output, MoveTo(first as u16, y as u16))?;
        for x in first..=last {
            let cell = screen.cells[row + x];
            if color && cell.foreground != foreground {
                queue!(
                    output,
                    SetForegroundColor(cell.foreground.unwrap_or(Color::Reset))
                )?;
                foreground = cell.foreground;
            }
            queue!(output, Print(cell.character))?;
        }
    }
    queue!(output, ResetColor, EndSynchronizedUpdate)?;
    output.flush()?;
    previous.copy_from_slice(&screen.cells);
    Ok(())
}

fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn fit(value: &str, width: usize) -> String {
    value.chars().take(width).collect()
}

fn draw_rain(screen: &mut Screen, area: Rect, seed: u64, tick: u64) {
    const GLYPHS: &[u8] = b"01<>[]{}:/\\*+";
    if area.width == 0 || area.height == 0 {
        return;
    }
    let cycle = u64::from(area.height) + 18;
    for x in (0..area.width).step_by(2) {
        let column = mix64(seed ^ u64::from(x).wrapping_mul(0x9e37_79b9));
        let speed = 1 + column % 3;
        let head = ((tick / speed + column / 17) % cycle) as i32 - 9;
        let length = 5 + (column as u16 % area.height.clamp(1, 14));
        for offset in 0..length {
            let y = head - i32::from(offset);
            if !(0..i32::from(area.height)).contains(&y) {
                continue;
            }
            let value = mix64(seed ^ (u64::from(x) << 32) ^ y as u64 ^ tick.wrapping_div(5));
            let character = GLYPHS[value as usize % GLYPHS.len()] as char;
            let color = match offset {
                0 => Color::White,
                1..=2 => Color::Green,
                3..=6 => Color::DarkGreen,
                _ => Color::Rgb {
                    r: 20,
                    g: 70,
                    b: 40,
                },
            };
            screen.set(area.x + x, area.y + y as u16, character, Some(color));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy)]
struct PipeCell {
    character: char,
    color: Color,
}

impl Default for PipeCell {
    fn default() -> Self {
        Self {
            character: ' ',
            color: Color::Reset,
        }
    }
}

struct Pipes {
    width: u16,
    height: u16,
    cells: Vec<PipeCell>,
    x: u16,
    y: u16,
    direction: Direction,
    remaining: u64,
    drawn: u64,
    color: Color,
    rng: Rng,
}

impl Pipes {
    fn new(seed: u64) -> Self {
        Self {
            width: 0,
            height: 0,
            cells: Vec::new(),
            x: 0,
            y: 0,
            direction: Direction::Right,
            remaining: 0,
            drawn: 0,
            color: Color::Cyan,
            rng: Rng::new(seed),
        }
    }

    fn ensure(&mut self, width: u16, height: u16) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.cells = vec![PipeCell::default(); usize::from(width) * usize::from(height)];
        self.remaining = 0;
        self.drawn = 0;
    }

    fn step(&mut self) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        if self.drawn >= self.cells.len() as u64 * 3 / 4 {
            self.cells.fill(PipeCell::default());
            self.drawn = 0;
            self.remaining = 0;
        }
        if self.remaining == 0 {
            self.x = self.rng.index(usize::from(self.width)) as u16;
            self.y = self.rng.index(usize::from(self.height)) as u16;
            self.direction = match self.rng.index(4) {
                0 => Direction::Up,
                1 => Direction::Down,
                2 => Direction::Left,
                _ => Direction::Right,
            };
            self.remaining = 8 + self.rng.next() % u64::from(self.width + self.height).max(9);
            self.color = [
                Color::Cyan,
                Color::Green,
                Color::Magenta,
                Color::Yellow,
                Color::Blue,
                Color::Red,
            ][self.rng.index(6)];
        }

        let previous = self.direction;
        if self.rng.chance(1, 5) {
            self.direction = match previous {
                Direction::Up | Direction::Down if self.rng.chance(1, 2) => Direction::Left,
                Direction::Up | Direction::Down => Direction::Right,
                Direction::Left | Direction::Right if self.rng.chance(1, 2) => Direction::Up,
                Direction::Left | Direction::Right => Direction::Down,
            };
        }
        let index = usize::from(self.y) * usize::from(self.width) + usize::from(self.x);
        self.cells[index] = PipeCell {
            character: pipe_character(previous, self.direction),
            color: self.color,
        };
        match self.direction {
            Direction::Up => self.y = self.y.checked_sub(1).unwrap_or(self.height - 1),
            Direction::Down => self.y = (self.y + 1) % self.height,
            Direction::Left => self.x = self.x.checked_sub(1).unwrap_or(self.width - 1),
            Direction::Right => self.x = (self.x + 1) % self.width,
        }
        self.remaining -= 1;
        self.drawn += 1;
    }

    fn draw(&self, screen: &mut Screen, area: Rect) {
        if self.width != area.width || self.height != area.height {
            return;
        }
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = self.cells[usize::from(y) * usize::from(self.width) + usize::from(x)];
                if cell.character != ' ' {
                    screen.set(area.x + x, area.y + y, cell.character, Some(cell.color));
                }
            }
        }
        screen.set(area.x + self.x, area.y + self.y, '◆', Some(Color::White));
    }
}

fn pipe_character(previous: Direction, next: Direction) -> char {
    use Direction::{Down, Left, Right, Up};
    match (previous, next) {
        (Up | Down, Up | Down) => '│',
        (Left | Right, Left | Right) => '─',
        (Up, Right) | (Left, Down) => '┌',
        (Up, Left) | (Right, Down) => '┐',
        (Down, Right) | (Left, Up) => '└',
        (Down, Left) | (Right, Up) => '┘',
    }
}

fn draw_pond(screen: &mut Screen, area: Rect, seed: u64, tick: u64) {
    if area.width < 12 || area.height < 6 {
        return;
    }
    for y in 0..area.height {
        for x in 0..area.width {
            let drift = (u64::from(x) + tick / 3 + u64::from(y) * 7) % 19;
            if drift == 0 || drift == 1 {
                screen.set(area.x + x, area.y + y, '~', Some(Color::DarkCyan));
            }
        }
    }

    let pad_count = (usize::from(area.width) * usize::from(area.height) / 220).clamp(2, 10);
    for pad in 0..pad_count {
        let value = mix64(seed ^ (pad as u64).wrapping_mul(0x517c_c1b7));
        let x = 1 + value as u16 % area.width.saturating_sub(7).max(1);
        let y = 1 + (value >> 24) as u16 % area.height.saturating_sub(2).max(1);
        screen.text(area.x + x, area.y + y, "(_==_)", Some(Color::Green));
        if value.is_multiple_of(3) {
            screen.set(area.x + x + 3, area.y, '*', Some(Color::Magenta));
        }
    }

    for frog in 0..3_u64 {
        let value = mix64(seed ^ frog.wrapping_mul(0x9e37_79b9));
        let range = area.width.saturating_sub(8).max(1);
        let period = u64::from(range).saturating_mul(2).max(2);
        let phase = (tick / (3 + frog) + value) % period;
        let x = if phase < u64::from(range) {
            phase as u16
        } else {
            (period - phase) as u16
        };
        let base_y = 1 + ((value >> 32) as u16 % area.height.saturating_sub(4).max(1));
        let hop = (tick + frog * 11).is_multiple_of(31) as u16;
        let y = base_y.saturating_sub(hop);
        screen.text(area.x + x, area.y + y, " @  @ ", Some(Color::Yellow));
        screen.text(area.x + x, area.y + y + 1, "( -- )", Some(Color::Green));
        screen.text(area.x + x, area.y + y + 2, "/_||_\\", Some(Color::Green));
        if (tick + frog * 7) % 23 < 4 {
            screen.text(
                area.x + x.saturating_sub(1),
                area.y + (y + 3).min(area.height - 1),
                "~     ~",
                Some(Color::Cyan),
            );
        }
    }
}

fn draw_weather(
    screen: &mut Screen,
    area: Rect,
    seed: u64,
    tick: u64,
    condition: Condition,
    night: bool,
) {
    if area.width < 24 || area.height < 8 {
        return;
    }
    if night {
        for y in 0..area.height.saturating_sub(5) {
            for x in 0..area.width {
                if mix64(seed ^ (u64::from(x) << 24) ^ u64::from(y)).is_multiple_of(97) {
                    screen.set(area.x + x, area.y + y, '.', Some(Color::DarkGrey));
                }
            }
        }
        screen.text(
            area.x + area.width - 9,
            area.y + 1,
            "(  )",
            Some(Color::White),
        );
        screen.text(area.x + area.width - 8, area.y, "_)", Some(Color::White));
    } else {
        let sun_x = area.x + area.width.saturating_sub(9);
        screen.text(sun_x, area.y, "\\ | /", Some(Color::Yellow));
        screen.text(sun_x, area.y + 1, "--O--", Some(Color::Yellow));
        screen.text(sun_x, area.y + 2, "/ | \\", Some(Color::Yellow));
    }

    let cloud_shift = (tick / 8) as u16 % area.width.max(1);
    for cloud in 0..3_u16 {
        let x = (cloud * (area.width / 3).max(1) + cloud_shift) % area.width;
        let y = 2 + cloud % 3;
        draw_cloud_shape(screen, area, x, y, Some(Color::Grey));
    }

    let ground = area.y + area.height - 3;
    for x in 0..area.width {
        screen.set(area.x + x, ground, '_', Some(Color::DarkGreen));
    }
    let house_x = area.x + area.width / 2 - 5;
    screen.text(house_x, ground - 3, "   /\\", Some(Color::Yellow));
    screen.text(house_x, ground - 2, "  /__\\", Some(Color::Yellow));
    screen.text(house_x, ground - 1, "  |[]|", Some(Color::DarkYellow));

    match condition {
        Condition::Clear => {}
        Condition::Fog => {
            for y in (area.y + 4..ground).step_by(2) {
                let shift = ((tick / 4 + u64::from(y)) % 8) as u16;
                for x in (shift..area.width).step_by(9) {
                    screen.text(area.x + x, y, "====", Some(Color::Grey));
                }
            }
        }
        Condition::Rain | Condition::Storm => {
            for y in area.y + 5..ground {
                for x in 0..area.width {
                    let value = mix64(seed ^ (u64::from(x) << 32) ^ u64::from(y) ^ (tick / 2));
                    if value.is_multiple_of(if condition == Condition::Storm {
                        11
                    } else {
                        19
                    }) {
                        screen.set(area.x + x, y, '|', Some(Color::Blue));
                    }
                }
            }
            if condition == Condition::Storm && tick % 47 < 3 {
                screen.text(area.x + 4, area.y + 5, "\\_/\\__", Some(Color::Yellow));
                screen.text(area.x + 7, area.y + 6, "\\", Some(Color::Yellow));
            }
        }
        Condition::Snow => {
            for y in area.y + 4..ground {
                for x in 0..area.width {
                    let value = mix64(seed ^ (u64::from(x) << 32) ^ u64::from(y) ^ (tick / 3));
                    if value.is_multiple_of(23) {
                        screen.set(
                            area.x + x,
                            y,
                            if value & 1 == 0 { '*' } else { '·' },
                            Some(Color::White),
                        );
                    }
                }
            }
        }
    }
}

fn draw_cloud_shape(screen: &mut Screen, area: Rect, x: u16, y: u16, color: Option<Color>) {
    let first = ".--.";
    let second = "(____)";
    let x = x.min(area.width.saturating_sub(1));
    for (offset, character) in first.chars().enumerate() {
        screen.set(
            area.x + (x + offset as u16) % area.width,
            area.y + y.min(area.height - 1),
            character,
            color,
        );
    }
    for (offset, character) in second.chars().enumerate() {
        screen.set(
            area.x + (x + offset as u16) % area.width,
            area.y + (y + 1).min(area.height - 1),
            character,
            color,
        );
    }
}

struct GitCommit {
    hash: String,
    author: String,
    date: String,
    subject: String,
}

struct GitScene {
    path: String,
    commits: Vec<GitCommit>,
    index: usize,
    typed: usize,
    idle: u64,
}

impl GitScene {
    fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Err(format!("Git path does not exist: {}", path.display()));
        }
        let path = path
            .canonicalize()
            .map_err(|error| format!("cannot resolve Git path {}: {error}", path.display()))?;
        let path_arg = path
            .to_str()
            .ok_or("Git path must be valid UTF-8")?
            .to_string();
        let output = command_output(
            "git",
            &[
                "--no-pager",
                "-C",
                &path_arg,
                "log",
                "--max-count=64",
                "--date=short",
                "--pretty=format:%h%x09%an%x09%ad%x09%s",
            ],
            false,
        )?;
        let commits = output
            .lines()
            .filter_map(|line| {
                let mut fields = line.splitn(4, '\t');
                let hash = clean(fields.next()?);
                let author = clean(fields.next()?);
                let date = clean(fields.next()?);
                let subject = clean(fields.next()?);
                (!hash.is_empty()).then_some(GitCommit {
                    hash,
                    author,
                    date,
                    subject,
                })
            })
            .collect::<Vec<_>>();
        if commits.is_empty() {
            return Err(format!("no commits found in {}", path.display()));
        }
        Ok(Self {
            path: clean(&path.display().to_string()),
            commits,
            index: 0,
            typed: 0,
            idle: 0,
        })
    }

    fn advance(&mut self) {
        let length = self.commits[self.index].subject.chars().count();
        if self.typed < length {
            self.typed += 1;
            return;
        }
        self.idle += 1;
        if self.idle >= 24 {
            self.next();
        }
    }

    fn seek(&mut self, frame: u64) {
        self.index = (frame as usize / 80) % self.commits.len();
        let length = self.commits[self.index].subject.chars().count();
        self.typed = (frame as usize % 80).min(length);
        self.idle = 0;
    }

    fn previous(&mut self) {
        self.index = self.index.checked_sub(1).unwrap_or(self.commits.len() - 1);
        self.typed = 0;
        self.idle = 0;
    }

    fn next(&mut self) {
        self.index = (self.index + 1) % self.commits.len();
        self.typed = 0;
        self.idle = 0;
    }

    fn draw(&self, screen: &mut Screen, area: Rect) {
        let left_width = (area.width / 3).clamp(18, 30);
        let divider = area.x + left_width;
        for y in 0..area.height {
            screen.set(divider, area.y + y, '│', Some(Color::DarkGrey));
        }
        let visible = usize::from(area.height).saturating_sub(2).max(1);
        let start = self.index.saturating_sub(visible / 2);
        for (offset, commit) in self.commits.iter().skip(start).take(visible).enumerate() {
            let index = start + offset;
            let marker = if index == self.index { '>' } else { ' ' };
            screen.text(
                area.x,
                area.y + offset as u16,
                &format!("{marker} {} {}", commit.hash, fit(&commit.subject, 9)),
                Some(if index == self.index {
                    Color::Green
                } else {
                    Color::DarkGrey
                }),
            );
        }

        let commit = &self.commits[self.index];
        let right_x = divider + 2;
        let right_width = usize::from(area.width.saturating_sub(left_width + 3));
        screen.text(right_x, area.y, &commit.hash, Some(Color::Cyan));
        screen.text(
            right_x,
            area.y + 2,
            &fit(&commit.author, right_width),
            Some(Color::White),
        );
        screen.text(right_x, area.y + 3, &commit.date, Some(Color::DarkGrey));
        let subject = commit.subject.chars().take(self.typed).collect::<String>();
        for (line, chunk) in wrap(&subject, right_width.max(1)).into_iter().enumerate() {
            if line as u16 + 6 >= area.height {
                break;
            }
            screen.text(
                right_x,
                area.y + 5 + line as u16,
                &chunk,
                Some(Color::Yellow),
            );
        }
        let cursor_x = right_x
            .saturating_add(subject.chars().count().min(right_width.saturating_sub(1)) as u16);
        screen.set(cursor_x, area.y + 5, '█', Some(Color::Green));
    }
}

fn wrap(value: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let characters = value.chars().collect::<Vec<_>>();
    characters
        .chunks(width)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

struct GifFrame {
    pixels: Vec<Rgb>,
    delay_ms: u64,
}

struct GifImage {
    width: u16,
    height: u16,
    render_width: u16,
    render_height: u16,
    frames: Vec<GifFrame>,
}

struct GifScene {
    image: GifImage,
    index: usize,
    elapsed_ms: u64,
}

impl GifScene {
    fn load(path: &Path) -> Result<Self, String> {
        let file = fs::File::open(path)
            .map_err(|error| format!("cannot open GIF {}: {error}", path.display()))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("cannot inspect GIF {}: {error}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!("GIF input is not a file: {}", path.display()));
        }
        if metadata.len() > MAX_GIF_BYTES {
            return Err(format!(
                "GIF input exceeds {} MiB",
                MAX_GIF_BYTES / 1024 / 1024
            ));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize + 1);
        file.take(MAX_GIF_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("cannot read GIF {}: {error}", path.display()))?;
        if bytes.len() as u64 > MAX_GIF_BYTES {
            return Err(format!(
                "GIF input exceeds {} MiB",
                MAX_GIF_BYTES / 1024 / 1024
            ));
        }
        Ok(Self {
            image: parse_gif(&bytes)?,
            index: 0,
            elapsed_ms: 0,
        })
    }

    fn seek(&mut self, frame: u64) {
        self.index = frame as usize % self.image.frames.len();
        self.elapsed_ms = 0;
    }

    fn advance(&mut self, elapsed_ms: u64, fps_override: Option<u64>) {
        self.elapsed_ms = self.elapsed_ms.saturating_add(elapsed_ms);
        loop {
            let delay = fps_override
                .map(|fps| 1000 / fps.max(1))
                .unwrap_or(self.image.frames[self.index].delay_ms)
                .max(1);
            if self.elapsed_ms < delay {
                break;
            }
            self.elapsed_ms -= delay;
            self.next();
        }
    }

    fn previous(&mut self) {
        self.index = self
            .index
            .checked_sub(1)
            .unwrap_or(self.image.frames.len() - 1);
        self.elapsed_ms = 0;
    }

    fn next(&mut self) {
        self.index = (self.index + 1) % self.image.frames.len();
    }

    fn draw(&self, screen: &mut Screen, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let source_width = usize::from(self.image.render_width);
        let source_height = usize::from(self.image.render_height);
        let cell_height = source_height.div_ceil(2);
        let scale = (f64::from(area.width) / source_width as f64)
            .min(f64::from(area.height) / cell_height as f64)
            .min(1.0);
        let target_width =
            ((source_width as f64 * scale).round() as usize).clamp(1, usize::from(area.width));
        let target_height =
            ((cell_height as f64 * scale).round() as usize).clamp(1, usize::from(area.height));
        let start_x = area.x + (area.width - target_width as u16) / 2;
        let start_y = area.y + (area.height - target_height as u16) / 2;
        let frame = &self.image.frames[self.index];
        const RAMP: &[u8] = b" .:-=+*#%@";
        for y in 0..target_height {
            let source_y = (y * source_height / target_height).min(source_height - 1);
            for x in 0..target_width {
                let source_x = (x * source_width / target_width).min(source_width - 1);
                let color = frame.pixels[source_y * source_width + source_x];
                let luminance = (u16::from(color.red) * 54
                    + u16::from(color.green) * 183
                    + u16::from(color.blue) * 19)
                    / 256;
                let character = RAMP[usize::from(luminance) * (RAMP.len() - 1) / 255] as char;
                screen.set(
                    start_x + x as u16,
                    start_y + y as u16,
                    character,
                    Some(Color::Rgb {
                        r: color.red,
                        g: color.green,
                        b: color.blue,
                    }),
                );
            }
        }
        screen.text(
            area.x,
            area.y + area.height - 1,
            &format!("frame {}/{}", self.index + 1, self.image.frames.len()),
            Some(Color::DarkGrey),
        );
    }
}

#[derive(Clone, Copy, Default)]
struct GifControl {
    disposal: u8,
    delay_ms: u64,
    transparent: Option<u8>,
}

struct GifCursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> GifCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn byte(&mut self) -> Result<u8, String> {
        let byte = self
            .bytes
            .get(self.position)
            .copied()
            .ok_or("truncated GIF")?;
        self.position += 1;
        Ok(byte)
    }

    fn word(&mut self) -> Result<u16, String> {
        let low = u16::from(self.byte()?);
        let high = u16::from(self.byte()?);
        Ok(low | high << 8)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or("truncated GIF")?;
        let value = &self.bytes[self.position..end];
        self.position = end;
        Ok(value)
    }

    fn sub_blocks(&mut self) -> Result<Vec<u8>, String> {
        let mut output = Vec::new();
        loop {
            let length = usize::from(self.byte()?);
            if length == 0 {
                return Ok(output);
            }
            if output.len().saturating_add(length) > MAX_GIF_BYTES as usize {
                return Err("GIF sub-block data exceeds 64 MiB".into());
            }
            output.extend_from_slice(self.take(length)?);
        }
    }
}

fn read_palette(cursor: &mut GifCursor<'_>, entries: usize) -> Result<Vec<Rgb>, String> {
    let mut palette = Vec::with_capacity(entries);
    for _ in 0..entries {
        palette.push(Rgb {
            red: cursor.byte()?,
            green: cursor.byte()?,
            blue: cursor.byte()?,
        });
    }
    Ok(palette)
}

fn parse_gif(bytes: &[u8]) -> Result<GifImage, String> {
    parse_gif_with_budget(bytes, MAX_GIF_DECODED_PIXELS)
}

fn parse_gif_with_budget(bytes: &[u8], decode_budget: usize) -> Result<GifImage, String> {
    let mut cursor = GifCursor::new(bytes);
    let header = cursor.take(6)?;
    if header != b"GIF87a" && header != b"GIF89a" {
        return Err("input is not GIF87a or GIF89a".into());
    }
    let width = cursor.word()?;
    let height = cursor.word()?;
    if width == 0 || height == 0 {
        return Err("GIF dimensions must be non-zero".into());
    }
    let canvas_pixels = usize::from(width)
        .checked_mul(usize::from(height))
        .ok_or("GIF dimensions overflow")?;
    if canvas_pixels > MAX_GIF_PIXELS {
        return Err("GIF canvas exceeds the 16-million-pixel limit".into());
    }
    let packed = cursor.byte()?;
    let background_index = usize::from(cursor.byte()?);
    cursor.byte()?;
    let global_palette = if packed & 0x80 != 0 {
        Some(read_palette(
            &mut cursor,
            1_usize << (usize::from(packed & 0x07) + 1),
        )?)
    } else {
        None
    };
    let background = global_palette
        .as_ref()
        .and_then(|palette| palette.get(background_index))
        .copied()
        .unwrap_or_default();
    let mut canvas = vec![background; canvas_pixels];
    let (render_width, render_height) = gif_render_size(width, height);
    let mut frames = Vec::new();
    let mut pending = GifControl::default();
    let mut pending_set = false;
    let mut decoded_pixels = 0_usize;
    type PreviousFrame = (GifControl, usize, usize, usize, usize, Option<Vec<Rgb>>);
    let mut previous: Option<PreviousFrame> = None;
    let mut trailer = false;

    while cursor.position < bytes.len() {
        match cursor.byte()? {
            0x3b => {
                trailer = true;
                break;
            }
            0x21 => {
                let label = cursor.byte()?;
                if label == 0xf9 {
                    if pending_set {
                        return Err("GIF has consecutive graphic-control blocks".into());
                    }
                    if cursor.byte()? != 4 {
                        return Err("invalid GIF graphic-control block size".into());
                    }
                    let control = cursor.byte()?;
                    let delay = cursor.word()?;
                    let transparent = cursor.byte()?;
                    if cursor.byte()? != 0 {
                        return Err("unterminated GIF graphic-control block".into());
                    }
                    let disposal = (control >> 2) & 0x07;
                    if disposal > 3 {
                        return Err(format!("unsupported GIF disposal method {disposal}"));
                    }
                    pending = GifControl {
                        disposal,
                        delay_ms: u64::from(delay).saturating_mul(10).max(20),
                        transparent: (control & 1 != 0).then_some(transparent),
                    };
                    pending_set = true;
                } else {
                    cursor.sub_blocks()?;
                    if label == 0x01 {
                        pending = GifControl::default();
                        pending_set = false;
                    }
                }
            }
            0x2c => {
                if frames.len() >= MAX_GIF_FRAMES {
                    return Err(format!("GIF exceeds the {MAX_GIF_FRAMES}-frame limit"));
                }
                if let Some((control, left, top, image_width, image_height, restore)) =
                    previous.take()
                {
                    match control.disposal {
                        2 => clear_rect(
                            &mut canvas,
                            usize::from(width),
                            left,
                            top,
                            image_width,
                            image_height,
                            background,
                        ),
                        3 => {
                            if let Some(saved) = restore {
                                canvas = saved;
                            }
                        }
                        _ => {}
                    }
                }

                let left = usize::from(cursor.word()?);
                let top = usize::from(cursor.word()?);
                let image_width = usize::from(cursor.word()?);
                let image_height = usize::from(cursor.word()?);
                if image_width == 0 || image_height == 0 {
                    return Err("GIF image dimensions must be non-zero".into());
                }
                if left
                    .checked_add(image_width)
                    .is_none_or(|end| end > usize::from(width))
                    || top
                        .checked_add(image_height)
                        .is_none_or(|end| end > usize::from(height))
                {
                    return Err("GIF image lies outside its logical screen".into());
                }
                let image_packed = cursor.byte()?;
                let local_palette = if image_packed & 0x80 != 0 {
                    Some(read_palette(
                        &mut cursor,
                        1_usize << (usize::from(image_packed & 0x07) + 1),
                    )?)
                } else {
                    None
                };
                let palette = local_palette
                    .as_ref()
                    .or(global_palette.as_ref())
                    .ok_or("GIF image has no color table")?;
                let minimum_code_size = cursor.byte()?;
                let compressed = cursor.sub_blocks()?;
                let image_pixels = image_width
                    .checked_mul(image_height)
                    .ok_or("GIF image dimensions overflow")?;
                decoded_pixels = decoded_pixels
                    .checked_add(image_pixels)
                    .filter(|pixels| *pixels <= decode_budget)
                    .ok_or("GIF exceeds the 128-million decoded-pixel limit")?;
                let indices = decode_lzw(&compressed, minimum_code_size, image_pixels)?;
                let restore = (pending.disposal == 3).then(|| canvas.clone());

                let mut rows = Vec::with_capacity(image_height);
                if image_packed & 0x40 != 0 {
                    for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
                        rows.extend((start..image_height).step_by(step));
                    }
                } else {
                    rows.extend(0..image_height);
                }
                for (source_row, destination_row) in rows.into_iter().enumerate() {
                    for x in 0..image_width {
                        let color_index = indices[source_row * image_width + x];
                        if pending.transparent == Some(color_index) {
                            continue;
                        }
                        let color = palette
                            .get(usize::from(color_index))
                            .copied()
                            .ok_or("GIF pixel references a missing color")?;
                        canvas[(top + destination_row) * usize::from(width) + left + x] = color;
                    }
                }

                let stored = downsample(
                    &canvas,
                    usize::from(width),
                    usize::from(height),
                    usize::from(render_width),
                    usize::from(render_height),
                );
                if stored.len() > MAX_GIF_PIXELS / (frames.len() + 1) {
                    return Err("retained GIF frames exceed the 16-million-pixel limit".into());
                }
                frames.push(GifFrame {
                    pixels: stored,
                    delay_ms: pending.delay_ms.max(20),
                });
                previous = Some((pending, left, top, image_width, image_height, restore));
                pending = GifControl::default();
                pending_set = false;
            }
            block => return Err(format!("unknown GIF block 0x{block:02x}")),
        }
    }
    if !trailer {
        return Err("GIF trailer is missing".into());
    }
    if frames.is_empty() {
        return Err("GIF contains no image frames".into());
    }
    Ok(GifImage {
        width,
        height,
        render_width,
        render_height,
        frames,
    })
}

fn gif_render_size(width: u16, height: u16) -> (u16, u16) {
    let scale = (MAX_GIF_RENDER_WIDTH as f64 / f64::from(width))
        .min(MAX_GIF_RENDER_HEIGHT as f64 / f64::from(height))
        .min(1.0);
    (
        (f64::from(width) * scale).round().max(1.0) as u16,
        (f64::from(height) * scale).round().max(1.0) as u16,
    )
}

fn downsample(
    pixels: &[Rgb],
    source_width: usize,
    source_height: usize,
    width: usize,
    height: usize,
) -> Vec<Rgb> {
    let mut output = Vec::with_capacity(width * height);
    for y in 0..height {
        let source_y = (y * source_height / height).min(source_height - 1);
        for x in 0..width {
            let source_x = (x * source_width / width).min(source_width - 1);
            output.push(pixels[source_y * source_width + source_x]);
        }
    }
    output
}

fn clear_rect(
    canvas: &mut [Rgb],
    canvas_width: usize,
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    background: Rgb,
) {
    for y in top..top + height {
        canvas[y * canvas_width + left..y * canvas_width + left + width].fill(background);
    }
}

fn decode_lzw(data: &[u8], minimum_code_size: u8, expected: usize) -> Result<Vec<u8>, String> {
    if !(2..=8).contains(&minimum_code_size) {
        return Err("GIF LZW minimum code size must be between 2 and 8".into());
    }
    let clear = 1_usize << minimum_code_size;
    let end = clear + 1;
    let mut prefix = [0_u16; 4096];
    let mut suffix = [0_u8; 4096];
    for (index, value) in suffix.iter_mut().take(clear).enumerate() {
        *value = index as u8;
    }
    let mut stack = [0_u8; 4096];
    let mut output = Vec::with_capacity(expected);
    let mut bit = 0_usize;
    let mut code_size = usize::from(minimum_code_size) + 1;
    let mut available = end + 1;
    let mut old_code = None;
    let mut first = 0_u8;
    let mut ended = false;

    while let Some(mut code) = read_lzw_code(data, &mut bit, code_size) {
        if code == clear {
            code_size = usize::from(minimum_code_size) + 1;
            available = end + 1;
            old_code = None;
            continue;
        }
        if code == end {
            ended = true;
            break;
        }
        if old_code.is_none() {
            if code >= clear {
                return Err("invalid first GIF LZW code".into());
            }
            first = code as u8;
            output.push(first);
            old_code = Some(code);
            continue;
        }
        if code > available {
            return Err("invalid GIF LZW dictionary reference".into());
        }

        let input_code = code;
        let mut depth = 0_usize;
        if code == available {
            stack[depth] = first;
            depth += 1;
            code = old_code.expect("old code is checked above");
        }
        while code >= clear {
            if code >= available || depth >= stack.len() {
                return Err("invalid GIF LZW prefix chain".into());
            }
            stack[depth] = suffix[code];
            depth += 1;
            code = usize::from(prefix[code]);
        }
        first = code as u8;
        if depth >= stack.len() {
            return Err("GIF LZW stack overflow".into());
        }
        stack[depth] = first;
        depth += 1;
        while depth > 0 {
            depth -= 1;
            output.push(stack[depth]);
            if output.len() > expected {
                return Err("GIF frame expands beyond its dimensions".into());
            }
        }

        if available < 4096 {
            prefix[available] = old_code.expect("old code is checked above") as u16;
            suffix[available] = first;
            available += 1;
            if available == 1_usize << code_size && code_size < 12 {
                code_size += 1;
            }
        }
        old_code = Some(input_code);
    }
    if !ended {
        return Err("GIF LZW stream has no end code".into());
    }
    if output.len() != expected {
        return Err(format!(
            "GIF frame decoded {} pixels, expected {expected}",
            output.len()
        ));
    }
    Ok(output)
}

fn read_lzw_code(data: &[u8], bit: &mut usize, width: usize) -> Option<usize> {
    let end = bit.checked_add(width)?;
    if end > data.len().checked_mul(8)? {
        return None;
    }
    let mut code = 0_usize;
    for shift in 0..width {
        let position = *bit + shift;
        code |= usize::from((data[position / 8] >> (position % 8)) & 1) << shift;
    }
    *bit = end;
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIF: &[u8] = &[
        b'G', b'I', b'F', b'8', b'9', b'a', 2, 0, 1, 0, 0x80, 0, 0, 0, 0, 0, 255, 255, 255, 0x21,
        0xf9, 4, 0, 5, 0, 0, 0, 0x2c, 0, 0, 0, 0, 2, 0, 1, 0, 0, 2, 2, 0x44, 0x0a, 0, 0x3b,
    ];

    fn options(mode: Mode) -> Options {
        Options {
            action: Action::Snapshot,
            requested: mode,
            input: None,
            seed: 7,
            frame: 24,
            format: Format::Plain,
            fps: None,
            condition: Condition::Snow,
            night: true,
        }
    }

    #[test]
    fn generated_scenes_are_deterministic_and_bounded() {
        for mode in [Mode::Rain, Mode::Pipes, Mode::Pond, Mode::Weather] {
            let mut first = Scene::load(mode, &options(mode)).expect("load scene");
            let mut second = Scene::load(mode, &options(mode)).expect("reload scene");
            let area = scene_area(80, 24);
            first.seek(24, area);
            second.seek(24, area);
            let mut first_screen = Screen::new(80, 24);
            let mut second_screen = Screen::new(80, 24);
            draw(&mut first_screen, &first, 24, false, 0);
            draw(&mut second_screen, &second, 24, false, 0);
            assert_eq!(first_screen.cells, second_screen.cells);
            assert!(first_screen.lines().iter().any(|line| !line.is_empty()));
        }
    }

    #[test]
    fn decodes_a_local_gif_without_a_codec_dependency() {
        let image = parse_gif(GIF).expect("decode GIF");
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.frames.len(), 1);
        assert_eq!(image.frames[0].delay_ms, 50);
        assert_eq!(image.frames[0].pixels[0], Rgb::default());
        assert_eq!(
            image.frames[0].pixels[1],
            Rgb {
                red: 255,
                green: 255,
                blue: 255
            }
        );
        assert!(parse_gif(&GIF[..GIF.len() - 1]).is_err());

        let mut two_frames = GIF[..GIF.len() - 1].to_vec();
        two_frames.extend_from_slice(&GIF[19..GIF.len() - 1]);
        two_frames.push(0x3b);
        assert!(parse_gif_with_budget(&two_frames, 3).is_err());
    }

    #[test]
    fn validates_public_options() {
        let parsed = parse_options(
            [
                "snapshot",
                "weather",
                "--condition",
                "snow",
                "--night",
                "--json",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            false,
        )
        .expect("parse options");
        assert_eq!(parsed.requested, Mode::Weather);
        assert_eq!(parsed.condition, Condition::Snow);
        assert!(parsed.night);
        assert_eq!(parsed.format, Format::Json);
        assert!(parse_options(vec!["gif".into()], true).is_err());
        assert!(
            parse_options(
                vec!["snapshot".into(), "rain".into(), "--fps".into(), "2".into()],
                false
            )
            .is_err()
        );
    }
}
