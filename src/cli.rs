use crate::{handlers::apply_handler, manifest::Manifest};
use clap::{Parser, Subcommand};
use eyre::Result;
use tracing_indicatif::IndicatifLayer;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "dotty", version, about = "Manage dotfiles")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Apply { profile: String },
}

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();
    let indicatif_layer = IndicatifLayer::new();
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().with_writer(indicatif_layer.get_stderr_writer()))
        .with(indicatif_layer)
        .init();

    let manifest = Manifest::load()?;

    match cli.command {
        Command::Apply { profile } => apply_handler::run(&profile, manifest.folders(&profile)?),
    }
}
