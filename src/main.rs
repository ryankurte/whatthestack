use clap::Parser;
use cli_table::{
    format::{Border, Separator},
    Cell, Table,
};
use log::{debug, info, LevelFilter};

use whatthestack::*;

/// WhatTheStack (wts), a tool for analysing stack use via LLVM `-Zemit-stack-sizes` information
#[derive(Clone, Debug, PartialEq, Parser)]
pub struct Args {
    /// ELF or object file for parsing
    pub file: String,

    /// ELF or object file mode
    #[clap(long, default_value = "elf")]
    pub mode: Mode,

    /// Sort by function or stack size
    #[clap(long, default_value = "stack")]
    pub sort: Sort,

    /// Minimum size for filtering
    #[clap(long, default_value = "16")]
    pub min_size: u64,

    /// Number of lines to show
    #[clap(short = 'n', long, default_value = "10")]
    pub lines: usize,

    /// Resolve addresses to source locations
    #[clap(long)]
    pub map_source: bool,

    /// Disable function name shortening
    #[clap(long)]
    pub long_names: bool,

    /// Filter results by prefix
    #[clap(long)]
    pub filter: Option<String>,

    /// Write the generated report to file
    #[clap(long)]
    pub write: Option<String>,

    /// Load a previously generated report for comparison
    #[clap(long)]
    pub prev: Option<String>,

    /// Log level
    #[clap(long, default_value = "info")]
    pub log_level: LevelFilter,
}

fn main() -> anyhow::Result<()> {
    // Parse arguments
    let args = Args::parse();

    // Setup logging
    let _ = simplelog::SimpleLogger::init(args.log_level, Default::default());

    debug!("args: {:?}", args);

    // Load ELF file
    let mut report = Report::parse(&args.file, args.mode, args.map_source)?;

    if args.write.is_some() && args.prev.is_some() {
        return Err(anyhow::anyhow!(
            "cannot write and compare reports at the same time"
        ));
    }

    // Write report if enabled
    if let Some(f) = args.write {
        info!("Saving report to: {}", f);
        report.save(&f)?;
    }

    // Load report for comparison if enabled
    let prev = match &args.prev {
        Some(f) => Report::load(f).map(|v| Some(v))?,
        None => None,
    };

    // Apply sort
    report.sort(args.sort);

    let mut defined = report.functions.clone();

    if defined.len() == 0 {
        return Err(anyhow::anyhow!("no stack length information found"));
    }

    // Apply filter if requested
    if let Some(s) = args.filter {
        defined = defined
            .drain(..)
            .filter(|f| f.name.starts_with(&s))
            .collect();
    }

    // Build table for display
    let n = defined.len().min(args.lines);
    let table_data: Vec<_> = (&defined[..n])
        .iter()
        .map(|f| {
            // Truncate name
            let name = match args.long_names {
                true => f.name.clone(),
                false => compress_name(&f.name),
            };

            // Compute diffs if we have a previous report
            let diffs = match prev.as_ref().map(|p| p.find(&f.name)).flatten() {
                Some(f1) => Some((
                    f.text as i64 - f1.text as i64,
                    f.stack as i64 - f1.stack as i64,
                )),
                None => None,
            };

            // Setup display line
            let mut line = vec![format!("0x{:08x}", f.addr).cell()];

            match diffs {
                Some((d_text, d_stack)) => {
                    line.push(format!("{:<4} ({:+})", f.text, d_text).cell());
                    line.push(format!("{:<4} ({:+})", f.stack, d_stack).cell());
                }
                None => {
                    line.push(f.text.cell());
                    line.push(f.stack.cell());
                }
            }

            line.push(name.cell());

            // Add source location if enabled
            if args.map_source {
                line.push(f.source.clone().cell());
            }

            line
        })
        .collect();

    let mut titles = vec!["ADDR", "SIZE", "STACK", "NAME"];
    if args.map_source {
        titles.push("SOURCE");
    }

    let table = table_data
        .table()
        .title(titles)
        .border(Border::builder().build())
        .separator(Separator::builder().row(None).build());

    let table_display = table.display().unwrap();
    println!("{}", table_display);

    // Warn on truncation
    if defined.len() > args.lines {
        info!("Truncated {} lines", defined.len() - args.lines);
    }

    Ok(())
}
