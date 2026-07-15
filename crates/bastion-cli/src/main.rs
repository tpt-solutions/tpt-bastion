// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `bastion` — the TPT Bastion command-line interface.
//!
//! The CLI currently supports the compile and plan stages of the pipeline: it
//! reads a `.bcl` document, compiles it to IR, and (for `plan`) builds a
//! dependency-ordered execution plan. Later phases (apply, state, providers)
//! will be added here as the engine lands.

use anyhow::{Context, Result};
use bastion_dsl::compile_source;
use bastion_engine::plan_document;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "bastion",
    version,
    about = "TPT Bastion — next-generation Infrastructure as Code",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile a `.bcl` document to the Bastion IR.
    Compile {
        /// Path to the `.bcl` source file. Use `-` to read from stdin.
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Output format for the compiled IR.
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },

    /// Build an execution plan (dry-run) from a `.bcl` document.
    Plan {
        /// Path to the `.bcl` source file. Use `-` to read from stdin.
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Format {
    /// Pretty-printed JSON (default).
    Json,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Compile { file, format } => cmd_compile(&file, format),
        Command::Plan { file } => cmd_plan(&file),
    }
}

fn read_source(file: &PathBuf) -> Result<String> {
    if file.as_os_str() == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("reading source from stdin")?;
        Ok(buf)
    } else {
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))
    }
}

fn cmd_compile(file: &PathBuf, format: Format) -> Result<()> {
    let source = read_source(file)?;

    let doc = compile_source(&source).map_err(|e| anyhow::anyhow!("compile failed: {e}"))?;

    match format {
        Format::Json => {
            let json = serde_json::to_string_pretty(&doc).context("serializing IR to JSON")?;
            println!("{json}");
        }
    }
    Ok(())
}

fn cmd_plan(file: &PathBuf) -> Result<()> {
    let source = read_source(file)?;

    let doc = compile_source(&source).map_err(|e| anyhow::anyhow!("compile failed: {e}"))?;
    let plan = plan_document(&doc).map_err(|e| anyhow::anyhow!("plan failed: {e}"))?;

    println!("{}", plan.render());
    println!(
        "Note: this is a dry-run. Apply requires the provider runtime (Phase 6), which is not yet implemented."
    );
    Ok(())
}
