use core::fmt;

use eonix_hal::arch_exported::gdt::{GDTEntry, GDT};
use eonix_hal::processor::CPU;
use eonix_mm::address::VAddr;
use posix_types::ctypes::PtrT;
use posix_types::x86_64::UserDescriptor;

use crate::kernel::syscall::{User, UserMut};
use crate::kernel::user::{CheckedUserPointer, UserPointerMut};
use crate::prelude::KResult;

#[derive(Debug, Clone)]
pub struct UserTLS {
    desc: GDTEntry,
    base: u64,
}

pub struct UserTLSDescriptor<'a> {
    ptr: UserPointerMut<'a, UserDescriptor>,
}

impl UserTLS {
    fn new(base: u32, limit: u32) -> Self {
        Self {
            desc: GDTEntry::new_tls(base, limit),
            base: base as u64,
        }
    }

    fn new_page_limit(base: u32, limit_in_pages: u32) -> Self {
        Self {
            desc: GDTEntry::new_tls_page_limit(base, limit_in_pages),
            base: base as u64,
        }
    }

    pub fn activate(&self) {
        CPU::local().as_mut().set_tls32(self.desc, self.base);
    }
}

impl UserTLSDescriptor<'_> {
    pub fn new(raw_tls: PtrT) -> KResult<Self> {
        Ok(Self {
            ptr: UserPointerMut::new(UserMut::<UserDescriptor>::with_addr(raw_tls.addr()))?,
        })
    }

    pub fn read(&self) -> KResult<UserTLS> {
        let mut desc = self.ptr.read()?;

        let base = VAddr::from(desc.base as usize);

        // Clear the TLS area if it is not present.
        if desc.flags.is_read_exec_only() && !desc.flags.is_present() {
            if desc.limit != 0 && base != VAddr::NULL {
                let len = if desc.flags.is_limit_in_pages() {
                    (desc.limit as usize) << 12
                } else {
                    desc.limit as usize
                };

                CheckedUserPointer::new(User::new(base), len)?.zero()?;
            }
        }

        desc.entry = GDT::TLS32_INDEX as u32;
        self.ptr.write(desc)?;

        Ok(if desc.flags.is_limit_in_pages() {
            UserTLS::new_page_limit(desc.base, desc.limit)
        } else {
            UserTLS::new(desc.base, desc.limit)
        })
    }
}

impl fmt::Debug for UserTLSDescriptor<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("UserTLSDescriptor").finish_non_exhaustive()
    }
}
