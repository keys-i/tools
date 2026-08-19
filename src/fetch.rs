use std::env;
#[cfg(target_os = "linux")]
use std::fs;

use crate::{VERSION, human_bytes, human_text, json_string, use_color};

const HELP: &str = "Fast system and CPU overview\n\nUsage: fetch [--plain | --json]\n\nOptions:\n  --plain     Stable text without decoration\n  --json      Machine-readable JSON\n  -h, --help  Show this help\n  -V, --version  Show the version";

#[derive(Debug, PartialEq)]
struct Snapshot {
    host: String,
    os: Option<String>,
    kernel: Option<String>,
    architecture: String,
    cpu: Option<String>,
    cpu_topology: Option<String>,
    cpu_cache: Option<String>,
    physical_cores: Option<usize>,
    logical_cores: usize,
    memory_bytes: Option<u64>,
    shell: Option<String>,
    terminal: Option<String>,
}

pub fn run(arguments: impl Iterator<Item = String>) -> Result<(), String> {
    let mut plain = false;
    let mut json = false;
    for argument in arguments {
        match argument.as_str() {
            "--plain" => plain = true,
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("fetch {VERSION}");
                return Ok(());
            }
            _ => return Err(format!("unknown option {argument:?}; try 'fetch --help'")),
        }
    }
    if plain && json {
        return Err("--plain and --json cannot be used together".into());
    }

    let snapshot = collect();
    if json {
        println!("{}", render_json(&snapshot));
    } else if use_color(plain) {
        print_styled(&snapshot);
    } else {
        print_plain(&snapshot);
    }
    Ok(())
}

fn collect() -> Snapshot {
    let logical_cores = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let mut snapshot = platform(logical_cores);
    snapshot.shell = env::var_os("SHELL")
        .and_then(|value| value.to_str().map(str::to_owned))
        .and_then(|value| value.rsplit('/').next().map(str::to_owned));
    snapshot.terminal = env::var("TERM_PROGRAM")
        .ok()
        .or_else(|| env::var("TERM").ok());
    snapshot
}

fn fields(snapshot: &Snapshot) -> Vec<(&'static str, String)> {
    let mut fields = Vec::with_capacity(9);
    fields.push(("Host", snapshot.host.clone()));
    if let Some(value) = &snapshot.os {
        fields.push(("OS", value.clone()));
    }
    if let Some(value) = &snapshot.kernel {
        fields.push(("Kernel", value.clone()));
    }
    fields.push(("Arch", snapshot.architecture.clone()));
    if let Some(value) = &snapshot.cpu {
        fields.push(("CPU", value.clone()));
    }
    if let Some(value) = &snapshot.cpu_topology {
        fields.push(("Topology", value.clone()));
    }
    fields.push((
        "Cores",
        match snapshot.physical_cores {
            Some(physical) if physical != snapshot.logical_cores => {
                format!("{physical} physical / {} logical", snapshot.logical_cores)
            }
            _ => snapshot.logical_cores.to_string(),
        },
    ));
    if let Some(value) = &snapshot.cpu_cache {
        fields.push(("Cache", value.clone()));
    }
    if let Some(value) = snapshot.memory_bytes {
        fields.push(("Memory", human_bytes(value)));
    }
    if let Some(value) = &snapshot.shell {
        fields.push(("Shell", value.clone()));
    }
    if let Some(value) = &snapshot.terminal {
        fields.push(("Terminal", value.clone()));
    }
    fields
}

fn print_plain(snapshot: &Snapshot) {
    for (name, value) in fields(snapshot) {
        println!("{name}\t{}", human_text(&value));
    }
}

