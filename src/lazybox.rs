use std::io::{self, IsTerminal as _, Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::queue;
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, size};

use crate::terminal::Session;
use crate::{VERSION, json_string, use_color};

const HELP: &str = "One native dashboard for Docker, Apple containers, and Slurm\n\nUsage:\n  lazybox [open] [--backend auto|docker|apple|slurm]\n  lazybox snapshot [--backend auto|docker|apple|slurm] [--plain|--json]\n\nOptions:\n  --backend NAME Select or auto-detect one live backend\n  --plain        Stable tab-separated snapshot\n  --json         Machine-readable snapshot\n  -h, --help     Show this help\n  -V, --version  Show the version\n\nDashboard controls: Up/Down select, Tab backend, Enter details, l logs, s start/stop, x restart Docker, c cancel Slurm, r refresh, q quit.";
const MAX_OUTPUT: usize = 8 * 1024 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backend {
    Docker,
    Apple,
    Slurm,
}

impl Backend {
    const ALL: [Self; 3] = [Self::Docker, Self::Apple, Self::Slurm];

    fn name(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Apple => "apple",
            Self::Slurm => "slurm",
        }
    }

    fn parse(value: &str) -> Result<Option<Self>, String> {
        match value {
            "auto" => Ok(None),
            "docker" => Ok(Some(Self::Docker)),
            "apple" => Ok(Some(Self::Apple)),
            "slurm" => Ok(Some(Self::Slurm)),
            _ => Err(format!(
                "unknown backend {value:?}; use auto, docker, apple, or slurm"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Open,
    Snapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Plain,
    Json,
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    mode: Option<Mode>,
    backend: Option<Backend>,
    format: Option<Format>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    id: String,
    name: String,
    source: String,
    state: String,
    detail: String,
}

struct Snapshot {
    backend: Backend,
    rows: Vec<Row>,
}

#[derive(Clone, Copy)]
enum Action {
    Toggle,
    Restart,
    Cancel,
}

pub fn run(arguments: Vec<String>) -> Result<i32, String> {
    match arguments.as_slice() {
        [argument] if matches!(argument.as_str(), "-h" | "--help") => {
            println!("{HELP}");
            return Ok(0);
        }
        [argument] if matches!(argument.as_str(), "-V" | "--version") => {
            println!("lazybox {VERSION}");
            return Ok(0);
        }
        _ => {}
    }
    let options = parse_options(arguments)?;
    let mode = options.mode.unwrap_or_else(|| {
        if options.format.is_none() && io::stdin().is_terminal() && io::stdout().is_terminal() {
            Mode::Open
        } else {
            Mode::Snapshot
        }
    });
    if mode == Mode::Open && options.format.is_some() {
        return Err("--plain and --json are snapshot options".into());
    }
    if mode == Mode::Open && (!io::stdin().is_terminal() || !io::stdout().is_terminal()) {
        return Err("the dashboard needs a terminal; use 'lazybox snapshot'".into());
    }

    let snapshot = options.backend.map_or_else(auto_load, load)?;
    match mode {
        Mode::Open => interactive(snapshot),
        Mode::Snapshot => {
            if options.format == Some(Format::Json) {
                print_json(&snapshot);
            } else {
                print_plain(&snapshot);
            }
            Ok(0)
        }
    }
}

fn parse_options(arguments: Vec<String>) -> Result<Options, String> {
    let mut options = Options {
        mode: None,
        backend: None,
        format: None,
    };
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "open" if options.mode.is_none() => options.mode = Some(Mode::Open),
            "snapshot" if options.mode.is_none() => options.mode = Some(Mode::Snapshot),
            "--backend" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--backend needs a value".to_owned())?;
                options.backend = Backend::parse(&value)?;
            }
            "--plain" if options.format.is_none() => options.format = Some(Format::Plain),
            "--json" if options.format.is_none() => options.format = Some(Format::Json),
            "--plain" | "--json" => {
                return Err("--plain and --json cannot be used together".into());
            }
            _ => {
                return Err(format!(
                    "unknown argument {argument:?}; try 'lazybox --help'"
                ));
            }
        }
    }
    Ok(options)
}

fn auto_load() -> Result<Snapshot, String> {
    let mut errors = Vec::new();
    for backend in Backend::ALL {
        match load(backend) {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) => errors.push(format!("{}: {error}", backend.name())),
        }
    }
    Err(format!("no backend responded ({})", errors.join("; ")))
}

