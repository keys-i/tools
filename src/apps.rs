use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, IsTerminal as _, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::queue;
use crossterm::style::Print;
use crossterm::terminal::{BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, size};

use crate::terminal::Session;
use crate::{VERSION, human_text, json_string};

const HELP: &str = "Fast native local terminal apps\n\nUsage:\n  apps [open|snapshot] read FILE [--find TEXT] [--limit N]\n  apps [open|snapshot] table FILE [--delimiter comma|tab] [--limit N]\n  apps [open|snapshot] slides FILE [--page N]\n  apps [open|snapshot] paint [FILE] [--width N --height N] [--output FILE]\n  apps [open|snapshot] timer --seconds N [--elapsed N] [--label TEXT]\n\nOptions:\n  --find TEXT          Find text in read mode\n  --delimiter NAME     comma or tab; inferred from .tsv otherwise\n  --limit N            Snapshot rows or lines (1..1000, default 20)\n  --page N             Slide number, starting at 1\n  --width N            New paint canvas width (1..240, default 40)\n  --height N           New paint canvas height (1..80, default 16)\n  --output FILE        New paint file; existing files are never overwritten\n  --seconds N          Timer duration (1..86400)\n  --elapsed N          Deterministic snapshot progress in seconds\n  --label TEXT         Timer label\n  --plain              Stable snapshot text\n  --json               Machine-readable snapshot\n  -h, --help           Show this help\n  -V, --version        Show the version\n\nControls: q quits; arrows navigate; PageUp/PageDown jump; Home/End move to edges. Slides use Left/Right. Paint uses a printable key or Space to draw, Backspace to erase, and s to save. Timer uses Space to pause and r to reset.";

const MAX_INPUT_BYTES: u64 = 32 * 1024 * 1024;
const MAX_LINES: usize = 200_000;
const MAX_LINE_BYTES: usize = 64 * 1024;
const MAX_ROWS: usize = 100_000;
const MAX_COLUMNS: usize = 256;
const MAX_CELLS: usize = 2_000_000;
const MAX_FIELD_BYTES: usize = 64 * 1024;
const MAX_SLIDES: usize = 512;
const MAX_PAINT_WIDTH: usize = 240;
const MAX_PAINT_HEIGHT: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Open,
    Snapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Read,
    Table,
    Slides,
    Paint,
    Timer,
}