fn print_styled(snapshot: &Snapshot) {
    const CYAN: &str = "\x1b[38;2;125;207;255m";
    const MAGENTA: &str = "\x1b[38;2;187;154;247m";
    const MUTED: &str = "\x1b[38;2;86;95;137m";
    const RESET: &str = "\x1b[0m";
    let user = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "user".into());
    let user = human_text(&user);
    let host = human_text(&snapshot.host);
    println!("{CYAN}╭─ ◈ FETCH {MUTED}//{RESET} {MAGENTA}{user}@{host}{RESET}");
    for (name, value) in fields(snapshot) {
        println!(
            "{CYAN}│{RESET} {MUTED}{name:8}{RESET} {}",
            human_text(&value)
        );
    }
    println!("{CYAN}╰─{RESET}");
}

fn render_json(snapshot: &Snapshot) -> String {
    fn optional(value: Option<&str>) -> String {
        value.map_or_else(|| "null".into(), json_string)
    }
    let physical = snapshot
        .physical_cores
        .map_or_else(|| "null".into(), |value| value.to_string());
    let memory = snapshot
        .memory_bytes
        .map_or_else(|| "null".into(), |value| value.to_string());
    format!(
        "{{\"host\":{},\"os\":{},\"kernel\":{},\"architecture\":{},\"cpu\":{},\"cpuTopology\":{},\"cpuCache\":{},\"physicalCores\":{},\"logicalCores\":{},\"memoryBytes\":{},\"shell\":{},\"terminal\":{}}}",
        json_string(&snapshot.host),
        optional(snapshot.os.as_deref()),
        optional(snapshot.kernel.as_deref()),
        json_string(&snapshot.architecture),
        optional(snapshot.cpu.as_deref()),
        optional(snapshot.cpu_topology.as_deref()),
        optional(snapshot.cpu_cache.as_deref()),
        physical,
        snapshot.logical_cores,
        memory,
        optional(snapshot.shell.as_deref()),
        optional(snapshot.terminal.as_deref()),
    )
}

#[cfg(target_os = "macos")]
fn platform(logical_cores: usize) -> Snapshot {
    let (cpu_topology, cpu_cache, physical_cores) = macos::cpu_details();
    Snapshot {
        host: unix::hostname(),
        os: macos::sysctl_string("kern.osproductversion").map(|value| format!("macOS {value}")),
        kernel: macos::sysctl_string("kern.osrelease").map(|value| format!("Darwin {value}")),
        architecture: env::consts::ARCH.into(),
        cpu: macos::sysctl_string("machdep.cpu.brand_string")
            .or_else(|| Some(format!("Apple {}", env::consts::ARCH))),
        cpu_topology,
        cpu_cache,
        physical_cores: physical_cores
            .or_else(|| macos::sysctl_number("hw.physicalcpu").map(|value| value as usize)),
        logical_cores,
        memory_bytes: macos::sysctl_number("hw.memsize"),
        shell: None,
        terminal: None,
    }
}

#[cfg(target_os = "linux")]
fn platform(logical_cores: usize) -> Snapshot {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    Snapshot {
        host: unix::hostname(),
        os: fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|text| linux::os_name(&text)),
        kernel: fs::read_to_string("/proc/sys/kernel/osrelease")
            .ok()
            .map(|value| value.trim().to_owned()),
        architecture: env::consts::ARCH.into(),
        cpu: linux::cpu_name(&cpuinfo),
        cpu_topology: None,
        cpu_cache: linux::cache(),
        physical_cores: linux::physical_cores(&cpuinfo),
        logical_cores,
        memory_bytes: fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|text| linux::memory_bytes(&text)),
        shell: None,
        terminal: None,
    }
}

