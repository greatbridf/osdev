use crate::io::ByteBuffer;
use crate::kernel::constants::ENOEXEC;
use crate::kernel::task::loader::elf::ELF;
use crate::kernel::vfs::FsContext;
use crate::path::Path;
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
    args: Vec<CString>,
    envs: Vec<CString>,
    object: Object,
}

impl ProgramLoader {
    pub fn parse(
        fs_context: &FsContext,
        mut exec_path: CString,
        mut file: Arc<Dentry>,
        mut args: Vec<CString>,
        envs: Vec<CString>,
    ) -> KResult<Self> {
        const RECURSION_LIMIT: usize = 4;
        let mut nrecur = 0;

        let object = loop {
            if nrecur >= RECURSION_LIMIT {
                return Err(ENOEXEC);
            }

            let mut magic = [0; 4];
            file.read(&mut ByteBuffer::new(magic.as_mut_slice()), 0)?;

            match magic {
                [b'#', b'!', ..] => {
                    let mut interpreter_line = [0; 256];
                    let nread = file.read(&mut ByteBuffer::new(&mut interpreter_line), 0)?;

                    // There is a tiny time gap between reading the magic number and
                    // reading the interpreter line, so we need to check if the line
                    // still starts with a shebang.
                    if !interpreter_line.starts_with(b"#!") {
                        return Err(ENOEXEC);
                    }

                    let line = &interpreter_line[2..nread];
                    let line = line.split(|&b| b == b'\n' || b == b'\0').next().unwrap();

                    let interpreter_name;
                    let mut interpreter_arg = None;
                    match line.iter().position(|&b| b == b' ') {
                        Some(blank_pos) => {
                            interpreter_name = CString::new(&line[..blank_pos]).unwrap();
                            interpreter_arg = Some(CString::new(&line[blank_pos + 1..]).unwrap());
                        }
                        None => interpreter_name = CString::new(line).unwrap(),
                    }

                    let path = Path::new(interpreter_name.as_bytes())?;
                    file = Dentry::open(fs_context, path, true)?;

                    args.insert(0, interpreter_name.clone());
                    if let Some(arg) = interpreter_arg {
                        args.insert(1, arg);
                    }

                    if args.len() > 2 {
                        args[2] = exec_path;
                    } else {
                        args.push(exec_path);
                    }

                    exec_path = interpreter_name;
                }
                ELF_MAGIC => break ELF::parse(file)?,
                _ => return Err(ENOEXEC),
            }

            nrecur += 1;
        };

        Ok(ProgramLoader {
            args,
            envs,
            object: Object::ELF(object),
        })
    }

    pub fn load(self) -> KResult<LoadInfo> {
        match self.object {
            Object::ELF(elf) => elf.load(self.args, self.envs),
        }
    }
}
