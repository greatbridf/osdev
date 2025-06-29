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
    header::{self, Class, HeaderPt1, Machine_},
    program::{self, ProgramHeader32, ProgramHeader64},
};

const INIT_STACK_SIZE: usize = 0x80_0000;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct HeaderPt2<P> {
    type_: header::Type_,
    machine: Machine_,
    version: u32,
    entry_point: P,
    ph_offset: P,
    sh_offset: P,
    flags: u32,
    header_size: u16,
    ph_entry_size: u16,
    ph_count: u16,
    sh_entry_size: u16,
    sh_count: u16,
    sh_str_index: u16,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct ElfHeader<P> {
    pt1: HeaderPt1,
    pt2: HeaderPt2<P>,
}

pub trait ProgramHeader {
    fn offset(&self) -> usize;
    fn file_size(&self) -> usize;
    fn virtual_addr(&self) -> usize;
    fn mem_size(&self) -> usize;
    fn flags(&self) -> program::Flags;
    fn type_(&self) -> KResult<program::Type>;
}

macro_rules! impl_program_header {
    ($ProgramHeaderN:ident) => {
        impl ProgramHeader for $ProgramHeaderN {
            fn offset(&self) -> usize {
                self.offset as usize
            }

            fn file_size(&self) -> usize {
                self.file_size as usize
            }

            fn virtual_addr(&self) -> usize {
                self.virtual_addr as usize
            }

            fn mem_size(&self) -> usize {
                self.mem_size as usize
            }

            fn flags(&self) -> program::Flags {
                self.flags
            }

            fn type_(&self) -> KResult<program::Type> {
                self.get_type().map_err(|_| ENOEXEC)
            }
        }
    };
}

impl_program_header!(ProgramHeader32);
impl_program_header!(ProgramHeader64);

struct LdsoLoadInfo {
    base: VAddr,
    entry_ip: VAddr,
}

pub trait ElfAddr {
    type Integer;

    fn from_usize(val: usize) -> Self;
    fn into_usize(&self) -> usize;
}

macro_rules! impl_elf_addr {
    ($Type:ident) => {
        impl ElfAddr for $Type {
            type Integer = $Type;

            fn from_usize(val: usize) -> Self {
                val as Self::Integer
            }

            fn into_usize(&self) -> usize {
                *self as usize
            }
        }
    };
}

impl_elf_addr!(u32);
impl_elf_addr!(u64);

pub trait ElfArch {
    type Ea: ElfAddr + Clone + Copy;
    type Ph: ProgramHeader + Clone + Copy + Default;

    const DYN_BASE_ADDR: usize;
    const LDSO_BASE_ADDR: usize;
    const STACK_BASE_ADDR: usize;
}

pub struct ElfArch32;

pub struct ElfArch64;

pub struct Elf<E: ElfArch> {
    file: Arc<Dentry>,
    elf_header: ElfHeader<E::Ea>,
    program_headers: Vec<E::Ph>,
}

impl ElfArch for ElfArch32 {
    type Ea = u32;
    type Ph = ProgramHeader32;

    const DYN_BASE_ADDR: usize = 0x4000_0000;
    const LDSO_BASE_ADDR: usize = 0xf000_0000;
    const STACK_BASE_ADDR: usize = 0xffff_0000;
}

impl ElfArch for ElfArch64 {
    type Ea = u64;
    type Ph = ProgramHeader64;

    const DYN_BASE_ADDR: usize = 0x2aaa_0000_0000;
    const LDSO_BASE_ADDR: usize = 0x7000_0000_0000;
    const STACK_BASE_ADDR: usize = 0x7fff_ffff_0000;
}

impl<E: ElfArch> Elf<E> {
    fn is_shared_object(&self) -> bool {
        self.elf_header.pt2.type_.as_type() == header::Type::SharedObject
    }

    fn entry_point(&self) -> usize {
        self.elf_header.pt2.entry_point.into_usize()
    }

    fn ph_count(&self) -> usize {
        self.elf_header.pt2.ph_count as usize
    }

    fn ph_offset(&self) -> usize {
        self.elf_header.pt2.ph_offset.into_usize()
    }

    fn ph_entry_size(&self) -> usize {
        self.elf_header.pt2.ph_entry_size as usize
    }

    fn ph_addr(&self) -> KResult<usize> {
        let ph_offset = self.ph_offset();
        for program_header in &self.program_headers {
            if program_header.offset() <= ph_offset
                && ph_offset < program_header.offset() + program_header.file_size()
            {
                return Ok(ph_offset - program_header.offset() + program_header.virtual_addr());
            }
        }
        Err(ENOEXEC)
    }

    fn parse(elf_file: Arc<Dentry>) -> KResult<Self> {
        let mut elf_header = UninitBuffer::<ElfHeader<E::Ea>>::new();
        elf_file.read(&mut elf_header, 0)?;

        let elf_header = elf_header.assume_init().map_err(|_| ENOEXEC)?;

        let ph_offset = elf_header.pt2.ph_offset;
        let ph_count = elf_header.pt2.ph_count;

        let mut program_headers = vec![E::Ph::default(); ph_count as usize];
        elf_file.read(
            &mut ByteBuffer::from(program_headers.as_mut_slice()),
            ph_offset.into_usize(),
        )?;

        Ok(Self {
            file: elf_file,
            elf_header,
            program_headers,
        })
    }

    fn load(&self, args: Vec<CString>, envs: Vec<CString>) -> KResult<LoadInfo> {
        let mm_list = MMList::new();

        // Load Segments
        let (elf_base, data_segment_end) = self.load_segments(&mm_list)?;

        // Load ldso (if any)
        let ldso_load_info = self.load_ldso(&mm_list)?;

        // Load vdso
        self.load_vdso(&mm_list)?;

        // Heap
        mm_list.register_break(data_segment_end + 0x10000);

        let aux_vec = self.init_aux_vec(
            elf_base,
            ldso_load_info
                .as_ref()
                .map(|ldso_load_info| ldso_load_info.base),
        )?;

        // Map stack
        let sp = self.create_and_init_stack(&mm_list, args, envs, aux_vec)?;

        let entry_ip = if let Some(ldso_load_info) = ldso_load_info {
            // Normal shared object(DYN)
            ldso_load_info.entry_ip.into()
        } else if self.is_shared_object() {
            // ldso itself
            elf_base + self.entry_point()
        } else {
            // statically linked executable
            self.entry_point().into()
        };

        Ok(LoadInfo {
            entry_ip,
            sp,
            mm_list,
        })
    }

    fn create_and_init_stack(
        &self,
        mm_list: &MMList,
        args: Vec<CString>,
        envs: Vec<CString>,
        aux_vec: AuxVec<E::Ea>,
    ) -> KResult<VAddr> {
        mm_list.mmap_fixed(
            VAddr::from(E::STACK_BASE_ADDR - INIT_STACK_SIZE),
            INIT_STACK_SIZE,
            Mapping::Anonymous,
            Permission {
                read: true,
                write: true,
                execute: false,
            },
            false,
        )?;

        StackInitializer::new(&mm_list, E::STACK_BASE_ADDR, args, envs, aux_vec).init()
    }

    fn init_aux_vec(&self, elf_base: VAddr, ldso_base: Option<VAddr>) -> KResult<AuxVec<E::Ea>> {
        let mut aux_vec: AuxVec<E::Ea> = AuxVec::new();
        let ph_addr = if self.is_shared_object() {
            elf_base.addr() + self.ph_addr()?
        } else {
            self.ph_addr()?
        };

        aux_vec.set(AuxKey::AT_PAGESZ, E::Ea::from_usize(PAGE_SIZE))?;
        aux_vec.set(AuxKey::AT_PHDR, E::Ea::from_usize(ph_addr))?;
        aux_vec.set(AuxKey::AT_PHNUM, E::Ea::from_usize(self.ph_count()))?;
        aux_vec.set(AuxKey::AT_PHENT, E::Ea::from_usize(self.ph_entry_size()))?;
        let elf_entry = if self.is_shared_object() {
            elf_base.addr() + self.entry_point()
        } else {
            self.entry_point()
        };
        aux_vec.set(AuxKey::AT_ENTRY, E::Ea::from_usize(elf_entry))?;
        aux_vec.set(
            AuxKey::AT_RANDOM,
            E::Ea::from_usize(E::STACK_BASE_ADDR - 16),
        )?;

        if let Some(ldso_base) = ldso_base {
            aux_vec.set(AuxKey::AT_BASE, E::Ea::from_usize(ldso_base.addr()))?;
        }
        Ok(aux_vec)
    }

    fn load_segments(&self, mm_list: &MMList) -> KResult<(VAddr, VAddr)> {
        let base: VAddr = if self.is_shared_object() { E::DYN_BASE_ADDR } else { 0 }.into();

        let mut segments_end = VAddr::NULL;

        for program_header in &self.program_headers {
            let type_ = program_header.type_().map_err(|_| ENOEXEC)?;

            if type_ == program::Type::Load {
                let segment_end = self.load_segment(program_header, mm_list, base)?;

                if segment_end > segments_end {
                    segments_end = segment_end;
                }
            }
        }

        Ok((base, segments_end))
    }

    fn load_segment(
        &self,
        program_header: &E::Ph,
        mm_list: &MMList,
        base_addr: VAddr,
    ) -> KResult<VAddr> {
        let virtual_addr = base_addr + program_header.virtual_addr();
        let vmem_vaddr_end = virtual_addr + program_header.mem_size();
        let load_vaddr_end = virtual_addr + program_header.file_size();

        let vmap_start = virtual_addr.floor();
        let vmem_len = vmem_vaddr_end.ceil() - vmap_start;
        let file_len = load_vaddr_end.ceil() - vmap_start;
        let file_offset = (program_header.offset()).align_down(PAGE_SIZE);

        let permission = Permission {
            read: program_header.flags().is_read(),
            write: program_header.flags().is_write(),
            execute: program_header.flags().is_execute(),
        };

        if file_len != 0 {
            let real_file_length = load_vaddr_end - vmap_start;

            mm_list.mmap_fixed(
                vmap_start,
                file_len,
                Mapping::File(FileMapping::new(
                    self.file.get_inode()?,
                    file_offset,
                    real_file_length,
                )),
                permission,
                false,
            )?;
        }

        if vmem_len > file_len {
            mm_list.mmap_fixed(
                vmap_start + file_len,
                vmem_len - file_len,
                Mapping::Anonymous,
                permission,
                false,
            )?;
        }

        Ok(vmap_start + vmem_len)
    }

    fn load_ldso(&self, mm_list: &MMList) -> KResult<Option<LdsoLoadInfo>> {
        let ldso_path = self.ldso_path()?;

        if let Some(ldso_path) = ldso_path {
            let fs_context = FsContext::global();
            let ldso_file = Dentry::open(fs_context, Path::new(ldso_path.as_bytes())?, true)?;
            let ldso_elf = Elf::<E>::parse(ldso_file)?;

            let base = VAddr::from(E::LDSO_BASE_ADDR);

            for program_header in &ldso_elf.program_headers {
                let type_ = program_header.type_().map_err(|_| ENOEXEC)?;

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

    fn load_vdso(&self, mm_list: &MMList) -> KResult<()> {
        mm_list.map_vdso()
    }

    fn ldso_path(&self) -> KResult<Option<String>> {
        for program_header in &self.program_headers {
            let type_ = program_header.type_().map_err(|_| ENOEXEC)?;

            if type_ == program::Type::Interp {
                let file_size = program_header.file_size();
                let file_offset = program_header.offset();

                let mut ldso_vec = vec![0u8; file_size - 1]; // -1 due to '\0'
                self.file
                    .read(&mut ByteBuffer::from(ldso_vec.as_mut_slice()), file_offset)?;
                let ldso_path = String::from_utf8(ldso_vec).map_err(|_| ENOEXEC)?;
                return Ok(Some(ldso_path));
            }
        }
        Ok(None)
    }
}

pub enum ELF {
    Elf32(Elf<ElfArch32>),
    Elf64(Elf<ElfArch64>),
}

impl ELF {
    pub fn parse(elf_file: Arc<Dentry>) -> KResult<Self> {
        let mut header_pt1 = UninitBuffer::<HeaderPt1>::new();
        elf_file.read(&mut header_pt1, 0)?;

        let header_pt1 = header_pt1.assume_init().map_err(|_| ENOEXEC)?;
        assert_eq!(header_pt1.magic, ELF_MAGIC);

        match header_pt1.class() {
            Class::ThirtyTwo => Ok(ELF::Elf32(Elf::parse(elf_file)?)),
            Class::SixtyFour => Ok(ELF::Elf64(Elf::parse(elf_file)?)),
            _ => Err(ENOEXEC),
        }
    }

    pub fn load(&self, args: Vec<CString>, envs: Vec<CString>) -> KResult<LoadInfo> {
        match &self {
            ELF::Elf32(elf32) => elf32.load(args, envs),
            ELF::Elf64(elf64) => elf64.load(args, envs),
        }
    }
}

struct StackInitializer<'a, T> {
    mm_list: &'a MMList,
    sp: usize,
    args: Vec<CString>,
    envs: Vec<CString>,
    aux_vec: AuxVec<T>,
}

impl<'a, T: ElfAddr + Clone + Copy> StackInitializer<'a, T> {
    fn new(
        mm_list: &'a MMList,
        sp: usize,
        args: Vec<CString>,
        envs: Vec<CString>,
        aux_vec: AuxVec<T>,
    ) -> Self {
        Self {
            mm_list,
            sp,
            args,
            envs,
            aux_vec,
        }
    }

    // return sp after stack init
    fn init(mut self) -> KResult<VAddr> {
        let env_pointers = self.push_envs()?;
        let arg_pointers = self.push_args()?;

        self.stack_alignment();
        self.push_aux_vec()?;
        self.push_pointers(env_pointers)?;
        self.push_pointers(arg_pointers)?;
        self.push_argc(T::from_usize(self.args.len()))?;

        assert_eq!(self.sp.align_down(16), self.sp);
        Ok(VAddr::from(self.sp))
    }

    fn push_envs(&mut self) -> KResult<Vec<T>> {
        let mut addrs = Vec::with_capacity(self.envs.len());
        for string in self.envs.iter().rev() {
            let len = string.as_bytes_with_nul().len();
            self.sp -= len;
            self.mm_list
                .access_mut(VAddr::from(self.sp), len, |offset, data| {
                    data.copy_from_slice(&string.as_bytes_with_nul()[offset..offset + data.len()])
                })?;
            addrs.push(T::from_usize(self.sp));
        }
        addrs.reverse();
        Ok(addrs)
    }

    fn push_args(&mut self) -> KResult<Vec<T>> {
        let mut addrs = Vec::with_capacity(self.args.len());
        for string in self.args.iter().rev() {
            let len = string.as_bytes_with_nul().len();
            self.sp -= len;
            self.mm_list
                .access_mut(VAddr::from(self.sp), len, |offset, data| {
                    data.copy_from_slice(&string.as_bytes_with_nul()[offset..offset + data.len()])
                })?;
            addrs.push(T::from_usize(self.sp));
        }
        addrs.reverse();
        Ok(addrs)
    }

    fn stack_alignment(&mut self) {
        let aux_vec_size = (self.aux_vec.table().len() + 1) * (size_of::<T>() * 2);
        let envp_pointers_size = (self.envs.len() + 1) * size_of::<T>();
        let argv_pointers_size = (self.args.len() + 1) * size_of::<T>();
        let argc_size = size_of::<T>();
        let all_size = aux_vec_size + envp_pointers_size + argv_pointers_size + argc_size;

        let align_sp = (self.sp - all_size).align_down(16);
        self.sp = align_sp + all_size;
    }

    fn push_pointers(&mut self, mut pointers: Vec<T>) -> KResult<()> {
        pointers.push(T::from_usize(0));
        self.sp -= pointers.len() * size_of::<T>();

        self.mm_list.access_mut(
            VAddr::from(self.sp),
            pointers.len() * size_of::<T>(),
            |offset, data| {
                data.copy_from_slice(unsafe {
                    core::slice::from_raw_parts(
                        pointers.as_ptr().byte_add(offset) as *const u8,
                        data.len(),
                    )
                })
            },
        )?;

        Ok(())
    }

    fn push_argc(&mut self, val: T) -> KResult<()> {
        self.sp -= size_of::<T>();

        self.mm_list
            .access_mut(VAddr::from(self.sp), size_of::<u32>(), |_, data| {
                data.copy_from_slice(unsafe {
                    core::slice::from_raw_parts(&val as *const _ as *const u8, data.len())
                })
            })?;

        Ok(())
    }

    fn push_aux_vec(&mut self) -> KResult<()> {
        let mut longs: Vec<T> = vec![];

        // Write Auxiliary vectors
        let aux_vec: Vec<_> = self
            .aux_vec
            .table()
            .iter()
            .map(|(aux_key, aux_value)| (*aux_key, *aux_value))
            .collect();

        for (aux_key, aux_value) in aux_vec.iter() {
            longs.push(T::from_usize(*aux_key as usize));
            longs.push(*aux_value);
        }

        // Write NULL auxiliary
        longs.push(T::from_usize(AuxKey::AT_NULL as usize));
        longs.push(T::from_usize(0));

        self.sp -= longs.len() * size_of::<T>();

        self.mm_list.access_mut(
            VAddr::from(self.sp),
            longs.len() * size_of::<T>(),
            |offset, data| {
                data.copy_from_slice(unsafe {
                    core::slice::from_raw_parts(
                        longs.as_ptr().byte_add(offset) as *const u8,
                        data.len(),
                    )
                })
            },
        )?;

        Ok(())
    }
}
