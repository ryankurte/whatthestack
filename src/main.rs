
use addr2line::{object::{Object, SymbolMap, SymbolMapName, ObjectSection}, ObjectContext};
use clap::{Parser, ValueEnum};
use log::{debug, info, LevelFilter};
use cli_table::{Cell, Table, format::{Border, Separator}};

use regex::Regex;
use stack_sizes::{analyze_executable, analyze_object};
use rustc_demangle::demangle;


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
    let _ = simplelog::SimpleLogger::init(args.log_level, Default::default());

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
    symbols: SymbolMap<SymbolMapName<'a>>,
    context: ObjectContext,
}

impl <'a>DwarfContext<'a> {
    fn load(d: &'a [u8]) -> anyhow::Result<Self> {
        // Parse file
        let object = addr2line::object::File::parse(d)?;

        debug!("Sections:");
        for s in object.sections() {
            debug!("{}, {}, {}", s.index().0, s.address(), s.name().unwrap());
        }

        let symbols = object.symbol_map();
    
        // Parse dwarf
        let context = addr2line::Context::new(&object)?;
    
        context.parse_lines()?;
        context.parse_functions()?;

        Ok(Self { symbols, context })
    }

    pub fn get_line(&self, name: &str, addr: u64) -> anyhow::Result<Option<String>> {
        debug!("Lookup sym: {} addr: 0x{:08x}", name, addr);

        // Find symbol by name / addr
        let s = self.symbols.get(addr);
        debug!("Match symbol: {:?}", s);

        // Find location via dwarf
        // TODO: this, doesn't seem to work, maybe dwarf is missing line info?
        let r = match self.context.find_location(addr)? {
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
