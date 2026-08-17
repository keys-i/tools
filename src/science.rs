use std::collections::HashMap;
use std::f64::consts::PI;
use std::fs::File;
use std::io::{self, BufRead, BufReader, IsTerminal as _, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::queue;
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, size};

use crate::terminal::Session;
use crate::{VERSION, json_string};

const HELP: &str = "Fast offline science views for planetary orbits, VCD waveforms, and point clouds\n\nUsage:\n  science [open] orbit [--days OFFSET]\n  science [open] wave FILE\n  science [open] cloud FILE\n  science snapshot orbit [--days OFFSET] [--plain|--json]\n  science snapshot wave FILE [--plain|--json]\n  science snapshot cloud FILE [--plain|--json]\n\nOptions:\n  --days OFFSET  Days from now; JPL approximation is limited to 1800-2050\n  --plain        Stable tab-separated snapshot\n  --json         Machine-readable snapshot\n  -h, --help     Show this help\n  -V, --version  Show the version\n\nControls: arrows navigate, +/- zoom, Home reset, r reset, q quit.";
const MAX_VCD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_VCD_SIGNALS: usize = 4096;
const MAX_VCD_CHANGES: usize = 2_000_000;
const MAX_CLOUD_POINTS: u64 = 50_000_000;
const MAX_CLOUD_SAMPLE: u64 = 60_000;
const MAX_LINE_BYTES: usize = 64 * 1024;
const MIN_JULIAN_DAY: f64 = 2_378_496.5;
const MAX_JULIAN_DAY: f64 = 2_470_172.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Open,
    Snapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Orbit,
    Wave,
    Cloud,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Plain,
    Json,
}

#[derive(Debug, PartialEq)]
struct Options {
    mode: Mode,
    view: View,
    input: Option<PathBuf>,
    days: f64,
    format: Option<Format>,
}

pub fn run(arguments: Vec<String>) -> Result<i32, String> {
    match arguments.as_slice() {
        [argument] if matches!(argument.as_str(), "-h" | "--help") => {
            println!("{HELP}");
            return Ok(0);
        }
        [argument] if matches!(argument.as_str(), "-V" | "--version") => {
            println!("science {VERSION}");
            return Ok(0);
        }
        _ => {}
    }

    let options = parse_options(arguments)?;
    if options.mode == Mode::Open {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err("interactive views need a terminal; use 'science snapshot'".into());
        }
        return match options.view {
            View::Orbit => {
                target_julian_day(options.days)?;
                interactive_orbit(options.days)
            }
            View::Wave => interactive_wave(load_wave(required_input(&options)?)?),
            View::Cloud => interactive_cloud(load_cloud(required_input(&options)?)?),
        };
    }

    match options.view {
        View::Orbit => {
            let julian_day = target_julian_day(options.days)?;
            let positions = planet_positions(julian_day);
            if options.format == Some(Format::Json) {
                print_orbit_json(julian_day, options.days, &positions);
            } else {
                print_orbit_plain(julian_day, options.days, &positions);
            }
        }
        View::Wave => {
            let waveform = load_wave(required_input(&options)?)?;
            if options.format == Some(Format::Json) {
                print_wave_json(&waveform);
            } else {
                print_wave_plain(&waveform);
            }
        }
        View::Cloud => {
            let cloud = load_cloud(required_input(&options)?)?;
            if options.format == Some(Format::Json) {
                print_cloud_json(&cloud);
            } else {
                print_cloud_plain(&cloud);
            }
        }
    }
    Ok(0)
}

fn parse_options(arguments: Vec<String>) -> Result<Options, String> {
    let terminal = io::stdin().is_terminal() && io::stdout().is_terminal();
    let mut mode = None;
    let mut view = None;
    let mut input = None;
    let mut days = 0.0;
    let mut days_set = false;
    let mut format = None;
    let mut arguments = arguments.into_iter();

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "open" if mode.is_none() => mode = Some(Mode::Open),
            "snapshot" if mode.is_none() => mode = Some(Mode::Snapshot),
            "orbit" if view.is_none() => view = Some(View::Orbit),
            "wave" if view.is_none() => view = Some(View::Wave),
            "cloud" if view.is_none() => view = Some(View::Cloud),
            "--days" if !days_set => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--days needs a finite number".to_owned())?;
                days = value
                    .parse::<f64>()
                    .map_err(|_| "--days needs a finite number".to_owned())?;
                if !days.is_finite() {
                    return Err("--days needs a finite number".into());
                }
                days_set = true;
            }
            "--plain" if format.is_none() => format = Some(Format::Plain),
            "--json" if format.is_none() => format = Some(Format::Json),
            "--plain" | "--json" => {
                return Err("--plain and --json cannot be used together".into());
            }
            value if value.starts_with('-') && value != "-" => {
                return Err(format!("unknown option {value:?}; try 'science --help'"));
            }
            value if input.is_none() => input = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument {value:?}")),
        }
    }

    let view = view.unwrap_or(View::Orbit);
    let mode = mode.unwrap_or(if format.is_none() && terminal {
        Mode::Open
    } else {
        Mode::Snapshot
    });
    if mode == Mode::Open && format.is_some() {
        return Err("--plain and --json are snapshot options".into());
    }
    if view == View::Orbit && input.is_some() {
        return Err("orbit does not accept an input file".into());
    }
    if view != View::Orbit && input.is_none() {
        return Err(match view {
            View::Wave => "wave needs a VCD file",
            View::Cloud => "cloud needs a PLY or .splat file",
            View::Orbit => unreachable!(),
        }
        .into());
    }
    if view != View::Orbit && days_set {
        return Err("--days is only valid with orbit".into());
    }
    if mode == Mode::Open && input.as_deref() == Some(Path::new("-")) {
        return Err("interactive input cannot also be stdin; use a file or snapshot '-'".into());
    }
    Ok(Options {
        mode,
        view,
        input,
        days,
        format,
    })
}

fn required_input(options: &Options) -> Result<&Path, String> {
    options
        .input
        .as_deref()
        .ok_or_else(|| "input file is required".to_owned())
}

#[derive(Clone, Copy)]
struct Elements {
    name: &'static str,
    symbol: char,
    base: [f64; 6],
    rate: [f64; 6],
}

#[derive(Clone, Copy, Debug)]
struct PlanetPosition {
    name: &'static str,
    symbol: char,
    x: f64,
    y: f64,
    z: f64,
}