fn load(backend: Backend) -> Result<Snapshot, String> {
    let output = match backend {
        Backend::Docker => command_output(
            "docker",
            &[
                "container",
                "ls",
                "--all",
                "--no-trunc",
                "--format",
                "{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.State}}\t{{.Status}}",
            ],
            false,
        )?,
        Backend::Apple => {
            command_output("container", &["list", "--all", "--format", "table"], false)?
        }
        Backend::Slurm => command_output(
            "squeue",
            &["--noheader", "--format=%i|%j|%u|%t|%M|%N|%P"],
            false,
        )?,
    };
    let rows = match backend {
        Backend::Docker => parse_docker(&output),
        Backend::Apple => parse_apple(&output),
        Backend::Slurm => parse_slurm(&output),
    };
    Ok(Snapshot { backend, rows })
}

fn command_output(
    program: &str,
    arguments: &[&str],
    include_stderr: bool,
) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => format!("{program} is not installed"),
            _ => format!("cannot run {program}: {error}"),
        })?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        return Err(format!("cannot capture {program} output"));
    };
    let stdout = thread::spawn(move || read_limited(stdout, MAX_OUTPUT));
    let stderr = thread::spawn(move || read_limited(stderr, MAX_OUTPUT));
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("cannot wait for {program}: {error}"));
            }
        }
        if started.elapsed() >= COMMAND_TIMEOUT {
            timed_out = true;
            let _ = child.kill();
            break child
                .wait()
                .map_err(|error| format!("cannot stop {program}: {error}"));
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout
        .join()
        .map_err(|_| format!("cannot read {program} stdout"))?
        .map_err(|error| format!("cannot read {program} stdout: {error}"))?;
    let stderr = stderr
        .join()
        .map_err(|_| format!("cannot read {program} stderr"))?
        .map_err(|error| format!("cannot read {program} stderr: {error}"))?;
    if timed_out {
        status?;
        return Err(format!("{program} timed out after 30 seconds"));
    }
    let status = status?;
    if stdout.len().saturating_add(stderr.len()) > MAX_OUTPUT {
        return Err(format!("{program} output exceeds 8 MiB"));
    }
    if !status.success() {
        let error = clean(&String::from_utf8_lossy(&stderr));
        return Err(if error.is_empty() {
            format!("{program} exited with {status}")
        } else {
            format!("{program}: {error}")
        });
    }
    Ok(success_text(&stdout, &stderr, include_stderr))
}

fn read_limited(reader: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    reader
        .take(limit.saturating_add(1) as u64)
        .read_to_end(&mut output)?;
    Ok(output)
}

fn success_text(stdout: &[u8], stderr: &[u8], include_stderr: bool) -> String {
    let mut output = String::from_utf8_lossy(stdout).into_owned();
    if include_stderr && !stderr.is_empty() {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&String::from_utf8_lossy(stderr));
    }
    output
}

fn parse_docker(output: &str) -> Vec<Row> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let row = Row {
                id: clean(fields.next()?),
                name: clean(fields.next()?),
                source: clean(fields.next()?),
                state: clean(fields.next()?),
                detail: clean(fields.next()?),
            };
            fields.next().is_none().then_some(row)
        })
        .collect()
}

// ponytail: parse Apple's small stable table without a JSON dependency; switch to
// `--format json` only if Apple changes the columns.
fn parse_apple(output: &str) -> Vec<Row> {
    output
        .lines()
        .skip_while(|line| line.trim().is_empty())
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let id = fields.next()?;
            let source = fields.next()?;
            fields.next()?;
            fields.next()?;
            let state = fields.next()?;
            Some(Row {
                id: clean(id),
                name: clean(id),
                source: clean(source),
                state: clean(state),
                detail: clean(&fields.collect::<Vec<_>>().join(" ")),
            })
        })
        .collect()
}

fn parse_slurm(output: &str) -> Vec<Row> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('|');
            let id = fields.next()?;
            let name = fields.next()?;
            let user = fields.next()?;
            let state = fields.next()?;
            let time = fields.next()?;
            let node = fields.next()?;
            let source = fields.next()?;
            let row = Row {
                id: clean(id),
                name: clean(name),
                source: clean(source),
                state: clean(state),
                detail: clean(&format!("{user}  {time}  {node}")),
            };
            fields.next().is_none().then_some(row)
        })
        .collect()
}

