use std::collections::HashSet;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{VERSION, json_string, use_color};

const HELP: &str = "One launcher for terminal games\n\nUsage:\n  games [list] [--plain | --json]\n  games info NAME\n  games run NAME [ARG ...]\n\nOptions:\n  --plain     Stable text without decoration\n  --json      Machine-readable game list\n  -h, --help  Show this help\n  -V, --version  Show the version\n\nExtra .game manifests are read from TOOLS_GAME_PATH and the user data directory.";
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_MANIFESTS: usize = 256;

#[derive(Clone, Debug, PartialEq)]
struct Game {
    name: String,
    command: String,
    summary: String,
    source: String,
    license: String,
}

const BUILTINS: &[(&str, &str, &str, &str, &str)] = &[
    (
        "balatro",
        "balatro_tui",
        "Deck-building roguelike",
        "https://github.com/Passeriform/BalatroTUI",
        "GPL-3.0",
    ),
    (
        "botany",
        "botany.py",
        "Grow a persistent terminal plant",
        "https://github.com/jifunks/botany",
        "ISC",
    ),
    (
        "brogue",
        "brogue",
        "Classic single-player roguelike",
        "https://github.com/tmewett/BrogueCE",
        "AGPL-3.0",
    ),
    (
        "pokete",
        "pokete",
        "Creature-catching terminal game",
        "https://github.com/lxgr-linux/pokete",
        "GPL-3.0",
    ),
    (
        "rebels",
        "rebels",
        "Space-pirate basketball",
        "https://github.com/ricott1/rebels-in-the-sky",
        "GPL-3.0",
    ),
    (
        "snake",
        "snake",
        "Minimal terminal Snake",
        "https://github.com/wick3dr0se/snake",
        "GPL-3.0",
    ),
    (
        "tttui",
        "tttui",
        "Terminal typing test",
        "https://github.com/reidoboss/tttui",
        "MIT",
    ),
    (
        "2048",
        "tui-2048",
        "Terminal 2048 puzzle",
        "https://github.com/ps06756/tui-2048",
        "MIT",
    ),
    (
        "wordle",
        "wordle.raku",
        "Raku Wordle implementation",
        "https://github.com/m-dango/raku-wordle",
        "Artistic-2.0",
    ),
    (
        "raycaster",
        "awkaster.awk",
        "gawk raycasting demo",
        "https://github.com/TheMozg/awk-raycaster",
        "MIT",
    ),
];