impl View {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "read" => Ok(Self::Read),
            "table" => Ok(Self::Table),
            "slides" => Ok(Self::Slides),
            "paint" => Ok(Self::Paint),
            "timer" => Ok(Self::Timer),
            _ => Err(format!(
                "unknown app {value:?}; expected read, table, slides, paint, or timer"
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Table => "table",
            Self::Slides => "slides",
            Self::Paint => "paint",
            Self::Timer => "timer",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Plain,
    Json,
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    action: Action,
    view: View,
    input: Option<PathBuf>,
    format: Format,
    find: Option<String>,
    delimiter: Option<char>,
    limit: usize,
    page: usize,
    width: usize,
    height: usize,
    output: Option<PathBuf>,
    seconds: u64,
    elapsed: u64,
    label: String,
}

pub fn run(arguments: Vec<String>) -> Result<i32, String> {
    match arguments.as_slice() {
        [] => {
            write_stdout(HELP)?;
            return Ok(0);
        }
        [argument] if matches!(argument.as_str(), "-h" | "--help") => {
            write_stdout(HELP)?;
            return Ok(0);
        }
        [argument] if matches!(argument.as_str(), "-V" | "--version") => {
            write_stdout(&format!("apps {VERSION}"))?;
            return Ok(0);
        }
        _ => {}
    }

    let terminal = io::stdin().is_terminal() && io::stdout().is_terminal();
    let options = parse_options(arguments, terminal)?;
    if options.action == Action::Open && !terminal {
        return Err("interactive apps need a terminal; use 'apps snapshot'".into());
    }
    if options.action == Action::Open && options.input.as_deref() == Some(Path::new("-")) {
        return Err("interactive input cannot also be stdin; use a file or snapshot '-'".into());
    }

    match (options.action, options.view) {
        (Action::Snapshot, View::Read) => {
            snapshot_read(load_text(required_input(&options)?)?, &options)
        }
        (Action::Snapshot, View::Table) => snapshot_table(
            load_table(required_input(&options)?, options.delimiter)?,
            &options,
        ),
        (Action::Snapshot, View::Slides) => {
            snapshot_slides(load_slides(required_input(&options)?)?, &options)
        }
        (Action::Snapshot, View::Paint) => snapshot_paint(
            load_paint(options.input.as_deref(), options.width, options.height)?,
            &options,
        ),
        (Action::Snapshot, View::Timer) => snapshot_timer(&options),
        (Action::Open, View::Read) => open_read(load_text(required_input(&options)?)?, &options),
        (Action::Open, View::Table) => {
            open_table(load_table(required_input(&options)?, options.delimiter)?)
        }
        (Action::Open, View::Slides) => {
            open_slides(load_slides(required_input(&options)?)?, options.page)
        }
        (Action::Open, View::Paint) => open_paint(
            load_paint(options.input.as_deref(), options.width, options.height)?,
            options.output.as_deref(),
        ),
        (Action::Open, View::Timer) => open_timer(options.seconds, &options.label),
    }
}

fn parse_options(arguments: Vec<String>, terminal: bool) -> Result<Options, String> {
    let mut action = None;
    let mut view = None;
    let mut input = None;
    let mut format = None;
    let mut find = None;
    let mut delimiter = None;
    let mut limit = 20_usize;
    let mut limit_set = false;
    let mut page = 1_usize;
    let mut page_set = false;
    let mut width = 40_usize;
    let mut width_set = false;
    let mut height = 16_usize;
    let mut height_set = false;
    let mut output = None;
    let mut seconds = None;
    let mut elapsed = 0_u64;
    let mut elapsed_set = false;
    let mut label = "focus".to_owned();
    let mut label_set = false;
    let mut arguments = arguments.into_iter();

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "open" if action.is_none() && view.is_none() => action = Some(Action::Open),
            "snapshot" if action.is_none() && view.is_none() => action = Some(Action::Snapshot),
            value @ ("read" | "table" | "slides" | "paint" | "timer") if view.is_none() => {
                view = Some(View::parse(value)?);
            }
            "--find" if find.is_none() => {
                let value = arguments.next().ok_or("--find needs text")?;
                if value.is_empty() {
                    return Err("--find text cannot be empty".into());
                }
                find = Some(value);
            }
            "--delimiter" if delimiter.is_none() => {
                delimiter = Some(match arguments.next().as_deref() {
                    Some("comma") => ',',
                    Some("tab") => '\t',
                    _ => return Err("--delimiter must be comma or tab".into()),
                });
            }
            "--limit" if !limit_set => {
                limit = number(&mut arguments, "--limit", 1, 1000)? as usize;
                limit_set = true;
            }
            "--page" if !page_set => {
                page = number(&mut arguments, "--page", 1, MAX_SLIDES as u64)? as usize;
                page_set = true;
            }
            "--width" if !width_set => {
                width = number(&mut arguments, "--width", 1, MAX_PAINT_WIDTH as u64)? as usize;
                width_set = true;
            }
            "--height" if !height_set => {
                height = number(&mut arguments, "--height", 1, MAX_PAINT_HEIGHT as u64)? as usize;
                height_set = true;
            }
            "--output" if output.is_none() => {
                output = Some(PathBuf::from(
                    arguments.next().ok_or("--output needs a file")?,
                ));
            }
            "--seconds" if seconds.is_none() => {
                seconds = Some(number(&mut arguments, "--seconds", 1, 86_400)?);
            }
            "--elapsed" if !elapsed_set => {
                elapsed = number(&mut arguments, "--elapsed", 0, 86_400)?;
                elapsed_set = true;
            }
            "--label" if !label_set => {
                label = arguments.next().ok_or("--label needs text")?;
                if label.trim().is_empty() || label.chars().any(char::is_control) {
                    return Err("--label must contain printable text".into());
                }
                label_set = true;
            }
            "--plain" if format.is_none() => format = Some(Format::Plain),
            "--json" if format.is_none() => format = Some(Format::Json),
            "--plain" | "--json" => return Err("--plain and --json cannot be combined".into()),
            value if value.starts_with('-') && value != "-" => {
                return Err(format!("unknown option {value:?}; try 'apps --help'"));
            }
            value if input.is_none() => input = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument {value:?}")),
        }
    }

    let view = view.ok_or("choose read, table, slides, paint, or timer")?;
    let action = action.unwrap_or(if terminal && format.is_none() {
        Action::Open
    } else {
        Action::Snapshot
    });
    if action == Action::Open && format.is_some() {
        return Err("--plain and --json are snapshot options".into());
    }
    if action == Action::Open && limit_set {
        return Err("--limit is only valid with snapshot read or table".into());
    }
    if matches!(view, View::Read | View::Table | View::Slides) && input.is_none() {
        return Err(format!("{} needs an input file", view.name()));
    }
    if view == View::Timer && input.is_some() {
        return Err("timer does not accept an input file".into());
    }
    if view != View::Read && find.is_some() {
        return Err("--find is only valid with read".into());
    }
    if view != View::Table && delimiter.is_some() {
        return Err("--delimiter is only valid with table".into());
    }
    if !matches!(view, View::Read | View::Table) && limit_set {
        return Err("--limit is only valid with read or table".into());
    }
    if view != View::Slides && page_set {
        return Err("--page is only valid with slides".into());
    }
    if view != View::Paint && (width_set || height_set || output.is_some()) {
        return Err("--width, --height, and --output are only valid with paint".into());
    }
    if view == View::Paint && input.is_some() && (width_set || height_set) {
        return Err("--width and --height only apply to a new paint canvas".into());
    }
    if action == Action::Snapshot && output.is_some() {
        return Err("--output is only valid with open paint".into());
    }
    if view != View::Timer && (seconds.is_some() || elapsed_set || label_set) {
        return Err("--seconds, --elapsed, and --label are only valid with timer".into());
    }
    let seconds = if view == View::Timer {
        seconds.ok_or("timer needs --seconds")?
    } else {
        1
    };
    if elapsed > seconds {
        return Err("--elapsed cannot exceed --seconds".into());
    }
    if action == Action::Open && elapsed_set {
        return Err("--elapsed is only valid with snapshot timer".into());
    }