// JPL Solar System Dynamics table 1, valid for 1800-2050:
// https://ssd.jpl.nasa.gov/planets/approx_pos.html
const PLANETS: [Elements; 8] = [
    Elements {
        name: "Mercury",
        symbol: 'm',
        base: [
            0.38709927,
            0.20563593,
            7.00497902,
            252.25032350,
            77.45779628,
            48.33076593,
        ],
        rate: [
            0.00000037,
            0.00001906,
            -0.00594749,
            149472.67411175,
            0.16047689,
            -0.12534081,
        ],
    },
    Elements {
        name: "Venus",
        symbol: 'v',
        base: [
            0.72333566,
            0.00677672,
            3.39467605,
            181.97909950,
            131.60246718,
            76.67984255,
        ],
        rate: [
            0.00000390,
            -0.00004107,
            -0.00078890,
            58517.81538729,
            0.00268329,
            -0.27769418,
        ],
    },
    Elements {
        name: "Earth-Moon",
        symbol: 'E',
        base: [
            1.00000261,
            0.01671123,
            -0.00001531,
            100.46457166,
            102.93768193,
            0.0,
        ],
        rate: [
            0.00000562,
            -0.00004392,
            -0.01294668,
            35999.37244981,
            0.32327364,
            0.0,
        ],
    },
    Elements {
        name: "Mars",
        symbol: 'M',
        base: [
            1.52371034,
            0.09339410,
            1.84969142,
            -4.55343205,
            -23.94362959,
            49.55953891,
        ],
        rate: [
            0.00001847,
            0.00007882,
            -0.00813131,
            19140.30268499,
            0.44441088,
            -0.29257343,
        ],
    },
    Elements {
        name: "Jupiter",
        symbol: 'J',
        base: [
            5.20288700,
            0.04838624,
            1.30439695,
            34.39644051,
            14.72847983,
            100.47390909,
        ],
        rate: [
            -0.00011607,
            -0.00013253,
            -0.00183714,
            3034.74612775,
            0.21252668,
            0.20469106,
        ],
    },
    Elements {
        name: "Saturn",
        symbol: 'S',
        base: [
            9.53667594,
            0.05386179,
            2.48599187,
            49.95424423,
            92.59887831,
            113.66242448,
        ],
        rate: [
            -0.00125060,
            -0.00050991,
            0.00193609,
            1222.49362201,
            -0.41897216,
            -0.28867794,
        ],
    },
    Elements {
        name: "Uranus",
        symbol: 'U',
        base: [
            19.18916464,
            0.04725744,
            0.77263783,
            313.23810451,
            170.95427630,
            74.01692503,
        ],
        rate: [
            -0.00196176,
            -0.00004397,
            -0.00242939,
            428.48202785,
            0.40805281,
            0.04240589,
        ],
    },
    Elements {
        name: "Neptune",
        symbol: 'N',
        base: [
            30.06992276,
            0.00859048,
            1.77004347,
            -55.12002969,
            44.96476227,
            131.78422574,
        ],
        rate: [
            0.00026291,
            0.00005105,
            0.00035372,
            218.45945325,
            -0.32241464,
            -0.00508664,
        ],
    },
];

fn now_julian_day() -> Result<f64, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before 1970".to_owned())?
        .as_secs_f64();
    Ok(2_440_587.5 + seconds / 86_400.0)
}

fn target_julian_day(days: f64) -> Result<f64, String> {
    let julian_day = now_julian_day()? + days;
    if !(MIN_JULIAN_DAY..MAX_JULIAN_DAY).contains(&julian_day) {
        return Err("requested orbit is outside the JPL approximation range (1800-2050)".into());
    }
    Ok(julian_day)
}

fn planet_positions(julian_day: f64) -> Vec<PlanetPosition> {
    let centuries = (julian_day - 2_451_545.0) / 36_525.0;
    PLANETS
        .iter()
        .map(|planet| {
            let mut values = [0.0; 6];
            for (index, value) in values.iter_mut().enumerate() {
                *value = planet.base[index] + planet.rate[index] * centuries;
            }
            let [a, eccentricity, inclination, longitude, perihelion, node] = values;
            let mean_anomaly = (longitude - perihelion + 180.0).rem_euclid(360.0) - 180.0;
            let mean_anomaly = mean_anomaly.to_radians();
            let mut eccentric_anomaly = mean_anomaly + eccentricity * mean_anomaly.sin();
            for _ in 0..12 {
                let correction = (mean_anomaly
                    - (eccentric_anomaly - eccentricity * eccentric_anomaly.sin()))
                    / (1.0 - eccentricity * eccentric_anomaly.cos());
                eccentric_anomaly += correction;
                if correction.abs() <= 1.0e-12 {
                    break;
                }
            }
            let orbital_x = a * (eccentric_anomaly.cos() - eccentricity);
            let orbital_y =
                a * (1.0 - eccentricity * eccentricity).sqrt() * eccentric_anomaly.sin();
            let argument = (perihelion - node).to_radians();
            let inclination = inclination.to_radians();
            let node = node.to_radians();
            let (sin_argument, cos_argument) = argument.sin_cos();
            let (sin_inclination, cos_inclination) = inclination.sin_cos();
            let (sin_node, cos_node) = node.sin_cos();
            PlanetPosition {
                name: planet.name,
                symbol: planet.symbol,
                x: (cos_argument * cos_node - sin_argument * sin_node * cos_inclination)
                    * orbital_x
                    + (-sin_argument * cos_node - cos_argument * sin_node * cos_inclination)
                        * orbital_y,
                y: (cos_argument * sin_node + sin_argument * cos_node * cos_inclination)
                    * orbital_x
                    + (-sin_argument * sin_node + cos_argument * cos_node * cos_inclination)
                        * orbital_y,
                z: sin_argument * sin_inclination * orbital_x
                    + cos_argument * sin_inclination * orbital_y,
            }
        })
        .collect()
}

fn print_orbit_plain(julian_day: f64, days: f64, positions: &[PlanetPosition]) {
    println!("Julian day\t{julian_day:.5}");
    println!("Offset days\t{days:.3}");
    println!("Planet\tX (AU)\tY (AU)\tZ (AU)\tDistance (AU)");
    for planet in positions {
        println!(
            "{}\t{:.6}\t{:.6}\t{:.6}\t{:.6}",
            planet.name,
            planet.x,
            planet.y,
            planet.z,
            distance(planet)
        );
    }
}

fn print_orbit_json(julian_day: f64, days: f64, positions: &[PlanetPosition]) {
    let planets = positions
        .iter()
        .map(|planet| {
            format!(
                "{{\"name\":{},\"x_au\":{:.9},\"y_au\":{:.9},\"z_au\":{:.9},\"distance_au\":{:.9}}}",
                json_string(planet.name),
                planet.x,
                planet.y,
                planet.z,
                distance(planet)
            )
        })
        .collect::<Vec<_>>();
    println!(
        "{{\"view\":\"orbit\",\"julian_day\":{julian_day:.9},\"offset_days\":{days:.6},\"model\":\"JPL approximate positions 1800-2050\",\"planets\":[{}]}}",
        planets.join(",")
    );
}

