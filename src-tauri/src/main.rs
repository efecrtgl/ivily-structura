// Prevent an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--iep ...` is the Ivily Ecosystem Protocol CLI surface: answer and exit without a window.
    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    if let Some(code) = structura_lib::iep::run_cli(std::env::args().skip(1), &mut stdin, &mut stdout) {
        std::process::exit(code);
    }
    structura_lib::run()
}
