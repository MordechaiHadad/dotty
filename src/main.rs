use std::process::ExitCode;
use tracing::error;

mod cli;
mod handlers;
mod manifest;
mod paths;

fn main() -> std::process::ExitCode {
    match cli::run() {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            error!("{error}");
            ExitCode::FAILURE},
    }
}