pub fn run(arguments: Vec<String>) -> Result<i32, String> {
    let mut arguments = arguments.into_iter();
    match arguments.next().as_deref() {
        None | Some("list") => list(arguments.collect()),
        Some("info") => {
            let name = required_name(arguments.next())?;
            if arguments.next().is_some() {
                return Err("info accepts one game name".into());
            }
            info(&name)
        }
        Some("run") => {
            let name = required_name(arguments.next())?;
            launch(&name, arguments.collect())
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

fn required_name(name: Option<String>) -> Result<String, String> {
    name.filter(|value| valid_name(value))
        .ok_or_else(|| "enter a valid game name".into())
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

    let games = registry()?;
    if json {
        println!("{}", render_json(&games));
    } else if use_color(plain) {
        print_styled(&games);
    } else {
        print_plain(&games);
    }
    Ok(0)
}

fn info(name: &str) -> Result<i32, String> {
    let game = find_game(name)?;
    println!("Name\t{}", game.name);
    println!("Command\t{}", game.command);
    println!(
        "Status\t{}",
        if find_command(&game.command).is_some() {
            "ready"
        } else {
            "missing"
        }
    );
    println!("License\t{}", game.license);
    println!("Source\t{}", game.source);
    println!("Summary\t{}", game.summary);
    Ok(0)
}

fn launch(name: &str, arguments: Vec<String>) -> Result<i32, String> {
    let game = find_game(name)?;
    let executable = find_command(&game.command).ok_or_else(|| {
        format!(
            "{} is not installed (expected {:?}); see {}",
            game.name, game.command, game.source
        )
    })?;
    let status = Command::new(&executable)
        .args(arguments)
        .status()
        .map_err(|error| format!("cannot launch {}: {error}", executable.display()))?;
    Ok(status.code().unwrap_or(1))
}

fn find_game(name: &str) -> Result<Game, String> {
    registry()?
        .into_iter()
        .find(|game| game.name == name)
        .ok_or_else(|| format!("unknown game {name:?}; run 'games list'"))
}

fn registry() -> Result<Vec<Game>, String> {
    let mut games = BUILTINS
        .iter()
        .map(|&(name, command, summary, source, license)| Game {
            name: name.into(),
            command: command.into(),
            summary: summary.into(),
            source: source.into(),
            license: license.into(),
        })
        .collect::<Vec<_>>();
    let mut names = games
        .iter()
        .map(|game| game.name.clone())
        .collect::<HashSet<_>>();
    let mut manifest_count = 0;

    for directory in manifest_directories() {
        if !directory.exists() {
            continue;
        }
        let mut paths = fs::read_dir(&directory)
            .map_err(|error| format!("cannot read {}: {error}", directory.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension() == Some(OsStr::new("game")))
            .collect::<Vec<_>>();
        paths.sort_unstable();
        for path in paths {
            manifest_count += 1;
            if manifest_count > MAX_MANIFESTS {
                return Err(format!("more than {MAX_MANIFESTS} game manifests found"));
            }
            let game = read_manifest(&path)?;
            if !names.insert(game.name.clone()) {
                return Err(format!(
                    "duplicate game name {:?} in {}",
                    game.name,
                    path.display()
                ));
            }
            games.push(game);
        }
    }
    games.sort_unstable_by(|left, right| left.name.cmp(&right.name));
    Ok(games)
}

fn manifest_directories() -> Vec<PathBuf> {
    let mut directories: Vec<PathBuf> = env::var_os("TOOLS_GAME_PATH")
        .map(|value| {
            env::split_paths(&value)
                .filter(|path| !path.as_os_str().is_empty())
                .collect()
        })
        .unwrap_or_default();
    if let Some(path) = env::var_os("XDG_DATA_HOME") {
        directories.push(PathBuf::from(path).join("keys-tools/games"));
    } else if let Some(home) = env::var_os("HOME") {
        directories.push(PathBuf::from(home).join(".local/share/keys-tools/games"));
    }
    directories
}

fn read_manifest(path: &Path) -> Result<Game, String> {
    let before = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    if before.file_type().is_symlink() || !before.is_file() {
        return Err(format!("unsafe game manifest: {}", path.display()));
    }
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    };
    #[cfg(not(windows))]
    let file = fs::File::open(path);
    let file = file.map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let opened = file
        .metadata()
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if before.dev() != opened.dev() || before.ino() != opened.ino() {
            return Err(format!("game manifest changed: {}", path.display()));
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if opened.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(format!("unsafe game manifest: {}", path.display()));
        }
    }
    if !opened.is_file() || opened.len() > MAX_MANIFEST_BYTES {
        return Err(format!("unsafe game manifest: {}", path.display()));
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize + 1);
    file.take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(format!("unsafe game manifest: {}", path.display()));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("{}: manifest must be UTF-8", path.display()))?;
    parse_manifest(&text).map_err(|error| format!("{}: {error}", path.display()))
}

fn parse_manifest(text: &str) -> Result<Game, String> {
    if text
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r'))
    {
        return Err("manifest contains control characters".into());
    }
    let mut values = std::collections::HashMap::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {} must be key=value", index + 1))?;
        let key = key.trim();
        let value = value.trim();
        if !matches!(key, "name" | "command" | "summary" | "source" | "license") {
            return Err(format!("unknown key {key:?}"));
        }
        if value.is_empty() || value.contains(['\0', '\n', '\r']) {
            return Err(format!("{key} must be one non-empty line"));
        }
        if values.insert(key, value).is_some() {
            return Err(format!("duplicate key {key:?}"));
        }
    }
    let get = |key| {
        values
            .get(key)
            .copied()
            .ok_or_else(|| format!("missing {key}"))
    };
    let name = get("name")?;
    let command = get("command")?;
    let summary = get("summary")?;
    let source = get("source")?;
    let license = get("license")?;
    if !valid_name(name) {
        return Err("name must contain lowercase letters, digits, and hyphens".into());
    }
    if command.chars().any(char::is_whitespace) || command.len() > 512 {
        return Err("command must be one executable without arguments".into());
    }
    if summary.len() > 200 || license.len() > 64 {
        return Err("summary or license is too long".into());
    }
    if !source.starts_with("https://") || source.len() > 512 {
        return Err("source must be an https URL".into());
    }
    Ok(Game {
        name: name.into(),
        command: command.into(),
        summary: summary.into(),
        source: source.into(),
        license: license.into(),
    })
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn find_command(command: &str) -> Option<PathBuf> {
    let candidate = Path::new(command);
    if candidate.components().count() > 1 {
        return executable(candidate).then(|| candidate.to_owned());
    }
    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path).filter(|path| !path.as_os_str().is_empty()) {
        let candidate = directory.join(command);
        if executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = directory.join(format!("{command}.exe"));
            if executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(windows)]
fn executable(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(OsStr::new("exe")))
        && path.is_file()
}