    Ok(Options {
        action,
        view,
        input,
        format: format.unwrap_or(Format::Plain),
        find,
        delimiter,
        limit,
        page,
        width,
        height,
        output,
        seconds,
        elapsed,
        label,
    })
}

fn number(
    arguments: &mut impl Iterator<Item = String>,
    flag: &str,
    minimum: u64,
    maximum: u64,
) -> Result<u64, String> {
    let value = arguments
        .next()
        .ok_or_else(|| format!("{flag} needs a number"))?
        .parse::<u64>()
        .map_err(|_| format!("{flag} needs an integer from {minimum} to {maximum}"))?;
    if !(minimum..=maximum).contains(&value) {
        return Err(format!("{flag} must be between {minimum} and {maximum}"));
    }
    Ok(value)
}

fn required_input(options: &Options) -> Result<&Path, String> {
    options
        .input
        .as_deref()
        .ok_or_else(|| format!("{} needs an input file", options.view.name()))
}

fn write_stdout(value: &str) -> Result<i32, String> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{value}").map_err(|error| format!("cannot write output: {error}"))?;
    Ok(0)
}

fn human_path(path: &Path) -> String {
    human_text(&path.to_string_lossy())
}

fn read_input(path: &Path) -> Result<(String, Vec<u8>), String> {
    let mut bytes = Vec::new();
    let name;
    if path == Path::new("-") {
        name = "stdin".to_owned();
        io::stdin()
            .lock()
            .take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("cannot read stdin: {error}"))?;
    } else {
        let file = File::open(path)
            .map_err(|error| format!("cannot open {}: {error}", human_path(path)))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("cannot inspect {}: {error}", human_path(path)))?;
        if !metadata.is_file() {
            return Err(format!("input is not a file: {}", human_path(path)));
        }
        if metadata.len() > MAX_INPUT_BYTES {
            return Err("input exceeds the 32 MiB limit".into());
        }
        bytes.reserve(metadata.len() as usize + 1);
        file.take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("cannot read {}: {error}", human_path(path)))?;
        name = human_path(path);
    }
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err("input exceeds the 32 MiB limit".into());
    }
    Ok((name, bytes))
}

struct TextDocument {
    name: String,
    bytes: usize,
    lines: Vec<String>,
}

fn load_text(path: &Path) -> Result<TextDocument, String> {
    let (name, bytes) = read_input(path)?;
    let bytes_len = bytes.len();
    let text = String::from_utf8(bytes).map_err(|_| "input is not valid UTF-8 text")?;
    let mut lines = Vec::new();
    for line in text.lines() {
        if line.len() > MAX_LINE_BYTES {
            return Err("input contains a line longer than 64 KiB".into());
        }
        if line
            .chars()
            .any(|character| character.is_control() && character != '\t')
        {
            return Err("input contains an unsupported control character".into());
        }
        if lines.len() >= MAX_LINES {
            return Err("input exceeds the 200,000-line limit".into());
        }
        lines.push(line.trim_end_matches('\r').to_owned());
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    Ok(TextDocument {
        name,
        bytes: bytes_len,
        lines,
    })
}

fn matches(document: &TextDocument, query: Option<&str>) -> Vec<usize> {
    let Some(query) = query else {
        return Vec::new();
    };
    let query = query.to_lowercase();
    document
        .lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line.to_lowercase().contains(&query).then_some(index))
        .collect()
}

fn snapshot_read(document: TextDocument, options: &Options) -> Result<i32, String> {
    let found = matches(&document, options.find.as_deref());
    let visible = document
        .lines
        .iter()
        .take(options.limit)
        .collect::<Vec<_>>();
    match options.format {
        Format::Plain => {
            let mut output = format!(
                "READ\t{}\t{} bytes\t{} lines\t{} matches",
                document.name,
                document.bytes,
                document.lines.len(),
                found.len()
            );
            for (index, line) in visible.into_iter().enumerate() {
                output.push_str(&format!("\n{}\t{}", index + 1, line));
            }
            write_stdout(&output)
        }
        Format::Json => {
            let lines = visible
                .into_iter()
                .map(|line| json_string(line))
                .collect::<Vec<_>>()
                .join(",");
            write_stdout(&format!(
                "{{\"app\":\"read\",\"source\":{},\"bytes\":{},\"lineCount\":{},\"matchCount\":{},\"lines\":[{}]}}",
                json_string(&document.name),
                document.bytes,
                document.lines.len(),
                found.len(),
                lines
            ))
        }
    }
}

struct Table {
    name: String,
    delimiter: char,
    rows: Vec<Vec<String>>,
    columns: usize,
}

fn load_table(path: &Path, delimiter: Option<char>) -> Result<Table, String> {
    let (name, bytes) = read_input(path)?;
    let text = String::from_utf8(bytes).map_err(|_| "table is not valid UTF-8 text")?;
    let delimiter = delimiter.unwrap_or_else(|| {
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("tsv"))
        {
            '\t'
        } else {
            ','
        }
    });
    let rows = parse_delimited(&text, delimiter)?;
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return Err("table has no columns".into());
    }
    Ok(Table {
        name,
        delimiter,
        rows,
        columns,
    })
}

