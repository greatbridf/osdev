use crate::{
    io::{ByteBuffer, UninitBuffer},
    kernel::{
        constants::ENOEXEC,
        mem::{FileMapping, MMList, Mapping, Permission},
        vfs::dentry::Dentry,
    },
    prelude::*,
};
use alloc::{ffi::CString, sync::Arc};
use bitflags::bitflags;
use eonix_mm::address::{Addr as _, AddrOps as _, VAddr};

#[repr(u8)]
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ElfFormat {
    Elf32 = 1,
    Elf64 = 2,
}

#[repr(u8)]
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ElfEndian {
    Little = 1,
    Big = 2,
}

#[repr(u8)]
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ElfABI {
    // SystemV = 0,
    Linux = 3,
}

#[repr(u16)]
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ElfType {
    Relocatable = 1,
    Executable = 2,
    Dynamic = 3,
    Core = 4,
}

#[repr(u16)]
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ElfArch {
    X86 = 0x03,
    Arm = 0x28,
    IA64 = 0x32,
    X86_64 = 0x3e,
    AArch64 = 0xb7,
    RiscV = 0xf3,
}

bitflags! {
    #[derive(Default, Clone, Copy)]
    pub struct Elf32PhFlags: u32 {
        const Exec = 1;
        const Write = 2;
        const Read = 4;
    }

    #[derive(Default, Clone, Copy)]
    pub struct Elf32ShFlags: u32 {
        const Write = 1;
        const Alloc = 2;
        const Exec = 4;
        const MaskProc = 0xf0000000;
    }
}

#[allow(dead_code)]
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Elf32PhType {
    #[default]
    Null = 0,
    Load = 1,
    Dynamic = 2,
    Interp = 3,
    Note = 4,
    Shlib = 5,
    Phdr = 6,
    Tls = 7,
    Loos = 0x60000000,
    Hios = 0x6fffffff,
    Loproc = 0x70000000,
    Hiproc = 0x7fffffff,
}