fn distance(planet: &PlanetPosition) -> f64 {
    (planet.x * planet.x + planet.y * planet.y + planet.z * planet.z).sqrt()
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LogicValue {
    Scalar(u8),
    Vector(Arc<str>),
    Real(Arc<str>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Change {
    time: u64,
    value: LogicValue,
}

#[derive(Debug, PartialEq, Eq)]
struct Signal {
    name: String,
    width: usize,
    changes: Vec<Change>,
}

#[derive(Debug, PartialEq, Eq)]
struct Waveform {
    source: String,
    timescale: String,
    max_time: u64,
    signals: Vec<Signal>,
}

fn load_wave(path: &Path) -> Result<Waveform, String> {
    let (source, bytes) = if path == Path::new("-") {
        let mut bytes = Vec::new();
        io::stdin()
            .take(MAX_VCD_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("cannot read stdin: {error}"))?;
        ("stdin".to_owned(), bytes)
    } else {
        let source = path_text(path);
        let file = File::open(path).map_err(|error| format!("cannot open {source}: {error}"))?;
        let mut bytes = Vec::new();
        file.take(MAX_VCD_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("cannot read {source}: {error}"))?;
        (source, bytes)
    };
    if bytes.len() as u64 > MAX_VCD_BYTES {
        return Err("VCD input exceeds 64 MiB".into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "VCD input is not UTF-8 text".to_owned())?;
    parse_vcd(source, text)
}

fn parse_vcd(source: String, text: &str) -> Result<Waveform, String> {
    let mut scope = Vec::new();
    let mut signals: Vec<Signal> = Vec::new();
    let mut aliases: HashMap<String, Vec<usize>> = HashMap::new();
    let mut timescale = String::new();
    let mut collecting_timescale = false;
    let mut definitions_done = false;
    let mut time = 0;
    let mut max_time = 0;
    let mut total_changes = 0;

    for (line_number, raw_line) in text.lines().enumerate() {
        if raw_line.len() > MAX_LINE_BYTES {
            return Err(format!("VCD line {} exceeds 64 KiB", line_number + 1));
        }
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if collecting_timescale {
            let before_end = line.split("$end").next().unwrap_or(line).trim();
            if !before_end.is_empty() {
                if !timescale.is_empty() {
                    timescale.push(' ');
                }
                timescale.push_str(before_end);
            }
            if line.contains("$end") {
                collecting_timescale = false;
            }
            continue;
        }
        if line.starts_with("$timescale") {
            let rest = line.trim_start_matches("$timescale").trim();
            let before_end = rest.split("$end").next().unwrap_or(rest).trim();
            timescale.push_str(before_end);
            collecting_timescale = !line.contains("$end");
            continue;
        }
        if line.starts_with("$scope") {
            let tokens = line.split_whitespace().collect::<Vec<_>>();
            if tokens.len() < 4 || tokens.last() != Some(&"$end") {
                return Err(format!("invalid $scope on VCD line {}", line_number + 1));
            }
            scope.push(tokens[2].to_owned());
            continue;
        }
        if line.starts_with("$upscope") {
            if scope.pop().is_none() {
                return Err(format!(
                    "unmatched $upscope on VCD line {}",
                    line_number + 1
                ));
            }
            continue;
        }
        if line.starts_with("$var") {
            let tokens = line.split_whitespace().collect::<Vec<_>>();
            let end = tokens.iter().position(|token| *token == "$end");
            let Some(end) = end else {
                return Err(format!("unterminated $var on VCD line {}", line_number + 1));
            };
            if end < 5 {
                return Err(format!("invalid $var on VCD line {}", line_number + 1));
            }
            if signals.len() >= MAX_VCD_SIGNALS {
                return Err("VCD contains more than 4096 signals".into());
            }
            let width = tokens[2]
                .parse::<usize>()
                .map_err(|_| format!("invalid signal width on VCD line {}", line_number + 1))?;
            if width == 0 || width > 1_000_000 {
                return Err(format!(
                    "unsupported signal width on VCD line {}",
                    line_number + 1
                ));
            }
            let reference = tokens[4..end].join("");
            let name = if scope.is_empty() {
                reference
            } else {
                format!("{}.{}", scope.join("."), reference)
            };
            let index = signals.len();
            signals.push(Signal {
                name: safe_text(&name),
                width,
                changes: Vec::new(),
            });
            aliases.entry(tokens[3].to_owned()).or_default().push(index);
            continue;
        }
        if line.starts_with("$enddefinitions") {
            definitions_done = true;
            continue;
        }
        if !definitions_done || line.starts_with('$') {
            continue;
        }
        if let Some(value) = line.strip_prefix('#') {
            let next_time = value
                .trim()
                .parse::<u64>()
                .map_err(|_| format!("invalid timestamp on VCD line {}", line_number + 1))?;
            if next_time < time {
                return Err(format!(
                    "timestamp moves backwards on VCD line {}",
                    line_number + 1
                ));
            }
            time = next_time;
            max_time = max_time.max(time);
            continue;
        }

        let prefix = line.as_bytes()[0];
        let (identifier, value) = match prefix {
            b'0' | b'1' | b'x' | b'X' | b'z' | b'Z' => {
                (line[1..].trim(), logic_value(prefix, &line[..1])?)
            }
            b'b' | b'B' | b'r' | b'R' => {
                let mut values = line[1..].split_whitespace();
                let value = values.next().unwrap_or("");
                let identifier = values.next().unwrap_or("");
                if value.is_empty() || identifier.is_empty() {
                    return Err(format!(
                        "invalid value change on VCD line {}",
                        line_number + 1
                    ));
                }
                (identifier, logic_value(prefix, value)?)
            }
            _ => {
                return Err(format!(
                    "invalid value change on VCD line {}",
                    line_number + 1
                ));
            }
        };
        let Some(indices) = aliases.get(identifier) else {
            return Err(format!("unknown signal id on VCD line {}", line_number + 1));
        };
        for &index in indices {
            let changes = &mut signals[index].changes;
            let length = changes.len();
            if length > 0 && changes[length - 1].time == time {
                changes[length - 1].value = value.clone();
                if length > 1 && changes[length - 2].value == changes[length - 1].value {
                    changes.pop();
                    total_changes -= 1;
                }
                continue;
            }
            if changes.last().is_some_and(|last| last.value == value) {
                continue;
            }
            if total_changes >= MAX_VCD_CHANGES {
                return Err("VCD contains more than 2,000,000 distinct changes".into());
            }
            changes.push(Change {
                time,
                value: value.clone(),
            });
            total_changes += 1;
        }
    }

    if !definitions_done {
        return Err("VCD is missing $enddefinitions".into());
    }
    if signals.is_empty() {
        return Err("VCD contains no signals".into());
    }
    if timescale.is_empty() {
        timescale = "unknown".into();
    }
    Ok(Waveform {
        source,
        timescale: safe_text(&timescale),
        max_time,
        signals,
    })
}

fn logic_value(prefix: u8, value: &str) -> Result<LogicValue, String> {
    match prefix.to_ascii_lowercase() {
        b'0' | b'1' | b'x' | b'z' => Ok(LogicValue::Scalar(prefix.to_ascii_lowercase())),
        b'b' => {
            if value.is_empty()
                || value
                    .bytes()
                    .any(|byte| !matches!(byte.to_ascii_lowercase(), b'0' | b'1' | b'x' | b'z'))
            {
                return Err("VCD vector contains a non-binary logic value".into());
            }
            Ok(LogicValue::Vector(Arc::from(value.to_ascii_lowercase())))
        }
        b'r' => {
            value
                .parse::<f64>()
                .map_err(|_| "VCD real value is not a number".to_owned())?;
            Ok(LogicValue::Real(Arc::from(value)))
        }
        _ => Err("unsupported VCD value".into()),
    }
}

fn wave_change_count(waveform: &Waveform) -> usize {
    waveform
        .signals
        .iter()
        .map(|signal| signal.changes.len())
        .sum()
}

fn print_wave_plain(waveform: &Waveform) {
    println!("Format\tVCD");
    println!("Source\t{}", waveform.source);
    println!("Timescale\t{}", waveform.timescale);
    println!("Duration\t{}", waveform.max_time);
    println!("Signals\t{}", waveform.signals.len());
    println!("Changes\t{}", wave_change_count(waveform));
    println!("Signal\tWidth\tChanges");
    for signal in &waveform.signals {
        println!(
            "{}\t{}\t{}",
            signal.name,
            signal.width,
            signal.changes.len()
        );
    }
}

fn print_wave_json(waveform: &Waveform) {
    let signals = waveform
        .signals
        .iter()
        .map(|signal| {
            format!(
                "{{\"name\":{},\"width\":{},\"changes\":{}}}",
                json_string(&signal.name),
                signal.width,
                signal.changes.len()
            )
        })
        .collect::<Vec<_>>();
    println!(
        "{{\"view\":\"wave\",\"format\":\"VCD\",\"source\":{},\"timescale\":{},\"duration\":{},\"signals\":{},\"changes\":{},\"signal_data\":[{}]}}",
        json_string(&waveform.source),
        json_string(&waveform.timescale),
        waveform.max_time,
        waveform.signals.len(),
        wave_change_count(waveform),
        signals.join(",")
    );
}

#[derive(Clone, Copy, Debug)]
struct Point {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug)]
struct Cloud {
    source: String,
    format: &'static str,
    count: u64,
    min: [f32; 3],
    max: [f32; 3],
    center: [f64; 3],
    sample: Vec<Point>,
}

struct CloudBuilder {
    count: u64,
    step: u64,
    min: [f32; 3],
    max: [f32; 3],
    sum: [f64; 3],
    sample: Vec<Point>,
}

impl CloudBuilder {
    fn new(count: u64) -> Result<Self, String> {
        if count == 0 {
            return Err("point cloud is empty".into());
        }
        if count > MAX_CLOUD_POINTS {
            return Err("point cloud exceeds 50,000,000 points".into());
        }
        Ok(Self {
            count,
            step: count.div_ceil(MAX_CLOUD_SAMPLE).max(1),
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
            sum: [0.0; 3],
            sample: Vec::with_capacity(count.min(MAX_CLOUD_SAMPLE) as usize),
        })
    }

    fn add(&mut self, index: u64, point: Point) -> Result<(), String> {
        let values = [point.x, point.y, point.z];
        if values.iter().any(|value| !value.is_finite()) {
            return Err(format!(
                "point {} contains a non-finite coordinate",
                index + 1
            ));
        }
        for (axis, value) in values.into_iter().enumerate() {
            self.min[axis] = self.min[axis].min(value);
            self.max[axis] = self.max[axis].max(value);
            self.sum[axis] += f64::from(value);
        }
        if index.is_multiple_of(self.step) {
            self.sample.push(point);
        }
        Ok(())
    }

    fn finish(self, source: String, format: &'static str) -> Cloud {
        Cloud {
            source,
            format,
            count: self.count,
            min: self.min,
            max: self.max,
            center: self.sum.map(|sum| sum / self.count as f64),
            sample: self.sample,
        }
    }
}

fn load_cloud(path: &Path) -> Result<Cloud, String> {
    if path == Path::new("-") {
        return Err("cloud input needs a seekable .ply or .splat file".into());
    }
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("splat") => load_splat(path),
        Some("ply") => load_ply(path),
        _ => Err("cloud input must end in .ply or .splat".into()),
    }
}

fn load_splat(path: &Path) -> Result<Cloud, String> {
    let source = path_text(path);
    let file = File::open(path).map_err(|error| format!("cannot open {source}: {error}"))?;
    let bytes = file
        .metadata()
        .map_err(|error| format!("cannot inspect {source}: {error}"))?
        .len();
    if bytes % 32 != 0 {
        return Err(".splat size is not a whole number of 32-byte records".into());
    }
    let count = bytes / 32;
    let mut builder = CloudBuilder::new(count)?;
    let mut reader = BufReader::new(file);
    let mut record = [0_u8; 32];
    for index in 0..count {
        reader
            .read_exact(&mut record)
            .map_err(|error| format!("cannot read {source}: {error}"))?;
        builder.add(
            index,
            Point {
                x: f32::from_le_bytes(record[0..4].try_into().expect("four-byte coordinate")),
                y: f32::from_le_bytes(record[4..8].try_into().expect("four-byte coordinate")),
                z: f32::from_le_bytes(record[8..12].try_into().expect("four-byte coordinate")),
            },
        )?;
    }
    Ok(builder.finish(source, "splat-v1"))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PlyEncoding {
    Ascii,
    BinaryLittleEndian,
}

#[derive(Clone, Copy)]
enum PlyType {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

impl PlyType {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "char" | "int8" => Ok(Self::I8),
            "uchar" | "uint8" => Ok(Self::U8),
            "short" | "int16" => Ok(Self::I16),
            "ushort" | "uint16" => Ok(Self::U16),
            "int" | "int32" => Ok(Self::I32),
            "uint" | "uint32" => Ok(Self::U32),
            "float" | "float32" => Ok(Self::F32),
            "double" | "float64" => Ok(Self::F64),
            _ => Err(format!("unsupported PLY property type {value:?}")),
        }
    }

    fn size(self) -> usize {
        match self {
            Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::F64 => 8,
        }
    }

    fn decode(self, bytes: &[u8]) -> f32 {
        match self {
            Self::I8 => i8::from_le_bytes([bytes[0]]) as f32,
            Self::U8 => bytes[0] as f32,
            Self::I16 => i16::from_le_bytes(bytes.try_into().expect("two-byte property")) as f32,
            Self::U16 => u16::from_le_bytes(bytes.try_into().expect("two-byte property")) as f32,
            Self::I32 => i32::from_le_bytes(bytes.try_into().expect("four-byte property")) as f32,
            Self::U32 => u32::from_le_bytes(bytes.try_into().expect("four-byte property")) as f32,
            Self::F32 => f32::from_le_bytes(bytes.try_into().expect("four-byte property")),
            Self::F64 => f64::from_le_bytes(bytes.try_into().expect("eight-byte property")) as f32,
        }
    }
}

struct PlyProperty {
    name: String,
    kind: PlyType,
    offset: usize,
}

fn load_ply(path: &Path) -> Result<Cloud, String> {
    let source = path_text(path);
    let file = File::open(path).map_err(|error| format!("cannot open {source}: {error}"))?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    read_line_limited(&mut reader, &mut line)?;
    if line.trim() != "ply" {
        return Err("PLY input is missing its ply header".into());
    }

    let mut encoding = None;
    let mut count = None;
    let mut in_vertices = false;
    let mut properties = Vec::new();
    let mut record_size = 0_usize;
    let mut saw_element = false;
    let mut header_bytes = line.len();
    loop {
        read_line_limited(&mut reader, &mut line)?;
        if line.is_empty() {
            return Err("PLY header ends before end_header".into());
        }
        header_bytes += line.len();
        if header_bytes > MAX_LINE_BYTES {
            return Err("PLY header exceeds 64 KiB".into());
        }
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        match tokens.as_slice() {
            ["format", "ascii", "1.0"] => encoding = Some(PlyEncoding::Ascii),
            ["format", "binary_little_endian", "1.0"] => {
                encoding = Some(PlyEncoding::BinaryLittleEndian)
            }
            ["format", ..] => {
                return Err("PLY must be ASCII or binary little-endian version 1.0".into());
            }
            ["element", "vertex", value] => {
                if saw_element {
                    return Err("PLY vertex data must be the first element".into());
                }
                count = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| "invalid PLY vertex count".to_owned())?,
                );
                in_vertices = true;
                saw_element = true;
            }
            ["element", ..] => {
                in_vertices = false;
                saw_element = true;
            }
            ["property", "list", ..] if in_vertices => {
                return Err("list properties are invalid for PLY vertices".into());
            }
            ["property", kind, name] if in_vertices => {
                let kind = PlyType::parse(kind)?;
                properties.push(PlyProperty {
                    name: (*name).to_owned(),
                    kind,
                    offset: record_size,
                });
                record_size = record_size
                    .checked_add(kind.size())
                    .ok_or_else(|| "PLY vertex record is too large".to_owned())?;
                if record_size > MAX_LINE_BYTES {
                    return Err("PLY vertex record exceeds 64 KiB".into());
                }
            }
            ["end_header"] => break,
            _ => {}
        }
    }
    let encoding = encoding.ok_or_else(|| "PLY header has no supported format".to_owned())?;
    let count = count.ok_or_else(|| "PLY header has no vertex element".to_owned())?;
    let x = property_index(&properties, "x")?;
    let y = property_index(&properties, "y")?;
    let z = property_index(&properties, "z")?;
    let mut builder = CloudBuilder::new(count)?;
    match encoding {
        PlyEncoding::Ascii => {
            for index in 0..count {
                read_line_limited(&mut reader, &mut line)?;
                if line.is_empty() {
                    return Err(format!("PLY ends after {index} of {count} vertices"));
                }
                let mut coordinates = [None; 3];
                for (property, value) in line.split_whitespace().enumerate() {
                    let axis = if property == x {
                        Some(0)
                    } else if property == y {
                        Some(1)
                    } else if property == z {
                        Some(2)
                    } else {
                        None
                    };
                    if let Some(axis) = axis {
                        coordinates[axis] = Some(value.parse::<f32>().map_err(|_| {
                            format!("PLY vertex {} has an invalid coordinate", index + 1)
                        })?);
                    }
                }
                let [Some(x), Some(y), Some(z)] = coordinates else {
                    return Err(format!("PLY vertex {} has too few properties", index + 1));
                };
                builder.add(index, Point { x, y, z })?;
            }
            Ok(builder.finish(source, "ply-ascii-1.0"))
        }
        PlyEncoding::BinaryLittleEndian => {
            let mut record = vec![0_u8; record_size];
            for index in 0..count {
                reader
                    .read_exact(&mut record)
                    .map_err(|error| format!("cannot read PLY vertex {}: {error}", index + 1))?;
                let coordinate = |property: &PlyProperty| {
                    property
                        .kind
                        .decode(&record[property.offset..property.offset + property.kind.size()])
                };
                builder.add(
                    index,
                    Point {
                        x: coordinate(&properties[x]),
                        y: coordinate(&properties[y]),
                        z: coordinate(&properties[z]),
                    },
                )?;
            }
            Ok(builder.finish(source, "ply-binary-le-1.0"))
        }
    }
}