#[cfg(not(any(unix, windows)))]
fn executable(path: &Path) -> bool {
    path.is_file()
}

fn print_plain(games: &[Game]) {
    for game in games {
        let status = if find_command(&game.command).is_some() {
            "ready"
        } else {
            "missing"
        };
        println!("{}\t{status}\t{}", game.name, game.summary);
    }
}

fn print_styled(games: &[Game]) {
    const CYAN: &str = "\x1b[38;2;125;207;255m";
    const GREEN: &str = "\x1b[38;2;158;206;106m";
    const MUTED: &str = "\x1b[38;2;86;95;137m";
    const RESET: &str = "\x1b[0m";
    println!("{CYAN}╭─ ◈ GAMES {MUTED}// terminal arcade{RESET}");
    for game in games {
        let (marker, color) = if find_command(&game.command).is_some() {
            ("●", GREEN)
        } else {
            ("○", MUTED)
        };
        println!(
            "{CYAN}│{RESET} {color}{marker}{RESET} {:12} {}",
            game.name, game.summary
        );
    }
    println!("{CYAN}╰─{RESET} {MUTED}games run NAME{RESET}");
}

fn render_json(games: &[Game]) -> String {
    let values = games
        .iter()
        .map(|game| {
            format!(
                "{{\"name\":{},\"command\":{},\"summary\":{},\"source\":{},\"license\":{},\"available\":{}}}",
                json_string(&game.name),
                json_string(&game.command),
                json_string(&game.summary),
                json_string(&game.source),
                json_string(&game.license),
                find_command(&game.command).is_some(),
            )
        })
        .collect::<Vec<_>>();
    format!("[{}]", values.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "name=maze\ncommand=maze\nsummary=Small maze game\nsource=https://example.com/maze\nlicense=MIT\n";

    #[test]
    fn parses_strict_manifest() {
        let game = parse_manifest(MANIFEST).expect("valid manifest");
        assert_eq!(game.name, "maze");
        assert_eq!(game.command, "maze");
        assert!(parse_manifest(&format!("{MANIFEST}unknown=x\n")).is_err());
        assert!(parse_manifest(&MANIFEST.replace("command=maze", "command=sh -c bad")).is_err());
        assert!(parse_manifest(&MANIFEST.replace("name=maze", "name=../maze")).is_err());
        assert!(parse_manifest(&MANIFEST.replace("Small maze", "Small\x1b[2Jmaze")).is_err());
    }

    #[test]
    fn rejects_oversized_manifest() {
        let path = env::temp_dir().join(format!("keys-tools-large-{}.game", std::process::id()));
        fs::write(&path, vec![b'x'; MAX_MANIFEST_BYTES as usize + 1]).expect("write manifest");
        let result = read_manifest(&path);
        let _ = fs::remove_file(path);
        assert!(result.is_err());
    }

    #[cfg(windows)]
    #[test]
    fn rejects_batch_launchers() {
        let path = env::temp_dir().join(format!("keys-tools-{}.cmd", std::process::id()));
        fs::write(&path, "@echo off\n").expect("write batch file");
        assert!(!executable(&path));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn json_exposes_license_and_availability() {
        let game = parse_manifest(MANIFEST).expect("valid manifest");
        let json = render_json(&[game]);
        assert!(json.contains("\"name\":\"maze\""));
        assert!(json.contains("\"license\":\"MIT\""));
        assert!(json.contains("\"available\":"));
    }
}
