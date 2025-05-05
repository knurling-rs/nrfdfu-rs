use std::iter;

use object::{
    elf::{FileHeader32, PT_LOAD},
    read::elf::{FileHeader, ProgramHeader, SectionHeader},
    Endianness, FileKind,
};

use crate::Result;

pub fn read_elf_image(elf: &[u8]) -> Result<Vec<u8>> {
    struct Chunk<'a> {
        flash_addr: u32,
        data: &'a [u8],
    }

    let file_kind = object::FileKind::parse(elf)
        .map_err(|e| format!("failed to parse firmware as ELF file: {}", e))?;

    if !matches!(file_kind, FileKind::Elf32) {
        return Err(format!(
            "firmware file has unsupported format {:?} (only 32-bit ELF files are supported)",
            file_kind
        )
        .into());
    }

    // Collect the to-be-flashed chunks.
    let mut chunks = Vec::new();

    let header = FileHeader32::<Endianness>::parse(elf)?;
    let endian = header.endian()?;
    let sections = header.section_headers(endian, elf)?;
    let strings = header.section_strings(endian, elf, sections)?;
    for (i, program) in header.program_headers(endian, elf)?.iter().enumerate() {
        let data = program
            .data(endian, elf)
            .map_err(|()| format!("failed to load segment data (corrupt ELF?)"))?;
        let p_type = program.p_type(endian);

        if !data.is_empty() && p_type == PT_LOAD {
            let (prog_offset, prog_size) = program.file_range(endian);

            let contains_section = sections.iter().enumerate().any(|(sidx, section)| {
                let name = String::from_utf8_lossy(section.name(endian, strings).unwrap());
                if section.sh_type(endian) == object::elf::SHT_NULL {
                    // Ignore NULL section that would start at 0
                    return false;
                }
                let (sec_offset, sec_size) = match section.file_range(endian) {
                    Some(range) => range,
                    None => return false,
                };

                let contained =
                    sec_offset >= prog_offset && sec_offset + sec_size <= prog_offset + prog_size;
                if contained {
                    log::debug!("phdr #{} contains section #{} {}", i, sidx, name);
                }
                contained
            });

            if contains_section {
                let flash_addr = program.p_paddr(endian);
                if flash_addr < 0x1000 {
                    return Err(format!(
                        "firmware starts at address {:#x}, expected an address equal or higher than 0x1000 to \
                         avoid a collision with the bootloader", flash_addr).into());
                }
                chunks.push(Chunk {
                    flash_addr,
                    data,
                });
            }
        }
    }

    chunks.sort_by_key(|chunk| chunk.flash_addr);
    for ch in chunks.windows(2) {
        if ch[1].flash_addr < ch[0].flash_addr + ch[0].data.len() as u32 {
            return Err(format!("overlapping chunks at {:#x}", ch[1].flash_addr).into());
        }
    }

    if chunks.is_empty() {
        return Err(format!(
            "no loadable program segments found; ensure that the linker is \
            invoked correctly (passing the linker script)"
        )
        .into());
    }

    let mut image = Vec::new();
    let mut addr = chunks[0].flash_addr;
    log::debug!("firmware starts at {:#x}", addr);

    for chunk in &chunks {
        if chunk.flash_addr < addr {
            return Err(format!(
                "overlapping program segments at 0x{:08x} (corrupt ELF?)",
                chunk.flash_addr
            )
            .into());
        }

        // Fill gaps between chunks with 0 bytes.
        let gap = chunk.flash_addr - addr;
        image.extend(iter::repeat(0).take(gap as usize));
        if gap > 0 {
            log::debug!("0x{:08x}-0x{:08x} (gap)", addr, addr + gap - 1);
        }
        addr += gap;

        image.extend(chunk.data);

        log::debug!(
            "0x{:08x}-0x{:08x}",
            chunk.flash_addr,
            chunk.flash_addr as usize + chunk.data.len() - 1
        );
        addr += chunk.data.len() as u32;
    }

    Ok(image)
}