#[cfg(target_os = "windows")]
fn platform(logical_cores: usize) -> Snapshot {
    Snapshot {
        host: env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".into()),
        os: Some("Windows".into()),
        kernel: None,
        architecture: env::consts::ARCH.into(),
        cpu: env::var("PROCESSOR_IDENTIFIER").ok(),
        cpu_topology: None,
        cpu_cache: None,
        physical_cores: None,
        logical_cores,
        memory_bytes: None,
        shell: None,
        terminal: None,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn platform(logical_cores: usize) -> Snapshot {
    Snapshot {
        host: env::var("HOSTNAME").unwrap_or_else(|_| "unknown".into()),
        os: Some(env::consts::OS.into()),
        kernel: None,
        architecture: env::consts::ARCH.into(),
        cpu: None,
        cpu_topology: None,
        cpu_cache: None,
        physical_cores: None,
        logical_cores,
        memory_bytes: None,
        shell: None,
        terminal: None,
    }
}

#[cfg(unix)]
mod unix {
    use std::ffi::{CStr, c_char, c_int};

    unsafe extern "C" {
        fn gethostname(name: *mut c_char, length: usize) -> c_int;
    }

    pub fn hostname() -> String {
        let mut buffer = [0_u8; 256];
        // SAFETY: `buffer` is writable for the supplied length and remains alive.
        let result = unsafe { gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
        if result == 0 {
            buffer[buffer.len() - 1] = 0;
            // SAFETY: the final byte above guarantees a terminating NUL.
            unsafe { CStr::from_ptr(buffer.as_ptr().cast()) }
                .to_string_lossy()
                .into_owned()
        } else {
            "unknown".into()
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{CStr, CString, c_char, c_int, c_void};
    use std::ptr;

    unsafe extern "C" {
        fn sysctlbyname(
            name: *const c_char,
            old: *mut c_void,
            old_length: *mut usize,
            new: *mut c_void,
            new_length: usize,
        ) -> c_int;
    }

    pub fn sysctl_string(name: &str) -> Option<String> {
        let name = CString::new(name).ok()?;
        let mut value = [0_u8; 256];
        let mut length = value.len();
        // SAFETY: `value` is writable for `length` bytes and no new value is supplied.
        if unsafe {
            sysctlbyname(
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                &mut length,
                ptr::null_mut(),
                0,
            )
        } != 0
            || length == 0
            || length > value.len()
        {
            return None;
        }
        value[value.len() - 1] = 0;
        CStr::from_bytes_until_nul(&value[..length.max(1)])
            .ok()
            .map(|value| value.to_string_lossy().trim().to_owned())
            .filter(|value| !value.is_empty())
    }

    pub fn sysctl_number(name: &str) -> Option<u64> {
        let name = CString::new(name).ok()?;
        let mut value = [0_u8; 8];
        let mut length = value.len();
        // SAFETY: `value` is writable for `length` bytes and no new value is supplied.
        if unsafe {
            sysctlbyname(
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                &mut length,
                ptr::null_mut(),
                0,
            )
        } != 0
        {
            return None;
        }
        match length {
            4 => Some(u32::from_ne_bytes(value[..4].try_into().ok()?) as u64),
            8 => Some(u64::from_ne_bytes(value)),
            _ => None,
        }
    }

    pub fn cpu_details() -> (Option<String>, Option<String>, Option<usize>) {
        let mut topology = Vec::new();
        let mut cache = Vec::new();
        let mut physical_cores = 0_usize;
        for index in 0..4 {
            let Some(name) = sysctl_string(&format!("hw.perflevel{index}.name")) else {
                break;
            };
            let Some(cores) = sysctl_number(&format!("hw.perflevel{index}.physicalcpu")) else {
                break;
            };
            physical_cores += cores as usize;
            topology.push(format!("{cores} {}", name.to_lowercase()));
            let Some(l2) = sysctl_number(&format!("hw.perflevel{index}.l2cachesize")) else {
                continue;
            };
            cache.push(format!("{} L2 {}", name, crate::human_bytes(l2)));
        }
        (
            (!topology.is_empty()).then(|| topology.join(" + ")),
            (!cache.is_empty()).then(|| cache.join(" / ")),
            (physical_cores > 0).then_some(physical_cores),
        )
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashSet;

    pub fn os_name(text: &str) -> Option<String> {
        value(text, "PRETTY_NAME").or_else(|| value(text, "NAME"))
    }

    fn value(text: &str, key: &str) -> Option<String> {
        text.lines().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name == key).then(|| value.trim_matches('"').replace("\\\"", "\""))
        })
    }

    pub fn cpu_name(text: &str) -> Option<String> {
        ["model name", "Hardware", "Processor"]
            .into_iter()
            .find_map(|key| {
                text.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    (name.trim() == key)
                        .then(|| value.trim().to_owned())
                        .filter(|value| !value.is_empty())
                })
            })
    }

    pub fn physical_cores(text: &str) -> Option<usize> {
        let mut cores = HashSet::new();
        for block in text.split("\n\n") {
            let mut package = None;
            let mut core = None;
            for line in block.lines() {
                let Some((key, value)) = line.split_once(':') else {
                    continue;
                };
                match key.trim() {
                    "physical id" => package = Some(value.trim()),
                    "core id" => core = Some(value.trim()),
                    _ => {}
                }
            }
            if let (Some(package), Some(core)) = (package, core) {
                cores.insert((package, core));
            }
        }
        (!cores.is_empty()).then_some(cores.len())
    }

    pub fn memory_bytes(text: &str) -> Option<u64> {
        text.lines().find_map(|line| {
            let rest = line.strip_prefix("MemTotal:")?;
            rest.split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()?
                .checked_mul(1024)
        })
    }

    pub fn cache() -> Option<String> {
        let base = std::path::Path::new("/sys/devices/system/cpu/cpu0/cache");
        let mut values = std::fs::read_dir(base)
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                let level = std::fs::read_to_string(path.join("level")).ok()?;
                let kind = std::fs::read_to_string(path.join("type")).ok()?;
                let size = std::fs::read_to_string(path.join("size")).ok()?;
                Some(format!("L{} {} {}", level.trim(), kind.trim(), size.trim()))
            })
            .collect::<Vec<_>>();
        values.sort_unstable();
        (!values.is_empty()).then(|| values.join(" / "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Snapshot {
        Snapshot {
            host: "laptop".into(),
            os: Some("Example OS".into()),
            kernel: Some("1.2.3".into()),
            architecture: "arm64".into(),
            cpu: Some("Example CPU".into()),
            cpu_topology: Some("4 performance + 4 efficiency".into()),
            cpu_cache: Some("Performance L2 16.0 MiB".into()),
            physical_cores: Some(4),
            logical_cores: 8,
            memory_bytes: Some(16 * 1024 * 1024 * 1024),
            shell: Some("zsh".into()),
            terminal: None,
        }
    }

    #[test]
    fn json_is_stable_and_machine_readable() {
        let json = render_json(&fixture());
        assert!(json.starts_with("{\"host\":\"laptop\",\"os\":\"Example OS\""));
        assert!(json.contains("\"cpuTopology\":\"4 performance + 4 efficiency\""));
        assert!(json.contains("\"physicalCores\":4,\"logicalCores\":8"));
        assert!(json.ends_with("\"terminal\":null}"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parses_linux_sources_once() {
        let cpu = "processor: 0\nphysical id: 0\ncore id: 0\nmodel name: Test CPU\n\nprocessor: 1\nphysical id: 0\ncore id: 0\nmodel name: Test CPU\n\nprocessor: 2\nphysical id: 0\ncore id: 1\nmodel name: Test CPU\n";
        assert_eq!(linux::cpu_name(cpu).as_deref(), Some("Test CPU"));
        assert_eq!(linux::physical_cores(cpu), Some(2));
        assert_eq!(linux::memory_bytes("MemTotal: 2048 kB\n"), Some(2_097_152));
        assert_eq!(
            linux::os_name("PRETTY_NAME=\"Small Linux\"\n").as_deref(),
            Some("Small Linux")
        );
    }
}