fn clean(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(512)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.starts_with('-')
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'_' | b'-' | b'.' | b'[' | b']' | b'%' | b'+')
        })
}

fn print_plain(snapshot: &Snapshot) {
    println!("Backend\t{}", snapshot.backend.name());
    println!("ID\tName\tSource\tState\tDetail");
    for row in &snapshot.rows {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            row.id, row.name, row.source, row.state, row.detail
        );
    }
}

fn print_json(snapshot: &Snapshot) {
    let rows = snapshot
        .rows
        .iter()
        .map(|row| {
            format!(
                "{{\"id\":{},\"name\":{},\"source\":{},\"state\":{},\"detail\":{}}}",
                json_string(&row.id),
                json_string(&row.name),
                json_string(&row.source),
                json_string(&row.state),
                json_string(&row.detail)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "{{\"backend\":{},\"items\":[{rows}]}}",
        json_string(snapshot.backend.name())
    );
}

fn interactive(mut snapshot: Snapshot) -> Result<i32, String> {
    let _terminal = Session::enter().map_err(|error| format!("terminal: {error}"))?;
    let mut stdout = io::stdout();
    let mut selected = 0_usize;
    let mut details = Vec::new();
    let mut status = format!("{} items", snapshot.rows.len());
    let mut pending = None;
    loop {
        draw(
            &mut stdout,
            &snapshot,
            selected,
            &details,
            &status,
            pending,
            size().unwrap_or((80, 24)),
        )
        .map_err(|error| format!("draw: {error}"))?;

        match event::read().map_err(|error| format!("input: {error}"))? {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Char('c' | 'C'))
                    || matches!(key.code, KeyCode::Char('q' | 'Q'))
                {
                    break;
                }
                if let Some(action) = pending {
                    match key.code {
                        KeyCode::Char('y' | 'Y') => {
                            status = match perform(
                                action,
                                snapshot.backend,
                                selected_row(&snapshot, selected),
                            ) {
                                Ok(message) => match load(snapshot.backend) {
                                    Ok(next) => {
                                        snapshot = next;
                                        selected =
                                            selected.min(snapshot.rows.len().saturating_sub(1));
                                        details.clear();
                                        message
                                    }
                                    Err(error) => format!("{message}; refresh failed: {error}"),
                                },
                                Err(error) => error,
                            };
                            pending = None;
                        }
                        KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                            status = "Action cancelled".into();
                            pending = None;
                        }
                        _ => {}
                    }
                    continue;
                }
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected = selected.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = (selected + 1).min(snapshot.rows.len().saturating_sub(1));
                    }
                    KeyCode::Tab => match next_snapshot(snapshot.backend) {
                        Ok(next) => {
                            snapshot = next;
                            selected = 0;
                            details.clear();
                            status = format!("{} items", snapshot.rows.len());
                        }
                        Err(error) => status = error,
                    },
                    KeyCode::Char('r' | 'R') => match load(snapshot.backend) {
                        Ok(next) => {
                            snapshot = next;
                            selected = selected.min(snapshot.rows.len().saturating_sub(1));
                            status = format!("refreshed {} items", snapshot.rows.len());
                        }
                        Err(error) => status = error,
                    },
                    KeyCode::Enter => {
                        show_details(snapshot.backend, selected_row(&snapshot, selected), false)
                            .map(|value| details = value)
                            .unwrap_or_else(|error| status = error);
                    }
                    KeyCode::Char('l' | 'L') => {
                        show_details(snapshot.backend, selected_row(&snapshot, selected), true)
                            .map(|value| details = value)
                            .unwrap_or_else(|error| status = error);
                    }
                    KeyCode::Char('s' | 'S')
                        if matches!(snapshot.backend, Backend::Docker | Backend::Apple) =>
                    {
                        pending = selected_row(&snapshot, selected).map(|_| Action::Toggle);
                    }
                    KeyCode::Char('x' | 'X') if snapshot.backend == Backend::Docker => {
                        pending = selected_row(&snapshot, selected).map(|_| Action::Restart);
                    }
                    KeyCode::Char('c' | 'C') if snapshot.backend == Backend::Slurm => {
                        pending = selected_row(&snapshot, selected).map(|_| Action::Cancel);
                    }
                    _ => {}
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(0)
}

fn selected_row(snapshot: &Snapshot, selected: usize) -> Option<&Row> {
    snapshot.rows.get(selected)
}

fn next_snapshot(current: Backend) -> Result<Snapshot, String> {
    let index = Backend::ALL
        .iter()
        .position(|backend| *backend == current)
        .unwrap_or(0);
    let mut errors = Vec::new();
    for offset in 1..=Backend::ALL.len() {
        let backend = Backend::ALL[(index + offset) % Backend::ALL.len()];
        match load(backend) {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) => errors.push(format!("{}: {error}", backend.name())),
        }
    }
    Err(errors.join("; "))
}

