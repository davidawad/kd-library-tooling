use std::path::PathBuf;

fn main() {
    let config_dir = std::env::var_os("KB_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let argv: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(kb_tooling::kb_root::main(&config_dir, &argv));
}