fn read_line_limited(reader: &mut impl BufRead, line: &mut String) -> Result<(), String> {
    line.clear();
    let read = (&mut *reader)
        .take((MAX_LINE_BYTES + 1) as u64)
        .read_line(line)
        .map_err(|error| format!("cannot read point cloud: {error}"))?;
    if read > MAX_LINE_BYTES {
        return Err("point-cloud line exceeds 64 KiB".into());
    }
    Ok(())
}

fn property_index(properties: &[PlyProperty], name: &str) -> Result<usize, String> {
    properties
        .iter()
        .position(|property| property.name == name)
        .ok_or_else(|| format!("PLY vertices have no {name} property"))
}

fn safe_text(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                '�'
            } else {
                character
            }
        })
        .collect()
}

fn path_text(path: &Path) -> String {
    safe_text(&path.display().to_string())
}

fn print_cloud_plain(cloud: &Cloud) {
    println!("Format\t{}", cloud.format);
    println!("Source\t{}", cloud.source);
    println!("Points\t{}", cloud.count);
    println!("Sampled\t{}", cloud.sample.len());
    println!(
        "Minimum\t{:.6}\t{:.6}\t{:.6}",
        cloud.min[0], cloud.min[1], cloud.min[2]
    );
    println!(
        "Maximum\t{:.6}\t{:.6}\t{:.6}",
        cloud.max[0], cloud.max[1], cloud.max[2]
    );
    println!(
        "Centroid\t{:.6}\t{:.6}\t{:.6}",
        cloud.center[0], cloud.center[1], cloud.center[2]
    );
}