fn parse_delimited(text: &str, delimiter: char) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut after_quote = false;
    let mut cells = 0_usize;
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        if character.is_control() && !matches!(character, '\n' | '\r' | '\t') {
            return Err("table contains an unsupported control character".into());
        }
        if quoted {
            if character == '"' {
                if characters.peek() == Some(&'"') {
                    characters.next();
                    field.push('"');
                } else {
                    quoted = false;
                    after_quote = true;
                }
            } else {
                field.push(character);
            }
        } else if after_quote {
            if character == delimiter {
                push_field(&mut row, &mut field)?;
                after_quote = false;
            } else if character == '\n' || character == '\r' {
                if character == '\r' && characters.peek() == Some(&'\n') {
                    characters.next();
                }
                push_field(&mut row, &mut field)?;
                push_row(&mut rows, &mut row, &mut cells)?;
                after_quote = false;
            } else {
                return Err("table has characters after a closing quote".into());
            }
        } else if character == '"' && field.is_empty() {
            quoted = true;
        } else if character == delimiter {
            push_field(&mut row, &mut field)?;
        } else if character == '\n' || character == '\r' {
            if character == '\r' && characters.peek() == Some(&'\n') {
                characters.next();
            }
            push_field(&mut row, &mut field)?;
            push_row(&mut rows, &mut row, &mut cells)?;
        } else if character == '"' {
            return Err("table has a quote inside an unquoted field".into());
        } else {
            field.push(character);
        }
        if field.len() > MAX_FIELD_BYTES {
            return Err("table field exceeds the 64 KiB limit".into());
        }
    }
    if quoted {
        return Err("table has an unterminated quoted field".into());
    }
    if after_quote || !field.is_empty() || !row.is_empty() {
        push_field(&mut row, &mut field)?;
        push_row(&mut rows, &mut row, &mut cells)?;
    }
    if rows.is_empty() {
        return Err("table is empty".into());
    }
    Ok(rows)
}

fn push_field(row: &mut Vec<String>, field: &mut String) -> Result<(), String> {
    if row.len() >= MAX_COLUMNS {
        return Err("table exceeds the 256-column limit".into());
    }
    row.push(std::mem::take(field));
    Ok(())
}

fn push_row(
    rows: &mut Vec<Vec<String>>,
    row: &mut Vec<String>,
    cells: &mut usize,
) -> Result<(), String> {
    if rows.len() >= MAX_ROWS {
        return Err("table exceeds the 100,000-row limit".into());
    }
    *cells = cells
        .checked_add(row.len())
        .filter(|cells| *cells <= MAX_CELLS)
        .ok_or("table exceeds the 2,000,000-cell limit")?;
    rows.push(std::mem::take(row));
    Ok(())
}

