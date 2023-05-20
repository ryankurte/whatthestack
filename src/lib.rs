use clap::ValueEnum;

use log::{debug, info};

use rustc_demangle::demangle;
use serde::{Deserialize, Serialize};
use stack_sizes::{analyze_executable, analyze_object};

mod dwarf;
pub use dwarf::*;

mod helpers;
pub use helpers::*;

#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum Mode {
    /// Load ELF file
    Elf,
    /// Load Object File
    Object,
}

#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum Sort {
    /// Sort by function size
    Text,
    /// Sort by stack size
    Stack,
    /// Sort by function address
    Address,
}

/// Stack use report
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub functions: Vec<Function>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Function {
    /// Memory address
    pub addr: u64,

    /// Full / demangled name
    pub name: String,

    /// Text size
    pub text: u64,

    /// Stack size
    pub stack: u64,

    /// Function source location
    #[serde(default)]
    pub source: String,
}

impl Report {
    /// Parse a report from an ELF or object file
    pub fn parse(file: &str, mode: Mode, map_source: bool) -> Result<Report, anyhow::Error> {
        // Load ELF file
        debug!("Loading object: {}", file);
        let b = std::fs::read(file)?;

        // Parse via stack sizes
        debug!("Parsing LLVM stack size information");
        let parsed = match mode {
            Mode::Elf => analyze_executable(&b[..])?,
            Mode::Object => {
                let _l = analyze_object(&b[..])?;

                todo!("object mode not yet implemented");
            }
        };

        info!(
            "Parsed {} functions ({} undefined)",
            parsed.defined.len(),
            parsed.undefined.len()
        );

        // Load Dwarf context for source->line resolution
        // TODO: this is broken atm
        let ctx = match map_source {
            true => Some(DwarfContext::load(&b[..])?),
            false => None,
        };

        // Process functions into report format
        let mut functions = vec![];
        for (addr, f) in parsed.defined.iter() {
            // Demangle name
            let name = format!("{:#}", demangle(f.names()[0]));

            // Fetch text and stack sizes
            let text = f.size();
            let stack = f.stack().unwrap_or(0);

            // Attempt to resolve source line
            let source = match ctx.as_ref().map(|d| d.get_line(f.names()[0], *addr)) {
                Some(Ok(Some(v))) => v,
                _ => "".to_string(),
            };

            functions.push(Function {
                name,
                addr: *addr,
                stack,
                text,
                source,
            })
        }

        // Sort functions by address
        functions.sort_by_key(|f| f.addr);

        // Return report
        Ok(Report { functions })
    }

    /// Apply a sort to the internal report
    pub fn sort(&mut self, sort: Sort) {
        match sort {
            Sort::Text => {
                self.functions.sort_by_key(|f| f.text);
                self.functions.reverse();
            }
            Sort::Stack => {
                self.functions.sort_by_key(|f| f.stack);
                self.functions.reverse();
            }
            Sort::Address => {
                self.functions.sort_by_key(|f| f.addr);
            }
        }
    }

    /// Load a report from file
    pub fn load(file: &str) -> Result<Report, anyhow::Error> {
        // Open file
        let f = std::fs::File::open(file)?;

        // Read report
        let r = serde_json::from_reader(f)?;

        Ok(r)
    }

    /// Write a report to file
    pub fn save(&self, file: &str) -> Result<(), anyhow::Error> {
        // Encode to JSON
        let s = serde_json::to_string_pretty(self)?;
        // Write to file
        std::fs::write(file, s.as_bytes())?;

        Ok(())
    }

    /// Find a function by name
    pub fn find(&self, name: &str) -> Option<&Function> {
        self.functions.iter().find(|f| f.name == name)
    }
}