fn print_cloud_json(cloud: &Cloud) {
    println!(
        "{{\"view\":\"cloud\",\"format\":{},\"source\":{},\"points\":{},\"sampled\":{},\"minimum\":[{:.9},{:.9},{:.9}],\"maximum\":[{:.9},{:.9},{:.9}],\"centroid\":[{:.9},{:.9},{:.9}]}}",
        json_string(cloud.format),
        json_string(&cloud.source),
        cloud.count,
        cloud.sample.len(),
        cloud.min[0],
        cloud.min[1],
        cloud.min[2],
        cloud.max[0],
        cloud.max[1],
        cloud.max[2],
        cloud.center[0],
        cloud.center[1],
        cloud.center[2]
    );
}

fn interactive_orbit(initial_days: f64) -> Result<i32, String> {
    let _session = Session::enter().map_err(|error| format!("terminal: {error}"))?;
    let color = crate::use_color(false);
    let today = now_julian_day()?;
    let mut days = initial_days;
    let mut stdout = io::stdout();
    loop {
        let julian_day = today + days;
        draw_orbit(
            &mut stdout,
            julian_day,
            days,
            &planet_positions(julian_day),
            size_or_default(),
            color,
        )
        .map_err(|error| format!("draw: {error}"))?;
        let Event::Key(key) = event::read().map_err(|error| format!("input: {error}"))? else {
            continue;
        };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }
        if quit_key(key.code, key.modifiers) {
            break;
        }
        let delta = match key.code {
            KeyCode::Left => -1.0,
            KeyCode::Right => 1.0,
            KeyCode::PageUp => 30.0,
            KeyCode::PageDown => -30.0,
            KeyCode::Home | KeyCode::Char('r' | 'R') => {
                days = 0.0;
                continue;
            }
            _ => 0.0,
        };
        days = (days + delta).clamp(MIN_JULIAN_DAY - today, MAX_JULIAN_DAY - today - 1.0e-9);
    }
    Ok(0)
}