fn snapshot_table(table: Table, options: &Options) -> Result<i32, String> {
    let visible = table.rows.iter().take(options.limit).collect::<Vec<_>>();
    match options.format {
        Format::Plain => {
            let mut output = format!(
                "TABLE\t{}\t{} rows\t{} columns\t{}",
                table.name,
                table.rows.len().saturating_sub(1),
                table.columns,
                if table.delimiter == '\t' {
                    "tab"
                } else {
                    "comma"
                }
            );
            for row in visible {
                output.push('\n');
                output.push_str(
                    &row.iter()
                        .map(|field| {
                            field
                                .replace('\t', "\\t")
                                .replace('\n', "\\n")
                                .replace('\r', "\\r")
                        })
                        .collect::<Vec<_>>()
                        .join("\t"),
                );
            }
            write_stdout(&output)
        }
        Format::Json => {
            let rows = visible
                .into_iter()
                .map(|row| {
                    format!(
                        "[{}]",
                        row.iter()
                            .map(|field| json_string(field))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            write_stdout(&format!(
                "{{\"app\":\"table\",\"source\":{},\"rowCount\":{},\"columnCount\":{},\"delimiter\":{},\"rows\":[{}]}}",
                json_string(&table.name),
                table.rows.len().saturating_sub(1),
                table.columns,
                json_string(if table.delimiter == '\t' {
                    "tab"
                } else {
                    "comma"
                }),
                rows
            ))
        }
    }
}

struct Slides {
    name: String,
    pages: Vec<Vec<String>>,
}

fn load_slides(path: &Path) -> Result<Slides, String> {
    let document = load_text(path)?;
    let mut pages = vec![Vec::new()];
    for line in document.lines {
        if line.trim() == "---" {
            if pages.len() >= MAX_SLIDES {
                return Err("slides exceed the 512-page limit".into());
            }
            pages.push(Vec::new());
        } else {
            pages.last_mut().expect("one page exists").push(line);
        }
    }
    pages.retain(|page| page.iter().any(|line| !line.trim().is_empty()));
    if pages.is_empty() {
        return Err("slides contain no content".into());
    }
    Ok(Slides {
        name: document.name,
        pages,
    })
}

fn snapshot_slides(slides: Slides, options: &Options) -> Result<i32, String> {
    let index = slide_index(options.page, slides.pages.len())?;
    let page = &slides.pages[index];
    match options.format {
        Format::Plain => {
            let mut output = format!(
                "SLIDES\t{}\tpage {}/{}",
                slides.name,
                index + 1,
                slides.pages.len()
            );
            for line in page {
                output.push('\n');
                output.push_str(line);
            }
            write_stdout(&output)
        }
        Format::Json => {
            let lines = page
                .iter()
                .map(|line| json_string(line))
                .collect::<Vec<_>>()
                .join(",");
            write_stdout(&format!(
                "{{\"app\":\"slides\",\"source\":{},\"page\":{},\"pageCount\":{},\"lines\":[{}]}}",
                json_string(&slides.name),
                index + 1,
                slides.pages.len(),
                lines
            ))
        }
    }
}

fn slide_index(page: usize, pages: usize) -> Result<usize, String> {
    page.checked_sub(1)
        .filter(|index| *index < pages)
        .ok_or_else(|| format!("--page exceeds the {pages}-page deck"))
}

struct Paint {
    name: String,
    width: usize,
    height: usize,
    cells: Vec<char>,
    cursor_x: usize,
    cursor_y: usize,
    brush: char,
    status: String,
}

fn load_paint(path: Option<&Path>, width: usize, height: usize) -> Result<Paint, String> {
    let (name, width, height, cells) = if let Some(path) = path {
        let document = load_text(path)?;
        let height = document.lines.len();
        let width = document
            .lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(1);
        if width == 0 || width > MAX_PAINT_WIDTH || height == 0 || height > MAX_PAINT_HEIGHT {
            return Err("paint input must fit within 240 columns and 80 rows".into());
        }
        let mut cells = vec![' '; width * height];
        for (y, line) in document.lines.iter().enumerate() {
            for (x, character) in line.chars().enumerate() {
                if character.is_control() {
                    return Err("paint input contains a control character".into());
                }
                cells[y * width + x] = character;
            }
        }
        (document.name, width, height, cells)
    } else {
        (
            "new canvas".to_owned(),
            width,
            height,
            vec![' '; width * height],
        )
    };
    Ok(Paint {
        name,
        width,
        height,
        cells,
        cursor_x: 0,
        cursor_y: 0,
        brush: '#',
        status: String::new(),
    })
}

impl Paint {
    fn lines(&self, cursor: bool) -> Vec<String> {
        self.cells
            .chunks(self.width)
            .enumerate()
            .map(|(y, row)| {
                row.iter()
                    .enumerate()
                    .map(|(x, character)| {
                        if cursor && x == self.cursor_x && y == self.cursor_y && *character == ' ' {
                            '▯'
                        } else {
                            *character
                        }
                    })
                    .collect()
            })
            .collect()
    }
}

fn snapshot_paint(paint: Paint, options: &Options) -> Result<i32, String> {
    let lines = paint.lines(false);
    match options.format {
        Format::Plain => {
            let mut output = format!("PAINT\t{}\t{}x{}", paint.name, paint.width, paint.height);
            for line in lines {
                output.push('\n');
                output.push_str(line.trim_end());
            }
            write_stdout(&output)
        }
        Format::Json => {
            let lines = lines
                .iter()
                .map(|line| json_string(line))
                .collect::<Vec<_>>()
                .join(",");
            write_stdout(&format!(
                "{{\"app\":\"paint\",\"source\":{},\"width\":{},\"height\":{},\"lines\":[{}]}}",
                json_string(&paint.name),
                paint.width,
                paint.height,
                lines
            ))
        }
    }
}

fn snapshot_timer(options: &Options) -> Result<i32, String> {
    let remaining = options.seconds - options.elapsed;
    match options.format {
        Format::Plain => write_stdout(&format!(
            "TIMER\t{}\t{} elapsed\t{} remaining\t{} total",
            options.label, options.elapsed, remaining, options.seconds
        )),
        Format::Json => write_stdout(&format!(
            "{{\"app\":\"timer\",\"label\":{},\"elapsedSeconds\":{},\"remainingSeconds\":{},\"totalSeconds\":{}}}",
            json_string(&options.label),
            options.elapsed,
            remaining,
            options.seconds
        )),
    }
}

struct TerminalFrame {
    previous: Vec<String>,
    dimensions: (u16, u16),
}

impl TerminalFrame {
    fn new() -> Self {
        Self {
            previous: Vec::new(),
            dimensions: (0, 0),
        }
    }

    fn draw(
        &mut self,
        title: &str,
        detail: &str,
        body: &[String],
        footer: &str,
    ) -> Result<(), String> {
        let (width, height) = size().unwrap_or((80, 24));
        let width = width.clamp(1, 240);
        let height = height.clamp(1, 80);
        let mut lines = vec![String::new(); usize::from(height)];
        if width < 50 || height < 16 {
            lines[usize::from(height / 2)] = centered("Resize to at least 50 x 16", width);
        } else {
            lines[1] = centered(&format!("+-- APPS // {} --+", title.to_uppercase()), width);
            lines[2] = centered(detail, width);
            let available = usize::from(height.saturating_sub(7));
            for (target, source) in lines[4..4 + available].iter_mut().zip(body) {
                *target = fit(source, usize::from(width));
            }
            lines[usize::from(height - 2)] = centered(footer, width);
        }

        let mut stdout = BufWriter::new(io::stdout());
        if self.dimensions != (width, height) {
            queue!(stdout, Clear(ClearType::All)).map_err(draw_error)?;
            self.previous = vec![String::new(); lines.len()];
            self.dimensions = (width, height);
        }
        queue!(stdout, BeginSynchronizedUpdate).map_err(draw_error)?;
        for (y, line) in lines.iter().enumerate() {
            if self.previous.get(y) == Some(line) {
                continue;
            }
            queue!(
                stdout,
                MoveTo(0, y as u16),
                Clear(ClearType::CurrentLine),
                Print(line)
            )
            .map_err(draw_error)?;
        }
        queue!(stdout, EndSynchronizedUpdate).map_err(draw_error)?;
        stdout.flush().map_err(draw_error)?;
        self.previous = lines;
        Ok(())
    }
}

fn draw_error(error: io::Error) -> String {
    format!("cannot draw terminal: {error}")
}

fn fit(value: &str, width: usize) -> String {
    value.chars().take(width).collect()
}

fn centered(value: &str, width: u16) -> String {
    let value = fit(value, usize::from(width));
    let padding = usize::from(width).saturating_sub(value.chars().count()) / 2;
    format!("{}{}", " ".repeat(padding), value)
}

fn body_height() -> usize {
    usize::from(size().unwrap_or((80, 24)).1.min(80).saturating_sub(7)).max(1)
}

fn read_key(wait: Duration) -> Result<Option<KeyEvent>, String> {
    if !event::poll(wait).map_err(|error| format!("cannot read terminal: {error}"))? {
        return Ok(None);
    }
    let event = event::read().map_err(|error| format!("cannot read terminal: {error}"))?;
    let Event::Key(key) = event else {
        return Ok(None);
    };
    Ok(matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat).then_some(key))
}

fn quits(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Esc | KeyCode::Char('q' | 'Q'))
        || key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c' | 'C'))
}

fn open_read(document: TextDocument, options: &Options) -> Result<i32, String> {
    let _session = Session::enter().map_err(|error| format!("cannot prepare terminal: {error}"))?;
    let mut frame = TerminalFrame::new();
    let found = matches(&document, options.find.as_deref());
    let mut offset = found.first().copied().unwrap_or(0);
    loop {
        let height = body_height();
        let body = document
            .lines
            .iter()
            .enumerate()
            .skip(offset)
            .take(height)
            .map(|(index, line)| {
                format!(
                    "{} {:>6}  {}",
                    if found.binary_search(&index).is_ok() {
                        '>'
                    } else {
                        ' '
                    },
                    index + 1,
                    line
                )
            })
            .collect::<Vec<_>>();
        frame.draw(
            "read",
            &format!(
                "{} · {} lines · {} matches",
                document.name,
                document.lines.len(),
                found.len()
            ),
            &body,
            "Up/Down scroll  PageUp/PageDown jump  Home/End edges  n next match  q quit",
        )?;
        let Some(key) = read_key(Duration::from_secs(1))? else {
            continue;
        };
        if quits(key) {
            break;
        }
        match key.code {
            KeyCode::Up => offset = offset.saturating_sub(1),
            KeyCode::Down => offset = (offset + 1).min(document.lines.len().saturating_sub(1)),
            KeyCode::PageUp => offset = offset.saturating_sub(height),
            KeyCode::PageDown => {
                offset = (offset + height).min(document.lines.len().saturating_sub(1));
            }
            KeyCode::Home => offset = 0,
            KeyCode::End => offset = document.lines.len().saturating_sub(height),
            KeyCode::Char('n' | 'N') if !found.is_empty() => {
                offset = found
                    .iter()
                    .copied()
                    .find(|index| *index > offset)
                    .unwrap_or(found[0]);
            }
            _ => {}
        }
    }
    Ok(0)
}

fn open_table(table: Table) -> Result<i32, String> {
    let _session = Session::enter().map_err(|error| format!("cannot prepare terminal: {error}"))?;
    let mut frame = TerminalFrame::new();
    let mut row_offset = 0_usize;
    let mut column_offset = 0_usize;
    loop {
        let (width, _) = size().unwrap_or((80, 24));
        let visible_columns = (usize::from(width.min(240)).saturating_sub(8) / 19).max(1);
        let height = body_height().saturating_sub(2).max(1);
        let mut body = Vec::new();
        if let Some(header) = table.rows.first() {
            body.push(render_table_row(header, column_offset, visible_columns, 0));
            body.push("-".repeat(usize::from(width.min(240))));
        }
        for (index, row) in table
            .rows
            .iter()
            .enumerate()
            .skip(row_offset + 1)
            .take(height)
        {
            body.push(render_table_row(row, column_offset, visible_columns, index));
        }
        frame.draw(
            "table",
            &format!(
                "{} · {} rows × {} columns",
                table.name,
                table.rows.len().saturating_sub(1),
                table.columns
            ),
            &body,
            "Arrows scroll  PageUp/PageDown jump  Home/End edges  q quit",
        )?;
        let Some(key) = read_key(Duration::from_secs(1))? else {
            continue;
        };
        if quits(key) {
            break;
        }
        match key.code {
            KeyCode::Up => row_offset = row_offset.saturating_sub(1),
            KeyCode::Down => {
                row_offset = (row_offset + 1).min(table.rows.len().saturating_sub(2));
            }
            KeyCode::Left => column_offset = column_offset.saturating_sub(1),
            KeyCode::Right => {
                column_offset = (column_offset + 1).min(table.columns.saturating_sub(1));
            }
            KeyCode::PageUp => row_offset = row_offset.saturating_sub(height),
            KeyCode::PageDown => {
                row_offset = (row_offset + height).min(table.rows.len().saturating_sub(2));
            }
            KeyCode::Home => {
                row_offset = 0;
                column_offset = 0;
            }
            KeyCode::End => row_offset = table.rows.len().saturating_sub(height + 1),
            _ => {}
        }
    }
    Ok(0)
}

fn render_table_row(row: &[String], offset: usize, columns: usize, index: usize) -> String {
    let mut output = if index == 0 {
        " HEADER ".to_owned()
    } else {
        format!("{:>7} ", index)
    };
    for column in offset..(offset + columns) {
        let value = row
            .get(column)
            .map_or("", String::as_str)
            .replace(['\n', '\r', '\t'], " ");
        output.push_str(&format!("{:<18} ", fit(&value, 18)));
    }
    output
}

fn open_slides(slides: Slides, page: usize) -> Result<i32, String> {
    let mut index = slide_index(page, slides.pages.len())?;
    let _session = Session::enter().map_err(|error| format!("cannot prepare terminal: {error}"))?;
    let mut frame = TerminalFrame::new();
    let mut offset = 0_usize;
    loop {
        let height = body_height();
        let body = slides.pages[index]
            .iter()
            .skip(offset)
            .take(height)
            .map(|line| markdown_line(line))
            .collect::<Vec<_>>();
        frame.draw(
            "slides",
            &format!(
                "{} · page {}/{}",
                slides.name,
                index + 1,
                slides.pages.len()
            ),
            &body,
            "Left/Right page  Up/Down scroll  Home/End first/last  q quit",
        )?;
        let Some(key) = read_key(Duration::from_secs(1))? else {
            continue;
        };
        if quits(key) {
            break;
        }
        match key.code {
            KeyCode::Left | KeyCode::PageUp => {
                index = index.saturating_sub(1);
                offset = 0;
            }
            KeyCode::Right | KeyCode::PageDown => {
                index = (index + 1).min(slides.pages.len() - 1);
                offset = 0;
            }
            KeyCode::Up => offset = offset.saturating_sub(1),
            KeyCode::Down => {
                offset = (offset + 1).min(slides.pages[index].len().saturating_sub(1));
            }
            KeyCode::Home => {
                index = 0;
                offset = 0;
            }
            KeyCode::End => {
                index = slides.pages.len() - 1;
                offset = 0;
            }
            _ => {}
        }
    }
    Ok(0)
}

fn markdown_line(line: &str) -> String {
    let trimmed = line.trim_start();
    let heading = trimmed.trim_start_matches('#').trim_start();
    if heading.len() != trimmed.len() {
        heading.to_uppercase()
    } else {
        line.to_owned()
    }
}

fn open_paint(mut paint: Paint, output: Option<&Path>) -> Result<i32, String> {
    let _session = Session::enter().map_err(|error| format!("cannot prepare terminal: {error}"))?;
    let mut frame = TerminalFrame::new();
    loop {
        frame.draw(
            "paint",
            &format!(
                "{} · {}×{} · brush {}{}{}",
                paint.name,
                paint.width,
                paint.height,
                paint.brush,
                if paint.status.is_empty() { "" } else { " · " },
                paint.status
            ),
            &paint.lines(true),
            "Arrows move  printable/Space draw  Backspace erase  s save new file  q quit",
        )?;
        let Some(key) = read_key(Duration::from_secs(1))? else {
            continue;
        };
        if quits(key) {
            break;
        }
        paint.status.clear();
        match key.code {
            KeyCode::Up => paint.cursor_y = paint.cursor_y.saturating_sub(1),
            KeyCode::Down => paint.cursor_y = (paint.cursor_y + 1).min(paint.height - 1),
            KeyCode::Left => paint.cursor_x = paint.cursor_x.saturating_sub(1),
            KeyCode::Right => paint.cursor_x = (paint.cursor_x + 1).min(paint.width - 1),
            KeyCode::Backspace | KeyCode::Delete => {
                paint.cells[paint.cursor_y * paint.width + paint.cursor_x] = ' ';
            }
            KeyCode::Char('s' | 'S') => {
                paint.status = match output {
                    Some(path) => {
                        save_paint(&paint, path).map(|()| format!("saved {}", human_path(path)))
                    }
                    None => Err("start paint with --output FILE to save".into()),
                }
                .unwrap_or_else(|error| error);
            }
            KeyCode::Char(' ') => {
                paint.cells[paint.cursor_y * paint.width + paint.cursor_x] = paint.brush;
            }
            KeyCode::Char(character)
                if !character.is_control()
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                paint.brush = character;
                paint.cells[paint.cursor_y * paint.width + paint.cursor_x] = character;
            }
            _ => {}
        }
    }
    Ok(0)
}

