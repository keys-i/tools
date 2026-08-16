fn main() {
    match keys_tools::lazybox::run(std::env::args().skip(1).collect()) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("lazybox: {error}");
            std::process::exit(2);
        }
    }
}