fn interactive_wave(waveform: Waveform) -> Result<i32, String> {
    let _session = Session::enter().map_err(|error| format!("terminal: {error}"))?;
    let color = crate::use_color(false);
    let mut selected = 0;
    let mut start = 0;
    let mut span = waveform.max_time.max(1);
    let mut stdout = io::stdout();
    loop {
        draw_wave(
            &mut stdout,
            &waveform,
            selected,
            start,
            span,
            size_or_default(),
            color,
        )
        .map_err(|error| format!("draw: {error}"))?;
        let Event::Key(key) = event::read().map_err(|error| format!("input: {error}"))? else {
            continue;
        };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }
        if quit_key(key.code, key.modifiers) {
            break;
        }
        match key.code {
            KeyCode::Up => {
                selected = selected
                    .checked_sub(1)
                    .unwrap_or(waveform.signals.len() - 1)
            }
            KeyCode::Down => selected = (selected + 1) % waveform.signals.len(),
            KeyCode::Left => start = start.saturating_sub((span / 4).max(1)),
            KeyCode::Right => {
                start = start
                    .saturating_add((span / 4).max(1))
                    .min(waveform.max_time.saturating_sub(span));
            }
            KeyCode::Char('+' | '=') => {
                let next = (span / 2).max(1);
                start = start.saturating_add((span - next) / 2);
                span = next;
            }
            KeyCode::Char('-') => {
                let next = span.saturating_mul(2).min(waveform.max_time.max(1));
                start = start.saturating_sub((next - span) / 2);
                span = next;
            }
            KeyCode::Home | KeyCode::Char('r' | 'R') => {
                start = 0;
                span = waveform.max_time.max(1);
            }
            _ => {}
        }
        start = start.min(waveform.max_time.saturating_sub(span));
    }
    Ok(0)
}

fn interactive_cloud(cloud: Cloud) -> Result<i32, String> {
    let _session = Session::enter().map_err(|error| format!("terminal: {error}"))?;
    let color = crate::use_color(false);
    let mut yaw = 0.0;
    let mut pitch = 0.0;
    let mut zoom = 1.0;
    let mut stdout = io::stdout();
    loop {
        draw_cloud(
            &mut stdout,
            &cloud,
            yaw,
            pitch,
            zoom,
            size_or_default(),
            color,
        )
        .map_err(|error| format!("draw: {error}"))?;
        let Event::Key(key) = event::read().map_err(|error| format!("input: {error}"))? else {
            continue;
        };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }
        if quit_key(key.code, key.modifiers) {
            break;
        }
        match key.code {
            KeyCode::Left => yaw -= 0.12,
            KeyCode::Right => yaw += 0.12,
            KeyCode::Up => pitch = (pitch + 0.12).min(PI / 2.0),
            KeyCode::Down => pitch = (pitch - 0.12).max(-PI / 2.0),
            KeyCode::Char('+' | '=') => zoom = (zoom * 1.2_f64).min(20.0),
            KeyCode::Char('-') => zoom = (zoom / 1.2_f64).max(0.1),
            KeyCode::Home | KeyCode::Char('r' | 'R') => {
                yaw = 0.0;
                pitch = 0.0;
                zoom = 1.0;
            }
            _ => {}
        }
    }
    Ok(0)
}

fn size_or_default() -> (u16, u16) {
    size().unwrap_or((80, 24))
}

fn quit_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(code, KeyCode::Char('q' | 'Q'))
        || modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c' | 'C'))
}

fn draw_orbit<W: Write>(
    stdout: &mut W,
    julian_day: f64,
    days: f64,
    planets: &[PlanetPosition],
    (width, height): (u16, u16),
    color: bool,
) -> io::Result<()> {
    begin(stdout)?;
    if width < 78 || height < 22 {
        return draw_small(stdout, width, height, "Resize to at least 78 x 22", color);
    }
    write_centered(
        stdout,
        1,
        width,
        "+-- SCIENCE // ORBIT --+",
        color.then_some(Color::Cyan),
    )?;
    let plot_width = usize::from(width.saturating_sub(34));
    let plot_height = usize::from(height.saturating_sub(6));
    let mut grid = vec![vec![' '; plot_width]; plot_height];
    let center_x = plot_width / 2;
    let center_y = plot_height / 2;
    grid[center_y][center_x] = '*';
    let radius = center_x.min(center_y.saturating_mul(2)).saturating_sub(1) as f64;
    for planet in planets {
        let distance = distance(planet);
        let scaled = (1.0 + distance).ln() / 31.5_f64.ln() * radius;
        let angle = planet.y.atan2(planet.x);
        let x = center_x as isize + (scaled * angle.cos()).round() as isize;
        let y = center_y as isize - (scaled * angle.sin() * 0.5).round() as isize;
        if x >= 0 && y >= 0 && (x as usize) < plot_width && (y as usize) < plot_height {
            grid[y as usize][x as usize] = planet.symbol;
        }
    }
    for (row, cells) in grid.iter().enumerate() {
        write_line(
            stdout,
            1,
            3 + row as u16,
            width,
            &cells.iter().collect::<String>(),
            color.then_some(Color::White),
        )?;
    }
    let table_x = plot_width as u16 + 2;
    write_line(
        stdout,
        table_x,
        3,
        width,
        "PLANET       DIST (AU)",
        color.then_some(Color::Cyan),
    )?;
    for (index, planet) in planets.iter().enumerate() {
        write_line(
            stdout,
            table_x,
            4 + index as u16,
            width,
            &format!("{:<10} {:>9.4}", planet.name, distance(planet)),
            color.then_some(if planet.name == "Earth-Moon" {
                Color::Green
            } else {
                Color::White
            }),
        )?;
    }
    write_line(
        stdout,
        table_x,
        13,
        width,
        &format!("JD {julian_day:.3}"),
        color.then_some(Color::Grey),
    )?;
    write_line(
        stdout,
        table_x,
        14,
        width,
        &format!("offset {days:+.0} d"),
        color.then_some(Color::Grey),
    )?;
    write_centered(
        stdout,
        height - 2,
        width,
        "Left/Right day  PgUp/PgDn month  Home reset  q quit",
        color.then_some(Color::Grey),
    )?;
    end(stdout)
}