fn save_paint(paint: &Paint, path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!("refusing to overwrite {}", human_path(path)));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let file_name = path
        .file_name()
        .ok_or("paint output needs a file name")?
        .to_string_lossy();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let temporary = parent.join(format!(
        ".{file_name}.apps-{}-{nonce}.tmp",
        std::process::id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("cannot create paint temporary file: {error}"))?;
        for line in paint.lines(false) {
            writeln!(file, "{}", line.trim_end())
                .map_err(|error| format!("cannot write paint output: {error}"))?;
        }
        file.sync_all()
            .map_err(|error| format!("cannot sync paint output: {error}"))?;
        drop(file);
        fs::hard_link(&temporary, path)
            .map_err(|error| format!("cannot create paint output {}: {error}", human_path(path)))?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn display_seconds(duration: Duration) -> u64 {
    duration
        .as_secs()
        .saturating_add(u64::from(duration.subsec_nanos() > 0))
}

fn open_timer(seconds: u64, label: &str) -> Result<i32, String> {
    let _session = Session::enter().map_err(|error| format!("cannot prepare terminal: {error}"))?;
    let mut frame = TerminalFrame::new();
    let total = Duration::from_secs(seconds);
    let mut accumulated = Duration::ZERO;
    let mut started = Some(Instant::now());
    loop {
        let elapsed = accumulated + started.map_or(Duration::ZERO, |instant| instant.elapsed());
        let remaining = total.saturating_sub(elapsed);
        let display_remaining = display_seconds(remaining);
        let completed = elapsed >= total;
        let bar_width = 40_usize;
        let filled = ((elapsed.as_secs_f64() / total.as_secs_f64()) * bar_width as f64)
            .round()
            .clamp(0.0, bar_width as f64) as usize;
        let width = size().unwrap_or((80, 24)).0.min(240);
        let body = vec![
            String::new(),
            centered(label, width),
            centered(
                &format!(
                    "{:02}:{:02}",
                    display_remaining / 60,
                    display_remaining % 60
                ),
                width,
            ),
            centered(
                &format!("[{}{}]", "#".repeat(filled), "-".repeat(bar_width - filled)),
                width,
            ),
            centered(
                if completed {
                    "DONE"
                } else if started.is_none() {
                    "PAUSED"
                } else {
                    "RUNNING"
                },
                width,
            ),
        ];
        frame.draw(
            "timer",
            &format!("{seconds} second countdown"),
            &body,
            "Space pause/resume  r reset  q quit",
        )?;
        if completed {
            break;
        }
        let Some(key) = read_key(Duration::from_millis(100))? else {
            continue;
        };
        if quits(key) {
            break;
        }
        match key.code {
            KeyCode::Char(' ') => {
                if let Some(instant) = started.take() {
                    accumulated += instant.elapsed();
                } else {
                    started = Some(Instant::now());
                }
            }
            KeyCode::Char('r' | 'R') => {
                accumulated = Duration::ZERO;
                started = Some(Instant::now());
            }
            _ => {}
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_tables_and_rejects_unbounded_or_malformed_input() {
        let rows = parse_delimited(
            "name,note\nAda,\"line one\nline two\"\nLin,\"a \"\"quote\"\"\"",
            ',',
        )
        .expect("parse CSV");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1][1], "line one\nline two");
        assert_eq!(rows[2][1], "a \"quote\"");
        assert!(parse_delimited("a,\"unterminated", ',').is_err());
        assert!(parse_delimited("a,\"closed\"junk", ',').is_err());
    }

    #[test]
    fn validates_mode_specific_options() {
        let timer = parse_options(
            [
                "snapshot",
                "timer",
                "--seconds",
                "60",
                "--elapsed",
                "5",
                "--json",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            false,
        )
        .expect("parse timer");
        assert_eq!(timer.seconds, 60);
        assert_eq!(timer.elapsed, 5);
        assert!(
            parse_options(
                vec!["read".into(), "file".into(), "--page".into(), "2".into()],
                false
            )
            .is_err()
        );
        assert!(
            parse_options(vec!["timer".into(), "--seconds".into(), "0".into()], false).is_err()
        );
        assert!(
            parse_options(
                vec![
                    "open".into(),
                    "read".into(),
                    "file".into(),
                    "--limit".into(),
                    "2".into(),
                ],
                true,
            )
            .is_err()
        );
        assert!(slide_index(3, 2).is_err());
        assert_eq!(display_seconds(Duration::ZERO), 0);
        assert_eq!(display_seconds(Duration::from_millis(1)), 1);
    }

    #[test]
    fn sanitizes_terminal_metadata() {
        assert_eq!(
            human_text("notes\x1b]0;owned\x07.md"),
            "notes\\u{1b}]0;owned\\u{7}.md"
        );
    }

    #[test]
    fn paint_save_is_complete_and_never_overwrites() {
        let directory = std::env::temp_dir().join(format!(
            "keys-tools-apps-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::create_dir(&directory).expect("create paint test directory");
        let output = directory.join("drawing.txt");
        let mut paint = load_paint(None, 3, 1).expect("new paint canvas");
        paint.cells.copy_from_slice(&['A', 'B', ' ']);

        save_paint(&paint, &output).expect("save paint");
        assert_eq!(fs::read_to_string(&output).expect("read paint"), "AB\n");
        assert!(save_paint(&paint, &output).is_err());
        assert_eq!(fs::read_to_string(&output).expect("reread paint"), "AB\n");

        fs::remove_file(output).expect("remove paint output");
        fs::remove_dir(directory).expect("remove paint test directory");
    }
}