#[allow(dead_code)]
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Elf32ShType {
    #[default]
    Null = 0,
    ProgBits = 1,
    SymTab = 2,
    StrTab = 3,
    Rela = 4,
    Hash = 5,
    Dynamic = 6,
    Note = 7,
    NoBits = 8,
    Rel = 9,
    Shlib = 10,
    DynSym = 11,
    InitArray = 14,
    FiniArray = 15,
    PreInitArray = 16,
    Group = 17,
    SymTabShndx = 18,
    Loos = 0x60000000,
    Hios = 0x6fffffff,
    Loproc = 0x70000000,
    Hiproc = 0x7fffffff,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Elf32Header {
    /// ELF magic number: 0x7f, "ELF"
    pub magic: [u8; 4],
    pub format: ElfFormat,
    pub endian: ElfEndian,
    /// ELF version, should be 1
    pub version: u8,
    pub abi: ElfABI,
    pub abi_version: u8,
    padding: [u8; 7],
    pub elf_type: ElfType,
    pub arch: ElfArch,
    /// ELF version, should be 1
    pub version2: u32,
    pub entry: u32,
    pub ph_offset: u32,
    pub sh_offset: u32,
    pub flags: u32,
    pub eh_size: u16,
    pub ph_entry_size: u16,
    pub ph_entry_count: u16,
    pub sh_entry_size: u16,
    pub sh_entry_count: u16,
    pub sh_str_index: u16,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Elf32PhEntry {
    pub ph_type: Elf32PhType,
    pub offset: u32,
    pub vaddr: u32,
    pub paddr: u32,
    pub file_size: u32,
    pub mem_size: u32,
    pub flags: Elf32PhFlags,
    /// `0` and `1` for no alignment, otherwise power of `2`
    pub align: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Elf32ShEntry {
    pub name_offset: u32,
    pub sh_type: Elf32ShType,
    pub flags: Elf32ShFlags,
    pub addr: u32,
    pub offset: u32,
    pub size: u32,
    pub link: u32,
    pub info: u32,
    pub addr_align: u32,
    pub entry_size: u32,
}

#[allow(dead_code)]
pub struct ParsedElf32 {
    entry: u32,
    file: Arc<Dentry>,
    phents: Vec<Elf32PhEntry>,
    shents: Vec<Elf32ShEntry>,
}

const ELF_MAGIC: [u8; 4] = *b"\x7fELF";

impl Elf32Header {
    fn check_valid(&self) -> bool {
        self.magic == ELF_MAGIC
            && self.version == 1
            && self.version2 == 1
            && self.eh_size as usize == size_of::<Elf32Header>()
            && self.ph_entry_size as usize == size_of::<Elf32PhEntry>()
            && self.sh_entry_size as usize == size_of::<Elf32ShEntry>()
    }
}

impl ParsedElf32 {
    pub fn parse(file: Arc<Dentry>) -> KResult<Self> {
        let mut header = UninitBuffer::<Elf32Header>::new();
        file.read(&mut header, 0)?;

        let header = header.assume_init().map_err(|_| ENOEXEC)?;
        if !header.check_valid() {
            return Err(ENOEXEC);
        }

        // TODO: Use `UninitBuffer` for `phents` and `shents`.
        let mut phents = vec![Elf32PhEntry::default(); header.ph_entry_count as usize];
        let nread = file.read(
            &mut ByteBuffer::from(phents.as_mut_slice()),
            header.ph_offset as usize,
        )?;
        if nread != header.ph_entry_count as usize * size_of::<Elf32PhEntry>() {
            return Err(ENOEXEC);
        }

        let mut shents = vec![Elf32ShEntry::default(); header.sh_entry_count as usize];
        let nread = file.read(
            &mut ByteBuffer::from(shents.as_mut_slice()),
            header.sh_offset as usize,
        )?;
        if nread != header.sh_entry_count as usize * size_of::<Elf32ShEntry>() {
            return Err(ENOEXEC);
        }

        Ok(Self {
            entry: header.entry,
            file,
            phents,
            shents,
        })
    }

    /// Load the ELF file into memory. Return the entry point address and the memory list containing the program data.
    ///
    /// We clear the user space and load the program headers into memory.
    /// Can't make a way back if failed from now on.
    ///
    /// # Return
    /// `(entry_ip, sp, mm_list)`
    pub fn load(self, args: Vec<CString>, envs: Vec<CString>) -> KResult<(VAddr, VAddr, MMList)> {
        let mm_list = MMList::new();

        let mut data_segment_end = VAddr::NULL;
        for phent in self
            .phents
            .into_iter()
            .filter(|ent| ent.ph_type == Elf32PhType::Load)
        {
            let vaddr_start = VAddr::from(phent.vaddr as usize);
            let vmem_vaddr_end = vaddr_start + phent.mem_size as usize;
            let load_vaddr_end = vaddr_start + phent.file_size as usize;

            let vaddr = vaddr_start.floor();
            let vmem_len = vmem_vaddr_end.ceil() - vaddr;
            let file_len = load_vaddr_end.ceil() - vaddr;
            let file_offset = phent.offset as usize & !0xfff;

            let permission = Permission {
                write: phent.flags.contains(Elf32PhFlags::Write),
                execute: phent.flags.contains(Elf32PhFlags::Exec),
            };

            if file_len != 0 {
                let real_file_length = load_vaddr_end - vaddr;
                mm_list.mmap_fixed(
                    vaddr,
                    file_len,
                    Mapping::File(FileMapping::new(
                        self.file.clone(),
                        file_offset,
                        real_file_length,
                    )),
                    permission,
                )?;
            }

            if vmem_len > file_len {
                mm_list.mmap_fixed(
                    vaddr + file_len,
                    vmem_len - file_len,
                    Mapping::Anonymous,
                    permission,
                )?;
            }

            if vaddr + vmem_len > data_segment_end {
                data_segment_end = vaddr + vmem_len;
            }
        }

        mm_list.register_break(data_segment_end + 0x10000);

        // Map stack area
        mm_list.mmap_fixed(
            VAddr::from(0xc0000000 - 0x800000), // Stack bottom is at 0xc0000000
            0x800000,                           // 8MB stack size
            Mapping::Anonymous,
            Permission {
                write: true,
                execute: false,
            },
        )?;

        let mut sp = VAddr::from(0xc0000000); // Current stack top
        let arg_addrs = push_strings(&mm_list, &mut sp, args)?;
        let env_addrs = push_strings(&mm_list, &mut sp, envs)?;

        let mut longs = vec![];
        longs.push(arg_addrs.len() as u32); // argc
        longs.extend(arg_addrs.into_iter()); // args
        longs.push(0); // null
        longs.extend(env_addrs.into_iter()); // envs
        longs.push(0); // null
        longs.push(0); // AT_NULL
        longs.push(0); // AT_NULL

        sp = sp - longs.len() * size_of::<u32>();
        sp = sp.floor_to(16);

        mm_list.access_mut(sp, longs.len() * size_of::<u32>(), |offset, data| {
            data.copy_from_slice(unsafe {
                core::slice::from_raw_parts(
                    longs.as_ptr().byte_add(offset) as *const u8,
                    data.len(),
                )
            })
        })?;

        Ok((VAddr::from(self.entry as usize), sp, mm_list))
    }
}

fn push_strings(mm_list: &MMList, sp: &mut VAddr, strings: Vec<CString>) -> KResult<Vec<u32>> {
    let mut addrs = vec![];
    for string in strings {
        let len = string.as_bytes_with_nul().len();
        *sp = *sp - len;
        mm_list.access_mut(*sp, len, |offset, data| {
            data.copy_from_slice(&string.as_bytes_with_nul()[offset..offset + data.len()])
        })?;
        addrs.push(sp.addr() as u32);
    }

    Ok(addrs)
}