fn draw_wave<W: Write>(
    stdout: &mut W,
    waveform: &Waveform,
    selected: usize,
    start: u64,
    span: u64,
    (width, height): (u16, u16),
    color: bool,
) -> io::Result<()> {
    begin(stdout)?;
    if width < 70 || height < 18 {
        return draw_small(stdout, width, height, "Resize to at least 70 x 18", color);
    }
    write_centered(
        stdout,
        1,
        width,
        "+-- SCIENCE // WAVE --+",
        color.then_some(Color::Cyan),
    )?;
    write_line(
        stdout,
        2,
        3,
        width,
        &format!(
            "{} signals  {} changes  timescale {}",
            waveform.signals.len(),
            wave_change_count(waveform),
            waveform.timescale
        ),
        color.then_some(Color::Grey),
    )?;
    let visible = usize::from(height.saturating_sub(8));
    let first = selected.saturating_sub(visible.saturating_sub(1));
    let graph_width = usize::from(width.saturating_sub(26)).max(1);
    for (offset, signal) in waveform
        .signals
        .iter()
        .skip(first)
        .take(visible)
        .enumerate()
    {
        let index = first + offset;
        let line = format!(
            "{} {:<20} {}",
            if index == selected { '>' } else { ' ' },
            fit(&signal.name, 20),
            wave_segment(signal, start, span, graph_width)
        );
        write_line(
            stdout,
            2,
            5 + offset as u16,
            width,
            &line,
            color.then_some(if index == selected {
                Color::Green
            } else {
                Color::White
            }),
        )?;
    }
    write_line(
        stdout,
        2,
        height - 3,
        width,
        &format!(
            "range {start}..{} / {}",
            start.saturating_add(span),
            waveform.max_time
        ),
        color.then_some(Color::Grey),
    )?;
    write_centered(
        stdout,
        height - 2,
        width,
        "Up/Down signal  Left/Right pan  +/- zoom  Home reset  q quit",
        color.then_some(Color::Grey),
    )?;
    end(stdout)
}

fn wave_segment(signal: &Signal, start: u64, span: u64, width: usize) -> String {
    let mut output = String::with_capacity(width);
    let mut change = signal
        .changes
        .partition_point(|change| change.time <= start);
    let mut value = change
        .checked_sub(1)
        .map(|index| &signal.changes[index].value);
    let denominator = width.saturating_sub(1).max(1);
    for column in 0..width {
        let time =
            start.saturating_add(((column as u128 * span as u128) / denominator as u128) as u64);
        let before = change;
        while change < signal.changes.len() && signal.changes[change].time <= time {
            value = Some(&signal.changes[change].value);
            change += 1;
        }
        output.push(
            if column > 0
                && change > before
                && matches!(value, Some(LogicValue::Vector(_) | LogicValue::Real(_)))
            {
                '│'
            } else {
                logic_symbol(value)
            },
        );
    }
    output
}

fn logic_symbol(value: Option<&LogicValue>) -> char {
    match value {
        Some(LogicValue::Scalar(b'0')) => '_',
        Some(LogicValue::Scalar(b'1')) => '─',
        Some(LogicValue::Scalar(b'z')) => 'z',
        Some(LogicValue::Vector(value)) if value.bytes().all(|byte| byte == b'z') => 'z',
        Some(LogicValue::Vector(value))
            if value.bytes().all(|byte| matches!(byte, b'0' | b'1')) =>
        {
            '═'
        }
        Some(LogicValue::Real(_)) => '~',
        _ => 'x',
    }
}

fn draw_cloud<W: Write>(
    stdout: &mut W,
    cloud: &Cloud,
    yaw: f64,
    pitch: f64,
    zoom: f64,
    (width, height): (u16, u16),
    color: bool,
) -> io::Result<()> {
    begin(stdout)?;
    if width < 60 || height < 20 {
        return draw_small(stdout, width, height, "Resize to at least 60 x 20", color);
    }
    write_centered(
        stdout,
        1,
        width,
        "+-- SCIENCE // CLOUD --+",
        color.then_some(Color::Cyan),
    )?;
    write_line(
        stdout,
        2,
        3,
        width,
        &format!(
            "{}  {} points  {} sampled",
            cloud.format,
            cloud.count,
            cloud.sample.len()
        ),
        color.then_some(Color::Grey),
    )?;
    let plot_width = usize::from(width.saturating_sub(4));
    let plot_height = usize::from(height.saturating_sub(8));
    let mut cells = vec![None::<(f64, char)>; plot_width * plot_height];
    let span = (0..3)
        .map(|axis| f64::from(cloud.max[axis]) - f64::from(cloud.min[axis]))
        .fold(0.0_f64, f64::max)
        .max(f64::EPSILON);
    let scale = (plot_width as f64).min(plot_height as f64 * 2.0) * 0.45 * zoom / span;
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let (sin_pitch, cos_pitch) = pitch.sin_cos();
    for point in &cloud.sample {
        let x = f64::from(point.x) - cloud.center[0];
        let y = f64::from(point.y) - cloud.center[1];
        let z = f64::from(point.z) - cloud.center[2];
        let rotated_x = x * cos_yaw + z * sin_yaw;
        let yaw_z = -x * sin_yaw + z * cos_yaw;
        let rotated_y = y * cos_pitch - yaw_z * sin_pitch;
        let depth = y * sin_pitch + yaw_z * cos_pitch;
        let screen_x = (plot_width as f64 / 2.0 + rotated_x * scale).round() as isize;
        let screen_y = (plot_height as f64 / 2.0 - rotated_y * scale * 0.5).round() as isize;
        if screen_x < 0
            || screen_y < 0
            || screen_x as usize >= plot_width
            || screen_y as usize >= plot_height
        {
            continue;
        }
        let index = screen_y as usize * plot_width + screen_x as usize;
        if cells[index].is_none_or(|(old_depth, _)| depth > old_depth) {
            let shade = if depth > span * 0.2 {
                '@'
            } else if depth > 0.0 {
                'O'
            } else if depth > -span * 0.2 {
                'o'
            } else {
                '.'
            };
            cells[index] = Some((depth, shade));
        }
    }
    for row in 0..plot_height {
        let line = cells[row * plot_width..(row + 1) * plot_width]
            .iter()
            .map(|cell| cell.map_or(' ', |(_, shade)| shade))
            .collect::<String>();
        write_line(
            stdout,
            2,
            5 + row as u16,
            width,
            &line,
            color.then_some(Color::White),
        )?;
    }
    write_centered(
        stdout,
        height - 2,
        width,
        "Arrows rotate  +/- zoom  Home reset  q quit",
        color.then_some(Color::Grey),
    )?;
    end(stdout)
}

