use addr2line::{
    object::{Object, ObjectSection, SymbolMap, SymbolMapName},
    ObjectContext,
};

use log::debug;

pub struct DwarfContext<'a> {
    symbols: SymbolMap<SymbolMapName<'a>>,
    context: ObjectContext,
}

impl<'a> DwarfContext<'a> {
    pub fn load(d: &'a [u8]) -> anyhow::Result<Self> {
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
            }
        };

        // Format source location
        let s = match (r.file, r.line) {
            (Some(f), Some(l)) => format!("{}:{}", f, l),
            _ => unimplemented!(),
        };

        Ok(Some(s))
    }
}
