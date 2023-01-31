
use std::borrow::Cow;

use addr2line::{object::{Object, SymbolMap, SymbolMapName, ObjectSection}, ObjectContext, Context, fallible_iterator::FallibleIterator};
use clap::{Parser, ValueEnum};
use gimli::{RunTimeEndian, EndianSlice, Dwarf};
use log::{debug, info, LevelFilter, error, warn};
use cli_table::{Cell, Table, format::{Border, Separator}};

use regex::Regex;
use stack_sizes::{analyze_executable, analyze_object};
use rustc_demangle::demangle;

/// WhatTheStack (wts), a tool for analysing stack use via LLVM `-Zemit-stack-sizes` information
#[derive(Clone, Debug, PartialEq, Parser)]
pub struct Args {
    /// ELF or object file for parsing
    pub file: String,

    /// ELF or object file mode
    #[clap(long, default_value="elf")]
    pub mode: Mode,

    /// Sort by function or stack size
    #[clap(long, default_value="stack")]
    pub sort: Sort,

    /// Minimum size for filtering
    #[clap(long, default_value="16")]
    pub min_size: u64,

    /// Number of lines to show
    #[clap(short='n', long, default_value="10")]
    pub lines: usize,

    /// Resolve addresses to source locations
    #[clap(long)]
    pub map_source: bool,

    /// Disable function name shortening
    #[clap(long)]
    pub long_names: bool,

    /// Log level
    #[clap(long, default_value="info")]
    pub log_level: LevelFilter,
}

#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum Sort {
    /// Sort by function size
    Text,
    /// Sort by stack size
    Stack,
}

#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum Mode {
    /// Load ELF file
    Elf,
    /// Load Object File
    Object,
}

fn main() -> anyhow::Result<()> {
    // Parse arguments
    let args = Args::parse();

    // Setup logging
    let _ = simplelog::TermLogger::init(
        args.log_level,
        Default::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    );

    debug!("args: {:?}", args);

    // Load ELF file
    debug!("Loading object: {}", args.file);
    let b = std::fs::read(args.file)?;

    // Parse via stack sizes
    debug!("Parsing LLVM stack size information");
    let funcs = match args.mode {
        Mode::Elf => analyze_executable(&b[..])?,
        Mode::Object => {
            let _l = analyze_object(&b[..])?;

            todo!("object mode not yet implemented");
        },
    };

    // Load DWARF symbols if available
    debug!("Loading DWARF information");

    let d = match args.map_source {
        true => Some(DwarfContext::load(&b[..])?),
        false => None,
    };

    info!("Parsed {} functions ({} undefined)", funcs.defined.len(), funcs.undefined.len());

    // Apply filters & sorts
    let defined = match args.sort {
        Sort::Text => {
            let mut defined: Vec<_> = funcs.defined.iter().collect();

            // Sort by text size
            defined.sort_by_key(|(_a, f)| f.size() );

            defined.reverse();

            defined
        },
        Sort::Stack => {
            // Filter by functions with defined stack use
            let mut defined: Vec<_> = funcs.defined.iter().filter(|(_a, f)| match f.stack() {
                Some(v) if v >= args.min_size => true,
                _ => false,
            }).collect();

            // Sort by stack size
            defined.sort_by_key(|(_a, f) | f.stack().unwrap() );
            defined.reverse();

            defined
        },
    };  

    if defined.len() == 0 {
        return Err(anyhow::anyhow!("no stack length information found"));
    }

    // Build table for display
    let n = defined.len().min(args.lines);
    let table_data: Vec<_> = (&defined[..n]).iter().map(|(addr, f)| {
        // Demangle name
        let name = format!("{:#}", demangle(f.names()[0]));
        let name = match args.long_names {
            true => name,
            false => compress_name(&name),
        };

        let stack = f.stack().map(|s| s.cell() ).unwrap_or( "UNKNOWN".cell() );

        let mut line = vec![
            format!("0x{:08x}", addr).cell(),
            f.size().cell(),
            stack.cell(),
            name.cell(),
        ];

        match d.as_ref().map(|d| d.get_line(f.names()[0], **addr) ) {
            Some(Ok(Some(v))) => line.push(v.cell()),
            _ => ()
        }

        line
    }).collect();

    let mut titles = vec![
        "ADDR",
        "SIZE",
        "STACK",
        "NAME",
    ];
    if d.is_some() {
        titles.push("SOURCE");
    }

    let table = table_data.table()
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

struct DwarfContext<'a>{
    endianness: gimli::RunTimeEndian,
    symbols: SymbolMap<SymbolMapName<'a>>,
    dwarf_cow: Dwarf<Cow<'a, [u8]>>,
    //context: Context<EndianSlice<'a, RunTimeEndian>>,
}