fn begin<W: Write>(stdout: &mut W) -> io::Result<()> {
    queue!(
        stdout,
        BeginSynchronizedUpdate,
        MoveTo(0, 0),
        Clear(ClearType::All)
    )
}

fn end<W: Write>(stdout: &mut W) -> io::Result<()> {
    queue!(stdout, EndSynchronizedUpdate)?;
    stdout.flush()
}

fn draw_small<W: Write>(
    stdout: &mut W,
    width: u16,
    height: u16,
    message: &str,
    color: bool,
) -> io::Result<()> {
    write_centered(
        stdout,
        height / 2,
        width,
        message,
        color.then_some(Color::Yellow),
    )?;
    end(stdout)
}

fn fit(value: &str, width: usize) -> String {
    let value = value.chars().take(width).collect::<String>();
    format!("{value:<width$}")
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
    write_line(stdout, column, row, width, &value, color)
}

fn write_line<W: Write>(
    stdout: &mut W,
    column: u16,
    row: u16,
    width: u16,
    value: &str,
    color: Option<Color>,
) -> io::Result<()> {
    let value = value
        .chars()
        .take(usize::from(width.saturating_sub(column)))
        .collect::<String>();
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

#[cfg(test)]
mod tests {
    use super::*;

    const VCD: &str = "$timescale 1 ns $end\n$scope module top $end\n$var wire 1 ! clk $end\n$var wire 1 \" data $end\n$upscope $end\n$enddefinitions $end\n#0\n0!\n0\"\n#5\n1!\n#10\n0!\n1\"\n";
    const BUS_VCD: &str = "$timescale 1 ns $end\n$scope module top $end\n$var wire 4 # bus $end\n$upscope $end\n$enddefinitions $end\n#0\nb0000 #\n#1\nb0001 #\n#2\nb0010 #\n#3\nb0010 #\n";

    #[test]
    fn parses_and_deduplicates_science_inputs() {
        let wave = parse_vcd("fixture.vcd".into(), VCD).expect("parse VCD");
        assert_eq!(wave.timescale, "1 ns");
        assert_eq!(wave.max_time, 10);
        assert_eq!(wave.signals.len(), 2);
        assert_eq!(wave_change_count(&wave), 5);
        assert_eq!(wave_segment(&wave.signals[0], 0, 10, 3), "_─_");
        let bus = parse_vcd("bus.vcd".into(), BUS_VCD).expect("parse bus VCD");
        assert_eq!(bus.signals[0].changes.len(), 3);
        assert_ne!(
            bus.signals[0].changes[1].value,
            bus.signals[0].changes[2].value
        );
        assert_eq!(wave_segment(&bus.signals[0], 0, 3, 4), "═││═");

        let options = parse_options(vec![
            "snapshot".into(),
            "orbit".into(),
            "--days".into(),
            "2".into(),
            "--json".into(),
        ])
        .expect("parse CLI");
        assert_eq!(options.mode, Mode::Snapshot);
        assert_eq!(options.view, View::Orbit);
        assert_eq!(options.days, 2.0);
        assert!(parse_options(vec!["snapshot".into(), "wave".into()]).is_err());

        let positions = planet_positions(2_451_545.0);
        assert_eq!(positions.len(), 8);
        assert!(positions.iter().all(|planet| distance(planet).is_finite()));
    }

    #[test]
    fn parses_point_records_and_renders_small_fallbacks() {
        let mut builder = CloudBuilder::new(2).expect("cloud builder");
        builder
            .add(
                0,
                Point {
                    x: -1.0,
                    y: 0.0,
                    z: 2.0,
                },
            )
            .expect("first point");
        builder
            .add(
                1,
                Point {
                    x: 1.0,
                    y: 2.0,
                    z: 4.0,
                },
            )
            .expect("second point");
        let cloud = builder.finish("fixture.ply".into(), "ply-ascii-1.0");
        assert_eq!(cloud.min, [-1.0, 0.0, 2.0]);
        assert_eq!(cloud.max, [1.0, 2.0, 4.0]);
        assert_eq!(cloud.center, [0.0, 1.0, 3.0]);

        let mut output = Vec::new();
        draw_orbit(
            &mut output,
            2_451_545.0,
            0.0,
            &planet_positions(2_451_545.0),
            (40, 10),
            false,
        )
        .expect("draw fallback");
        assert!(String::from_utf8_lossy(&output).contains("Resize to at least"));
    }

    #[test]
    fn loads_ascii_and_binary_ply_and_renders_each_view() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        let ascii_path = std::env::temp_dir().join(format!("keys-tools-{suffix}-ascii.ply"));
        let binary_path = std::env::temp_dir().join(format!("keys-tools-{suffix}-binary.ply"));
        std::fs::write(
            &ascii_path,
            b"ply\nformat ascii 1.0\nelement vertex 2\nproperty float x\nproperty float y\nproperty float z\nend_header\n-1 0 2\n1 2 4\n",
        )
        .expect("write ASCII PLY");
        let mut binary = b"ply\nformat binary_little_endian 1.0\nelement vertex 2\nproperty float x\nproperty float y\nproperty float z\nend_header\n".to_vec();
        for value in [-1.0_f32, 0.0, 2.0, 1.0, 2.0, 4.0] {
            binary.extend(value.to_le_bytes());
        }
        std::fs::write(&binary_path, binary).expect("write binary PLY");

        let ascii = load_ply(&ascii_path).expect("load ASCII PLY");
        let binary = load_ply(&binary_path).expect("load binary PLY");
        std::fs::remove_file(&ascii_path).expect("remove ASCII PLY");
        std::fs::remove_file(&binary_path).expect("remove binary PLY");
        assert_eq!(ascii.center, [0.0, 1.0, 3.0]);
        assert_eq!(binary.center, ascii.center);
        assert_eq!(binary.format, "ply-binary-le-1.0");

        let wave = parse_vcd("fixture.vcd".into(), VCD).expect("parse VCD");
        let mut orbit_screen = Vec::new();
        draw_orbit(
            &mut orbit_screen,
            2_451_545.0,
            0.0,
            &planet_positions(2_451_545.0),
            (100, 28),
            false,
        )
        .expect("draw orbit");
        let mut wave_screen = Vec::new();
        draw_wave(&mut wave_screen, &wave, 0, 0, 10, (100, 24), false).expect("draw wave");
        let mut cloud_screen = Vec::new();
        draw_cloud(&mut cloud_screen, &binary, 0.0, 0.0, 1.0, (100, 28), false)
            .expect("draw cloud");
        assert!(String::from_utf8_lossy(&orbit_screen).contains("SCIENCE // ORBIT"));
        assert!(String::from_utf8_lossy(&wave_screen).contains("SCIENCE // WAVE"));
        assert!(String::from_utf8_lossy(&cloud_screen).contains("SCIENCE // CLOUD"));
        assert!(!orbit_screen.windows(5).any(|bytes| bytes == b"\x1b[38;"));
    }
}