fn perform(action: Action, backend: Backend, row: Option<&Row>) -> Result<String, String> {
    let row = row.ok_or_else(|| "nothing is selected".to_owned())?;
    if !safe_id(&row.id) {
        return Err("backend returned an unsafe item ID".into());
    }
    match (backend, action) {
        (Backend::Docker, Action::Toggle) => {
            let (verb, past) = if row.state.eq_ignore_ascii_case("running") {
                ("stop", "stopped")
            } else {
                ("start", "started")
            };
            command_output("docker", &["container", verb, &row.id], false)?;
            Ok(format!("{past} {}", row.name))
        }
        (Backend::Apple, Action::Toggle) => {
            let (verb, past) = if row.state.eq_ignore_ascii_case("running") {
                ("stop", "stopped")
            } else {
                ("start", "started")
            };
            command_output("container", &[verb, &row.id], false)?;
            Ok(format!("{past} {}", row.name))
        }
        (Backend::Docker, Action::Restart) => {
            command_output("docker", &["container", "restart", &row.id], false)?;
            Ok(format!("restarted {}", row.name))
        }
        (Backend::Slurm, Action::Cancel) => {
            command_output("scancel", &[&row.id], false)?;
            Ok(format!("cancelled {}", row.id))
        }
        _ => Err("that action is not available for this backend".into()),
    }
}

fn show_details(backend: Backend, row: Option<&Row>, logs: bool) -> Result<Vec<String>, String> {
    let row = row.ok_or_else(|| "nothing is selected".to_owned())?;
    if !safe_id(&row.id) {
        return Err("backend returned an unsafe item ID".into());
    }
    let output = match (backend, logs) {
        (Backend::Docker, false) => command_output("docker", &["inspect", &row.id], false)?,
        (Backend::Docker, true) => {
            command_output("docker", &["logs", "--tail", "40", &row.id], true)?
        }
        (Backend::Apple, false) => command_output("container", &["inspect", &row.id], false)?,
        (Backend::Apple, true) => {
            command_output("container", &["logs", "-n", "40", &row.id], true)?
        }
        (Backend::Slurm, false) => command_output("scontrol", &["show", "job", &row.id], false)?,
        (Backend::Slurm, true) => return Err("Slurm log paths are shown in job details".into()),
    };
    let lines = output.lines().map(clean).take(100).collect::<Vec<_>>();
    Ok(if lines.is_empty() {
        vec!["No output".into()]
    } else {
        lines
    })
}

fn action_name(action: Action, backend: Backend, row: Option<&Row>) -> &'static str {
    match (action, backend, row.map(|row| row.state.as_str())) {
        (Action::Toggle, _, Some(state)) if state.eq_ignore_ascii_case("running") => "stop",
        (Action::Toggle, _, _) => "start",
        (Action::Restart, _, _) => "restart",
        (Action::Cancel, _, _) => "cancel",
    }
}

