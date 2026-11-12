mod additive;
mod entry;
mod fetch;
mod index;
mod readme;
mod release;
mod review;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Checks the plugins listed in `plugins/` and builds the index Lorealm
/// reads.
#[derive(Parser)]
struct Cli {
    /// Where downloaded releases are kept, so they're only downloaded once
    #[arg(long, global = true, default_value = "cache")]
    cache: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List a release: downloads its zip, checks it, and adds it to the
    /// plugin's file in `plugins/`
    Add {
        /// The zip attached to the plugin's GitHub release, f.ex.
        /// `https://github.com/you/plugin/releases/download/v0.1.0/plugin-0.1.0.zip`
        url: String,
    },
    /// Check changed plugin files, and print what a reviewer needs to know
    /// about the versions they add, as Markdown
    Check {
        /// The `plugins/` folder as it was before the change
        #[arg(long)]
        previous: Option<PathBuf>,
        /// The plugin files that changed
        files: Vec<PathBuf>,
    },
    /// Print what a change takes back from the plugins already listed, as
    /// Markdown: removed plugins, and removed or altered versions. Prints
    /// nothing when the change only adds
    Additive {
        /// The `plugins/` folder as it was before the change
        #[arg(long)]
        previous: PathBuf,
        /// The `plugins/` folder with the change
        #[arg(long)]
        current: PathBuf,
    },
    /// Build `index.json` and the READMEs from every plugin file
    Build {
        /// Where to write them
        #[arg(long, default_value = "site")]
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let fetcher = fetch::Fetcher::new(cli.cache);
    match cli.command {
        Command::Add { url } => entry::add(&fetcher, &url),
        Command::Check { previous, files } => review::run(&fetcher, previous.as_deref(), &files),
        Command::Additive { previous, current } => {
            print!("{}", additive::findings(&previous, &current)?);
            Ok(())
        }
        Command::Build { out } => index::build(&fetcher, &out),
    }
}