impl <'a>DwarfContext<'a> {
    fn load(d: &'a [u8]) -> anyhow::Result<Self> {
        debug!("Loading DWARF information");

        // Parse file
        let obj = object::File::parse(d)?;

        debug!("Sections:");
        for s in obj.sections() {
            debug!("{}, {}, {}", s.index().0, s.address(), s.name().unwrap());
        }
        let symbols = obj.symbol_map();

        // Load a section and return as `Cow<[u8]>`.
        let load_section = |id: gimli::SectionId| -> Result<Cow<[u8]>, gimli::Error> {
            let name = id.name();
            debug!("load section: {:?}", name);

            let section = match obj.section_by_name(name) {
                Some(s) => s,
                None => {
                    warn!("No section {} found", name);
                    return Ok(Cow::Borrowed(&[][..]));
                },
            };

            debug!("found section: {:?}", section);

            let d = match section.uncompressed_data() {
                Ok(v) => v,
                Err(e) => {
                    error!("Failed to load data for section {}: {:?}", id.name(), e);
                    
                    return Err(gimli::Error::UnknownIndexSection)
                }
            };

            debug!("Data: {}", d.len());

            Ok(d)
        };

        // Load all of the sections.
        let mut dwarf_cow = gimli::Dwarf::load(&load_section)?;
        dwarf_cow.load_sup(&load_section)?;

        let endianness = match obj.endianness() {
            object::Endianness::Little => gimli::RunTimeEndian::Little,
            object::Endianness::Big => gimli::RunTimeEndian::Big,
        };

        let s = Self { symbols, dwarf_cow, endianness };

        // Create `EndianSlice`s for all of the sections.
        let dwarf = s.dwarf();

        let units: Vec<_> = dwarf.units().collect()?;
        debug!("Loaded {} units: {:?}", units.len(), units);

        Ok(s)
    }

    pub fn dwarf(&'a self) -> Dwarf<EndianSlice<'a, RunTimeEndian>> {
        // Borrow a `Cow<[u8]>` to create an `EndianSlice`.
        let borrow_section: &dyn for<'c> Fn(
            &'c Cow<[u8]>,
        ) -> gimli::EndianSlice<'c, gimli::RunTimeEndian> =
            &|section| gimli::EndianSlice::new(&*section, self.endianness);

        self.dwarf_cow.borrow(&borrow_section)
    }

    pub fn context(&'a self) -> anyhow::Result<Context<EndianSlice<'a, RunTimeEndian>>> {
        let c = addr2line::Context::from_dwarf(self.dwarf())?;
        Ok(c)
    }

    pub fn get_line(&self, name: &str, addr: u64) -> anyhow::Result<Option<String>> {
        debug!("Lookup sym: {} addr: 0x{:08x}", name, addr);

        // Find symbol by name / addr
        let s = self.symbols.get(addr);
        debug!("Match symbol: {:?}", s);

        let dwarf = self.dwarf();

        let x = dwarf.debug_line.program(addr);
        debug!("DWARF: {:?}", x);

        Ok(None)
    }

    #[cfg(nope)]
    fn idk {
        // Find location via dwarf
        // TODO: this, doesn't seem to work, maybe dwarf is missing line info?
        let r = match ctx.find_location(addr)? {
            Some(r) => r,
            None => {
                debug!("No line info for {}", name);
                return Ok(None);
            },
        };

        // Format source location
        let s = match (r.file, r.line) {
            (Some(f), Some(l)) => format!("{}:{}", f, l),
            _ => unimplemented!()
        };

        Ok(Some(s))
    }
}

lazy_static::lazy_static! {
    static ref PREFIX: Regex = Regex::new(r"^([a-z0-9_:]+)(.*)").unwrap();
    static ref NAMES: Regex = Regex::new(r"(?:[a-z0-9_]+::)+([A-Z][a-z0-9_A-Z]+)").unwrap();
}

fn compress_name(n: &str) -> String {
    // Strip obvious prefixes
    let mut s = PREFIX.replace_all(n, "$2").to_string();
    if s.len() == 0 {
        return n.to_string();
    }

    // Shorten names
    s = NAMES.replace_all(&s, "$1").to_string();
    
    // Return compressed form
    s.to_string()
}
