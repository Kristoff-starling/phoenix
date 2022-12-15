use object::elf;
use object::elf::FileHeader64;
use object::endian::LittleEndian;
use object::read::elf::ElfSection;
use object::read::SymbolSection;
use object::{ObjectSection, Relocation, SectionFlags, SectionIndex, SectionKind};
use object::{RelocationKind, RelocationTarget};

use mmap::{Mmap, MmapOptions};

use super::symbol::{Symbol, SymbolLookupTable, SymbolTable};
use super::Error;

#[derive(Debug)]
pub(crate) struct Section {
    pub(crate) index: SectionIndex,
    pub(crate) address: u64,
    pub(crate) size: u64,
    pub(crate) align: u64,
    pub(crate) file_range: Option<(u64, u64)>,
    pub(crate) name: String,
    pub(crate) segment_name: Option<String>,
    pub(crate) kind: SectionKind,
    pub(crate) flags: SectionFlags,
    pub(crate) relocations: Vec<(u64, Relocation)>,
    /// For .bss sections, we need to allocate extra spaces.
    pub(crate) mmap: Option<Mmap>,
}

impl Section {
    pub(crate) fn new(section: &ElfSection<FileHeader64<LittleEndian>>) -> Self {
        Section {
            index: section.index(),
            // We will update the address later.
            address: section.address(),
            size: section.size(),
            align: section.align(),
            file_range: section.file_range(),
            name: section.name().unwrap_or("").to_owned(),
            segment_name: section.segment_name().unwrap_or(None).map(|x| x.to_owned()),
            kind: section.kind(),
            flags: section.flags(),
            relocations: section.relocations().collect::<Vec<(u64, Relocation)>>(),
            // For .bss sections, we need to allocate extra spaces. We'll fill this later.
            mmap: None,
        }
    }

    #[inline]
    pub(crate) fn need_load(&self) -> bool {
        if self.size == 0 {
            return false;
        }

        match self.kind {
            SectionKind::Text
            | SectionKind::Data
            | SectionKind::ReadOnlyData
            | SectionKind::Elf(elf::SHT_INIT_ARRAY)
            | SectionKind::Elf(elf::SHT_FINI_ARRAY) => true,
            _ => false,
        }
    }

    /// Update runtime address for sections needed to load. Allocate memory for .bss sections
    /// if encountered.
    pub(crate) fn update_runtime_addr(&mut self, image_addr: *const u8) -> Result<(), Error> {
        if self.kind.is_bss() && self.size > 0 {
            // Allocate memory for .bss section.
            assert!(self.align as usize <= page_size::get());
            // round up to page
            let rounded_size = self.size.next_multiple_of(page_size::get() as u64) as usize;
            let mmap = MmapOptions::new()
                .len(rounded_size)
                .anon(true)
                .private(true)
                .read(true)
                .write(true)
                .mmap()?;
            // update the address
            self.address = mmap.as_ptr().addr() as u64;
        } else if self.need_load() {
            let file_off = self.file_range.expect("impossible").0;
            self.address = unsafe { image_addr.offset(file_off as isize) }.addr() as u64;
        }
        Ok(())
    }
}

// Common symbols are a feature that allow a programmer to 'define' several
// variables of the same name in different source files.
// This is indeed 'common' in ELF relocatable object files.
pub(crate) struct CommonSection {
    mmap: Option<Mmap>,
    used: isize,
}

impl CommonSection {
    pub(crate) fn new(sym_table: &SymbolTable) -> Result<Self, Error> {
        let size: u64 = sym_table
            .symbols
            .iter()
            .filter_map(|sym| if sym.is_common { Some(sym.size) } else { None })
            .sum();
        let mmap = if size > 0 {
            Some(
                MmapOptions::new()
                    .len(size as usize)
                    .anon(true)
                    .private(true)
                    .read(true)
                    .write(true)
                    .mmap()?,
            )
        } else {
            None
        };
        Ok(Self { mmap, used: 0 })
    }

    pub(crate) fn alloc_entry_for_symbol(&mut self, sym: &Symbol) -> *const u8 {
        let mmap = self
            .mmap
            .as_ref()
            .expect("Something is wrong with calculating common size");
        let ret = unsafe { mmap.as_ptr().offset(self.used) };
        self.used += sym.size as isize;
        assert!(self.used as usize <= mmap.len());
        ret
    }
}

#[allow(non_snake_case)]
pub(crate) fn do_relocation(
    sections: &Vec<Section>,
    local_sym_table: &SymbolTable,
    global_sym_table: &SymbolLookupTable,
) {
    for sec in sections {
        if !sec.need_load() {
            continue;
        }

        for (off, rela) in &sec.relocations {
            let P = sec.address + off;
            let A = rela.addend();
            let S = match rela.target() {
                RelocationTarget::Symbol(sym_index) => {
                    let sym = local_sym_table.symbol_by_index(sym_index).unwrap();
                    if sym.is_global {
                        // for global symbols, get its name first
                        // then query the symbol in the global symbol lookup table
                        let def = global_sym_table
                            .get(&sym.name)
                            .unwrap_or_else(|| panic!("missing symbol {}", sym.name));
                        def.address
                    } else {
                        // for local symbols, just read its symbol address
                        let SymbolSection::Section(section_index) = sym.section else {
                            panic!("no such section: {:?}", sym.section);
                        };
                        let section = &sections[section_index.0];
                        section.address + sym.address
                    }
                }
                RelocationTarget::Section(_sec_index) => todo!("Got a section to relocate"),
                RelocationTarget::Absolute => 0,
                _ => panic!("rela: {:?}", rela),
            };

            let (P, A, S) = (P as i64, A as i64, S as i64);
            let Image = 0;
            let Section = 0;
            let value = match rela.kind() {
                RelocationKind::Absolute => S + A,
                RelocationKind::Relative => S + A - P,
                RelocationKind::Got => {
                    let GotBase = get_got_base();
                    let G = make_got();
                    G + A - GotBase
                }
                RelocationKind::GotRelative => {
                    let G = make_got();
                    G + A - P
                }
                RelocationKind::GotBaseRelative => {
                    let GotBase = get_got_base();
                    GotBase + A - P
                }
                RelocationKind::GotBaseOffset => {
                    let GotBase = get_got_base();
                    S + A - GotBase
                }
                RelocationKind::PltRelative => {
                    let L = make_plt();
                    L + A - P
                }
                RelocationKind::ImageOffset => S + A - Image,
                RelocationKind::SectionOffset => S + A - Section,
                _ => panic!("rela: {:?}", rela),
            };

            if rela.size() == 0 {
            } else {
                // SAFETY: P must be a valid
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        &value as *const i64 as *const u8,
                        P as *mut u8,
                        (rela.size() / 8) as usize,
                    );
                }
            }
        }
    }
}

#[inline]
fn get_got_base() -> i64 {
    todo!("get_got_base")
}

#[inline]
fn make_got() -> i64 {
    todo!("make_got")
}

#[inline]
fn make_plt() -> i64 {
    todo!("make_plt")
}