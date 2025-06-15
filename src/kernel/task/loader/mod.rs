use crate::io::ByteBuffer;
use crate::kernel::constants::ENOEXEC;
use crate::kernel::task::loader::elf::ELF;
use crate::{
    kernel::{mem::MMList, vfs::dentry::Dentry},
    prelude::*,
};

use alloc::{ffi::CString, sync::Arc};
use eonix_mm::address::VAddr;

mod aux_vec;
mod elf;

const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];

#[derive(Debug)]
pub struct LoadInfo {
    pub entry_ip: VAddr,
    pub sp: VAddr,
    pub mm_list: MMList,
}

enum Object {
    ELF(ELF),
}

pub struct ProgramLoader {
    object: Object,
}

impl ProgramLoader {
    pub fn parse(file: Arc<Dentry>) -> KResult<Self> {
        let mut magic = [0u8; 4];
        file.read(&mut ByteBuffer::new(magic.as_mut_slice()), 0)?;

        let object = match magic {
            ELF_MAGIC => ELF::parse(file),
            _ => Err(ENOEXEC),
        }?;

        Ok(ProgramLoader {
            object: Object::ELF(object),
        })
    }

    pub fn load(&self, args: Vec<CString>, envs: Vec<CString>) -> KResult<LoadInfo> {
        match &self.object {
            Object::ELF(elf) => elf.load(args, envs),
        }
    }
}
