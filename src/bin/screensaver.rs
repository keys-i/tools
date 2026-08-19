fn main() {
    match keys_tools::screensaver::run(std::env::args().skip(1).collect()) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("screensaver: {error}");
            std::process::exit(2);
        }
    }
}
