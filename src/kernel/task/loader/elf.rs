use super::{LoadInfo, ELF_MAGIC};

use crate::io::UninitBuffer;
use crate::kernel::task::loader::aux_vec::{AuxKey, AuxVec};
use crate::path::Path;
use crate::{
    io::ByteBuffer,
    kernel::{
        constants::ENOEXEC,
        mem::{FileMapping, MMList, Mapping, Permission},
        vfs::{dentry::Dentry, FsContext},
    },
    prelude::*,
};
use align_ext::AlignExt;
use alloc::vec::Vec;
use alloc::{ffi::CString, sync::Arc};
use eonix_mm::{
    address::{Addr, AddrOps as _, VAddr},
    paging::PAGE_SIZE,
};

use xmas_elf::{
    header::{self, Class, HeaderPt1, Machine_, Type_},
    program::{self, ProgramHeader32, ProgramHeader64},
    P32, P64,
};

const INIT_STACK_SIZE: usize = 0x80_0000;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct HeaderPt2<P> {
    pub type_: Type_,
    pub machine: Machine_,
    pub version: u32,
    pub entry_point: P,
    pub ph_offset: P,
    pub sh_offset: P,
    pub flags: u32,
    pub header_size: u16,
    pub ph_entry_size: u16,
    pub ph_count: u16,
    pub sh_entry_size: u16,
    pub sh_count: u16,
    pub sh_str_index: u16,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ElfHeader<P> {
    pub pt1: HeaderPt1,
    pub pt2: HeaderPt2<P>,
}

pub struct LdsoLoadInfo {
    pub base: VAddr,
    pub entry_ip: VAddr,
}

pub struct Elf32 {
    pub file: Arc<Dentry>,
    pub elf_header: ElfHeader<P32>,
    pub program_headers: Vec<ProgramHeader32>,
}

impl Elf32 {
    const DYN_BASE_ADDR: usize = 0x4000_0000;
    const LDSO_BASE_ADDR: usize = 0xf000_0000;
    const STACK_BASE_ADDR: usize = 0xffff_0000;

    pub fn parse(elf_file: Arc<Dentry>) -> KResult<Self> {
        let mut elf_header = UninitBuffer::<ElfHeader<P32>>::new();
        elf_file.read(&mut elf_header, 0)?;

        let elf_header = elf_header.assume_init().map_err(|_| ENOEXEC)?;

        let ph_offset = elf_header.pt2.ph_offset;
        let ph_count = elf_header.pt2.ph_count;

        let mut program_headers = vec![ProgramHeader32::default(); ph_count as usize];
        elf_file.read(
            &mut ByteBuffer::from(program_headers.as_mut_slice()),
            ph_offset as usize,
        )?;

        Ok(Self {
            file: elf_file,
            elf_header,
            program_headers,
        })
    }

    fn is_shared_object(&self) -> bool {
        self.elf_header.pt2.type_.as_type() == header::Type::SharedObject
    }

    fn entry_point(&self) -> u32 {
        self.elf_header.pt2.entry_point
    }

    fn ph_count(&self) -> u16 {
        self.elf_header.pt2.ph_count
    }

    fn ph_offset(&self) -> u32 {
        self.elf_header.pt2.ph_offset
    }

    fn ph_entry_size(&self) -> u16 {
        self.elf_header.pt2.ph_entry_size
    }

    fn ph_addr(&self) -> KResult<u32> {
        let ph_offset = self.ph_offset();
        for program_header in &self.program_headers {
            if program_header.offset <= ph_offset
                && ph_offset < program_header.offset + program_header.file_size
            {
                return Ok(ph_offset - program_header.offset + program_header.virtual_addr);
            }
        }
        Err(ENOEXEC)
    }

    pub fn load(&self, args: Vec<CString>, envs: Vec<CString>) -> KResult<LoadInfo> {
        let mm_list = MMList::new();

        // Load Segments
        let (elf_base, data_segment_end) = self.load_segments(&mm_list)?;

        // Load ldso(if have)
        let ldso_load_info = self.load_ldso(&mm_list)?;

        // Heap
        mm_list.register_break(data_segment_end + 0x10000);

        let aux_vec = Elf32::init_aux_vec(
            self,
            elf_base,
            ldso_load_info
                .as_ref()
                .map(|ldso_load_info| ldso_load_info.base),
        )?;

        // Map stack
        let sp = Elf32::create_and_init_stack(&mm_list, args, envs, aux_vec)?;

        let entry_ip = if let Some(ldso_load_info) = ldso_load_info {
            // Normal shared object(DYN)
            ldso_load_info.entry_ip.into()
        } else if self.is_shared_object() {
            // ldso itself
            elf_base + self.entry_point() as usize
        } else {
            // statically linked executable
            (self.entry_point() as usize).into()
        };

        Ok(LoadInfo {
            entry_ip,
            sp,
            mm_list,
        })
    }

    fn init_aux_vec(
        elf: &Elf32,
        elf_base: VAddr,
        ldso_base: Option<VAddr>,
    ) -> KResult<AuxVec<u32>> {
        let mut aux_vec: AuxVec<u32> = AuxVec::new();
        let ph_addr = if elf.is_shared_object() {
            elf_base.addr() as u32 + elf.ph_addr()?
        } else {
            elf.ph_addr()?
        };
        aux_vec.set(AuxKey::AT_PAGESZ, PAGE_SIZE as u32)?;
        aux_vec.set(AuxKey::AT_PHDR, ph_addr)?;
        aux_vec.set(AuxKey::AT_PHNUM, elf.ph_count() as u32)?;
        aux_vec.set(AuxKey::AT_PHENT, elf.ph_entry_size() as u32)?;
        let elf_entry = if elf.is_shared_object() {
            elf_base.addr() as u32 + elf.entry_point()
        } else {
            elf.entry_point()
        };
        aux_vec.set(AuxKey::AT_ENTRY, elf_entry)?;

        if let Some(ldso_base) = ldso_base {
            aux_vec.set(AuxKey::AT_BASE, ldso_base.addr() as u32)?;
        }
        Ok(aux_vec)
    }

    pub fn load_segments(&self, mm_list: &MMList) -> KResult<(VAddr, VAddr)> {
        let base: VAddr = if self.is_shared_object() {
            Elf32::DYN_BASE_ADDR
        } else {
            0
        }
        .into();

        let mut segments_end = VAddr::NULL;

        for program_header in &self.program_headers {
            let type_ = program_header.get_type().map_err(|_| ENOEXEC)?;

            if type_ == program::Type::Load {
                let segment_end = self.load_segment(program_header, mm_list, base)?;

                if segment_end > segments_end {
                    segments_end = segment_end;
                }
            }
        }

        Ok((base, segments_end))
    }

    pub fn load_segment(
        &self,
        program_header: &ProgramHeader32,
        mm_list: &MMList,
        base_addr: VAddr,
    ) -> KResult<VAddr> {
        let virtual_addr = base_addr + program_header.virtual_addr as usize;
        let vmem_vaddr_end = virtual_addr + program_header.mem_size as usize;
        let load_vaddr_end = virtual_addr + program_header.file_size as usize;

        let vmap_start = virtual_addr.floor();
        let vmem_len = vmem_vaddr_end.ceil() - vmap_start;
        let file_len = load_vaddr_end.ceil() - vmap_start;
        let file_offset = (program_header.offset as usize).align_down(PAGE_SIZE);

        let permission = Permission {
            read: program_header.flags.is_read(),
            write: program_header.flags.is_write(),
            execute: program_header.flags.is_execute(),
        };

        if file_len != 0 {
            let real_file_length = load_vaddr_end - vmap_start;

            mm_list.mmap_fixed(
                vmap_start,
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
                vmap_start + file_len,
                vmem_len - file_len,
                Mapping::Anonymous,
                permission,
            )?;
        }

        Ok(vmap_start + vmem_len)
    }

    pub fn load_ldso(&self, mm_list: &MMList) -> KResult<Option<LdsoLoadInfo>> {
        let ldso_path = self.ldso_path()?;

        if let Some(ldso_path) = ldso_path {
            let fs_context = FsContext::global();
            let ldso_file =
                Dentry::open(fs_context, Path::new(ldso_path.as_bytes()).unwrap(), true).unwrap();
            let ldso_elf = Elf32::parse(ldso_file).unwrap();

            let base = VAddr::from(Elf32::LDSO_BASE_ADDR);

            for program_header in &ldso_elf.program_headers {
                let type_ = program_header.get_type().map_err(|_| ENOEXEC)?;

                if type_ == program::Type::Load {
                    ldso_elf.load_segment(program_header, mm_list, base)?;
                }
            }

            return Ok(Some(LdsoLoadInfo {
                base,
                entry_ip: base + ldso_elf.entry_point() as usize,
            }));
        }

        Ok(None)
    }

    fn ldso_path(&self) -> KResult<Option<String>> {
        for program_header in &self.program_headers {
            let type_ = program_header.get_type().map_err(|_| ENOEXEC)?;

            if type_ == program::Type::Interp {
                let file_size = program_header.file_size as usize;
                let file_offset = program_header.offset as usize;

                let mut ldso_vec = vec![0u8; file_size - 1]; // -1 due to '\0'
                self.file
                    .read(&mut ByteBuffer::from(ldso_vec.as_mut_slice()), file_offset)?;
                let ldso_path = String::from_utf8(ldso_vec).map_err(|_| ENOEXEC)?;
                return Ok(Some(ldso_path));
            }
        }
        Ok(None)
    }

    fn create_and_init_stack(
        mm_list: &MMList,
        args: Vec<CString>,
        envs: Vec<CString>,
        aux_vec: AuxVec<u32>,
    ) -> KResult<VAddr> {
        mm_list.mmap_fixed(
            VAddr::from(Elf32::STACK_BASE_ADDR - INIT_STACK_SIZE),
            INIT_STACK_SIZE,
            Mapping::Anonymous,
            Permission {
                read: true,
                write: true,
                execute: false,
            },
        )?;

        let mut sp = VAddr::from(Elf32::STACK_BASE_ADDR);

        let env_pointers = Elf32::push_strings(&mm_list, &mut sp, envs)?;
        let arg_pointers = Elf32::push_strings(&mm_list, &mut sp, args)?;

        let argc = arg_pointers.len() as u32;

        Elf32::stack_alignment(&mut sp, &arg_pointers, &env_pointers, &aux_vec);
        Elf32::push_aux_vec(&mm_list, &mut sp, aux_vec)?;
        Elf32::push_pointers(&mm_list, &mut sp, env_pointers)?;
        Elf32::push_pointers(&mm_list, &mut sp, arg_pointers)?;
        Elf32::push_u32(&mm_list, &mut sp, argc)?;

        assert_eq!(sp.floor_to(16), sp);
        Ok(sp)
    }

    fn stack_alignment(
        sp: &mut VAddr,
        arg_pointers: &Vec<u32>,
        env_pointers: &Vec<u32>,
        aux_vec: &AuxVec<u32>,
    ) {
        let aux_vec_size = (aux_vec.table().len() + 1) * (size_of::<u32>() * 2);
        let envp_pointers_size = (env_pointers.len() + 1) * size_of::<u32>();
        let argv_pointers_size = (arg_pointers.len() + 1) * size_of::<u32>();
        let argc_size = size_of::<u32>();
        let all_size = aux_vec_size + envp_pointers_size + argv_pointers_size + argc_size;

        let align_sp = (sp.addr() - all_size).align_down(16);
        *sp = VAddr::from(align_sp + all_size);
    }

    fn push_strings(mm_list: &MMList, sp: &mut VAddr, strings: Vec<CString>) -> KResult<Vec<u32>> {
        let mut addrs = Vec::with_capacity(strings.len());
        for string in strings.iter().rev() {
            let len = string.as_bytes_with_nul().len();
            *sp = *sp - len;
            mm_list.access_mut(*sp, len, |offset, data| {
                data.copy_from_slice(&string.as_bytes_with_nul()[offset..offset + data.len()])
            })?;
            addrs.push(sp.addr() as u32);
        }
        addrs.reverse();
        Ok(addrs)
    }

    fn push_pointers(mm_list: &MMList, sp: &mut VAddr, mut pointers: Vec<u32>) -> KResult<()> {
        pointers.push(0);
        *sp = *sp - pointers.len() * size_of::<u32>();

        mm_list.access_mut(*sp, pointers.len() * size_of::<u32>(), |offset, data| {
            data.copy_from_slice(unsafe {
                core::slice::from_raw_parts(
                    pointers.as_ptr().byte_add(offset) as *const u8,
                    data.len(),
                )
            })
        })?;

        Ok(())
    }

    fn push_u32(mm_list: &MMList, sp: &mut VAddr, val: u32) -> KResult<()> {
        *sp = *sp - size_of::<u32>();

        mm_list.access_mut(*sp, size_of::<u32>(), |_, data| {
            data.copy_from_slice(unsafe {
                core::slice::from_raw_parts(&val as *const _ as *const u8, data.len())
            })
        })?;

        Ok(())
    }

    fn push_aux_vec(mm_list: &MMList, sp: &mut VAddr, aux_vec: AuxVec<u32>) -> KResult<()> {
        let mut longs: Vec<u32> = vec![];

        // Write Auxiliary vectors
        let aux_vec: Vec<_> = aux_vec
            .table()
            .iter()
            .map(|(aux_key, aux_value)| (*aux_key, *aux_value))
            .collect();

        for (aux_key, aux_value) in aux_vec.iter() {
            longs.push(*aux_key as u32);
            longs.push(*aux_value);
        }

        // Write NULL auxiliary
        longs.push(AuxKey::AT_NULL as u32);
        longs.push(0);

        *sp = *sp - longs.len() * size_of::<u32>();

        mm_list.access_mut(*sp, longs.len() * size_of::<u32>(), |offset, data| {
            data.copy_from_slice(unsafe {
                core::slice::from_raw_parts(
                    longs.as_ptr().byte_add(offset) as *const u8,
                    data.len(),
                )
            })
        })?;

        Ok(())
    }
}

pub struct Elf64 {
    elf_header: ElfHeader<P64>,
    program_headers: Vec<ProgramHeader64>,
}

impl Elf64 {
    // const LDSO_BASE_ADDR: usize = 0xffff00000000;
    // const STACK_BASE_ADDR: usize = 0xffff_ff00_0000;
    // const DYN_BASE_ADDR: usize = 0xaaaa00000000;

    fn parse(file: Arc<Dentry>) -> KResult<Self> {
        todo!()
    }

    fn load(&self, args: Vec<CString>, envs: Vec<CString>) -> KResult<LoadInfo> {
        todo!()
    }
}

pub enum Elf {
    ELF32(Elf32),
    ELF64(Elf64),
}

impl Elf {
    pub fn parse(elf_file: Arc<Dentry>) -> KResult<Self> {
        let mut header_pt1 = UninitBuffer::<HeaderPt1>::new();
        elf_file.read(&mut header_pt1, 0)?;

        let header_pt1 = header_pt1.assume_init().map_err(|_| ENOEXEC)?;
        assert_eq!(header_pt1.magic, ELF_MAGIC);

        match header_pt1.class() {
            Class::ThirtyTwo => Ok(Elf::ELF32(Elf32::parse(elf_file)?)),
            Class::SixtyFour => Ok(Elf::ELF64(Elf64::parse(elf_file)?)),
            _ => Err(ENOEXEC),
        }
    }

    pub fn load(&self, args: Vec<CString>, envs: Vec<CString>) -> KResult<LoadInfo> {
        match &self {
            Elf::ELF32(elf32) => elf32.load(args, envs),
            Elf::ELF64(elf64) => elf64.load(args, envs),
        }
    }
}
