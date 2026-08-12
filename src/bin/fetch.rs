fn main() {
    if let Err(error) = keys_tools::fetch::run(std::env::args().skip(1)) {
        eprintln!("fetch: {error}");
        std::process::exit(2);
    }
}
