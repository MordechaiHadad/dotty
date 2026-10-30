use std::process::ExitCode;

mod cli;
mod handlers;
mod manifest;
mod paths;

fn main() -> std::process::ExitCode {
    match cli::run() {
        Ok(_) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