fn draw<W: Write>(
    stdout: &mut W,
    snapshot: &Snapshot,
    selected: usize,
    details: &[String],
    status: &str,
    pending: Option<Action>,
    (width, height): (u16, u16),
) -> io::Result<()> {
    let color = use_color(false);
    queue!(
        stdout,
        BeginSynchronizedUpdate,
        MoveTo(0, 0),
        Clear(ClearType::All)
    )?;
    if width < 80 || height < 20 {
        write_line(
            stdout,
            0,
            height / 2,
            width,
            "Resize to at least 80 x 20",
            color.then_some(Color::Yellow),
        )?;
        queue!(stdout, EndSynchronizedUpdate)?;
        return stdout.flush();
    }

    write_line(
        stdout,
        0,
        1,
        width,
        &format!(
            "+-- LAZYBOX // {} --+",
            snapshot.backend.name().to_uppercase()
        ),
        color.then_some(Color::Cyan),
    )?;
    write_line(
        stdout,
        2,
        3,
        width,
        "  ID            NAME               SOURCE                 STATE       DETAIL",
        color.then_some(Color::Grey),
    )?;

    let visible = usize::from(height.saturating_sub(11).min(10));
    let start = selected.saturating_sub(visible.saturating_sub(1));
    for (offset, row) in snapshot.rows.iter().skip(start).take(visible).enumerate() {
        let index = start + offset;
        let line = format!(
            "{} {:14} {:18} {:22} {:11} {}",
            if index == selected { '>' } else { ' ' },
            fit(&row.id, 14),
            fit(&row.name, 18),
            fit(&row.source, 22),
            fit(&row.state, 11),
            row.detail
        );
        write_line(
            stdout,
            2,
            4 + offset as u16,
            width,
            &line,
            color.then_some(if index == selected {
                Color::Green
            } else {
                Color::White
            }),
        )?;
    }

    let detail_row = 5 + visible as u16;
    write_line(
        stdout,
        2,
        detail_row,
        width,
        "DETAIL",
        color.then_some(Color::Cyan),
    )?;
    for (offset, line) in details
        .iter()
        .take(usize::from(height.saturating_sub(detail_row + 4)))
        .enumerate()
    {
        write_line(
            stdout,
            2,
            detail_row + 1 + offset as u16,
            width,
            line,
            color.then_some(Color::White),
        )?;
    }

    let status = pending.map_or_else(
        || status.to_owned(),
        |action| {
            format!(
                "Confirm {} {}? y/n",
                action_name(action, snapshot.backend, selected_row(snapshot, selected)),
                selected_row(snapshot, selected).map_or("", |row| row.id.as_str())
            )
        },
    );
    write_line(
        stdout,
        2,
        height - 2,
        width,
        &status,
        color.then_some(if pending.is_some() {
            Color::Yellow
        } else {
            Color::Grey
        }),
    )?;
    write_line(
        stdout,
        2,
        height - 1,
        width,
        "Up/Down select  Tab backend  Enter details  l logs  s start/stop  x restart  c cancel  r refresh  q quit",
        color.then_some(Color::Grey),
    )?;
    queue!(stdout, EndSynchronizedUpdate)?;
    stdout.flush()
}

fn fit(value: &str, width: usize) -> String {
    let value = value.chars().take(width).collect::<String>();
    format!("{value:<width$}")
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

    #[test]
    fn parses_each_backend_and_removes_terminal_controls() {
        assert_eq!(
            parse_docker("abc\tweb\tnginx\trunning\tUp 2 minutes\n")[0].name,
            "web"
        );
        assert_eq!(
            parse_apple("ID IMAGE OS ARCH STATE ADDR\nweb alpine linux arm64 running 10.0.0.2\n")
                [0]
            .state,
            "running"
        );
        assert_eq!(
            parse_slurm("42|train|keys|R|01:02|node1|gpu\n")[0].source,
            "gpu"
        );
        assert_eq!(clean("safe\x1b[2J text\n"), "safe[2J text");
        assert_eq!(success_text(b"", b"stderr log\n", true), "stderr log\n");
        assert_eq!(
            read_limited(std::io::Cursor::new(b"12345"), 3).expect("bounded read"),
            b"1234"
        );

        let snapshot = Snapshot {
            backend: Backend::Docker,
            rows: parse_docker("abc\tweb\tnginx\trunning\tUp\n"),
        };
        let mut screen = Vec::new();
        draw(&mut screen, &snapshot, 0, &[], "ready", None, (80, 20)).expect("render dashboard");
        assert!(String::from_utf8_lossy(&screen).contains("STATE       DETAIL"));
    }

    #[test]
    fn validates_cli_and_backend_ids() {
        let options = parse_options(vec![
            "snapshot".into(),
            "--backend".into(),
            "slurm".into(),
            "--json".into(),
        ])
        .expect("valid options");
        assert_eq!(options.mode, Some(Mode::Snapshot));
        assert_eq!(options.backend, Some(Backend::Slurm));
        assert_eq!(options.format, Some(Format::Json));
        assert!(parse_options(vec!["--plain".into(), "--json".into()]).is_err());
        assert!(Backend::parse("missing").is_err());
        assert!(safe_id("123_[1-4%2]"));
        assert!(!safe_id("--all"));
        assert!(!safe_id("bad id"));
    }
}
