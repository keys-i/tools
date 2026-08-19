fn main() {
    match keys_tools::science::run(std::env::args().skip(1).collect()) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("science: {error}");
            std::process::exit(2);
        }
    }
}
