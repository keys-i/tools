pub mod fetch;
pub mod games;
pub mod lazybox;
pub mod science;
pub mod screensaver;
mod terminal;

use std::fmt::Write as _;
use std::io::{self, IsTerminal as _};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn use_color(plain: bool) -> bool {
    !plain && std::env::var_os("NO_COLOR").is_none() && io::stdout().is_terminal()
}

pub fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            value if value < '\u{20}' => {
                let _ = write!(escaped, "\\u{:04x}", value as u32);
            }
            value => escaped.push(value),
        }
    }
    escaped.push('"');
    escaped
}

pub fn human_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_control() {
            output.extend(character.escape_default());
        } else {
            output.push(character);
        }
    }
    output
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_json_and_formats_bytes() {
        assert_eq!(json_string("a\n\"b"), "\"a\\n\\\"b\"");
        assert_eq!(human_text("a\x1b\x07b"), "a\\u{1b}\\u{7}b");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
    }
}
