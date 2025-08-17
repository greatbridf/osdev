use super::{dentry::Dentry, vfs::Vfs, DevId};
use crate::io::Stream;
use crate::kernel::constants::{
    EINVAL, EISDIR, ENOTDIR, EPERM, STATX_ATIME, STATX_BLOCKS, STATX_CTIME, STATX_GID, STATX_INO,
    STATX_MODE, STATX_MTIME, STATX_NLINK, STATX_SIZE, STATX_TYPE, STATX_UID, S_IFBLK, S_IFCHR,
    S_IFDIR, S_IFLNK, S_IFMT, S_IFREG,
};
use crate::kernel::mem::PageCache;
use crate::kernel::syscall::{FromSyscallArg, SyscallRetVal};
use crate::kernel::task::block_on;
use crate::kernel::timer::Instant;
use crate::{io::Buffer, prelude::*};
use alloc::sync::{Arc, Weak};
use core::{
    mem::MaybeUninit,
    ops::ControlFlow,
    ptr::addr_of_mut,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};
use eonix_sync::RwLock;
use posix_types::stat::StatX;

pub type Ino = u64;
pub type AtomicIno = AtomicU64;
#[allow(dead_code)]
pub type ISize = u64;
pub type AtomicISize = AtomicU64;
#[allow(dead_code)]
pub type Nlink = u64;
pub type AtomicNlink = AtomicU64;
#[allow(dead_code)]
pub type Uid = u32;
pub type AtomicUid = AtomicU32;
#[allow(dead_code)]
pub type Gid = u32;
pub type AtomicGid = AtomicU32;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Mode(u32);

pub struct AtomicMode(AtomicU32);

#[derive(Debug)]
pub struct InodeData {
    pub ino: Ino,
    pub size: AtomicISize,
    pub nlink: AtomicNlink,

    pub uid: AtomicUid,
    pub gid: AtomicGid,
    pub mode: AtomicMode,

    pub atime: Spin<Instant>,
    pub ctime: Spin<Instant>,
    pub mtime: Spin<Instant>,

    pub rwsem: RwLock<()>,

    pub vfs: Weak<dyn Vfs>,
}

impl InodeData {
    pub fn new(ino: Ino, vfs: Weak<dyn Vfs>) -> Self {
        Self {
            ino,
            vfs,
            atime: Spin::new(Instant::now()),
            ctime: Spin::new(Instant::now()),
            mtime: Spin::new(Instant::now()),
            rwsem: RwLock::new(()),
            size: AtomicU64::new(0),
            nlink: AtomicNlink::new(0),
            uid: AtomicUid::new(0),
            gid: AtomicGid::new(0),
            mode: AtomicMode::new(0),
        }
    }
}

#[allow(dead_code)]
pub trait InodeInner:
    Send + Sync + core::ops::Deref<Target = InodeData> + core::ops::DerefMut
{
    fn data(&self) -> &InodeData;
    fn data_mut(&mut self) -> &mut InodeData;
}

pub enum WriteOffset<'end> {
    Position(usize),
    End(&'end mut usize),
}

pub struct RenameData<'a, 'b> {
    pub old_dentry: &'a Arc<Dentry>,
    pub new_dentry: &'b Arc<Dentry>,
    pub new_parent: Arc<dyn Inode>,
    pub vfs: Arc<dyn Vfs>,
    pub is_exchange: bool,
    pub no_replace: bool,
}

#[allow(unused_variables)]
pub trait Inode: Send + Sync + InodeInner + Any {
    fn file_size(&self) -> usize {
        self.size.load(Ordering::Relaxed) as usize
    }

    fn is_dir(&self) -> bool {
        self.mode.load().is_dir()
    }

    fn lookup(&self, dentry: &Arc<Dentry>) -> KResult<Option<Arc<dyn Inode>>> {
        Err(if !self.is_dir() { ENOTDIR } else { EPERM })
    }

    fn creat(&self, at: &Arc<Dentry>, mode: Mode) -> KResult<()> {
        Err(if !self.is_dir() { ENOTDIR } else { EPERM })
    }

    fn mkdir(&self, at: &Dentry, mode: Mode) -> KResult<()> {
        Err(if !self.is_dir() { ENOTDIR } else { EPERM })
    }

    fn mknod(&self, at: &Dentry, mode: Mode, dev: DevId) -> KResult<()> {
        Err(if !self.is_dir() { ENOTDIR } else { EPERM })
    }

    fn unlink(&self, at: &Arc<Dentry>) -> KResult<()> {
        Err(if !self.is_dir() { ENOTDIR } else { EPERM })
    }

    fn symlink(&self, at: &Arc<Dentry>, target: &[u8]) -> KResult<()> {
        Err(if !self.is_dir() { ENOTDIR } else { EPERM })
    }

    fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        Err(if self.is_dir() { EISDIR } else { EINVAL })
    }

    fn read_direct(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        Err(if self.is_dir() { EISDIR } else { EINVAL })
    }

    fn write(&self, stream: &mut dyn Stream, offset: WriteOffset) -> KResult<usize> {
        Err(if self.is_dir() { EISDIR } else { EINVAL })
    }

    fn write_direct(&self, stream: &mut dyn Stream, offset: usize) -> KResult<usize> {
        Err(if self.is_dir() { EISDIR } else { EINVAL })
    }

    fn devid(&self) -> KResult<DevId> {
        Err(if self.is_dir() { EISDIR } else { EINVAL })
    }

    fn readlink(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        Err(if self.is_dir() { EISDIR } else { EINVAL })
    }

    fn truncate(&self, length: usize) -> KResult<()> {
        Err(if self.is_dir() { EISDIR } else { EPERM })
    }

    fn rename(&self, rename_data: RenameData) -> KResult<()> {
        Err(if !self.is_dir() { ENOTDIR } else { EPERM })
    }

    fn do_readdir(
        &self,
        offset: usize,
        callback: &mut dyn FnMut(&[u8], Ino) -> KResult<ControlFlow<(), ()>>,
    ) -> KResult<usize> {
        Err(if !self.is_dir() { ENOTDIR } else { EPERM })
    }

    fn chmod(&self, mode: Mode) -> KResult<()> {
        Err(EPERM)
    }

    fn chown(&self, uid: u32, gid: u32) -> KResult<()> {
        Err(EPERM)
    }

    fn page_cache(&self) -> Option<&PageCache> {
        None
    }

    fn statx(&self, stat: &mut StatX, mask: u32) -> KResult<()> {
        // Safety: ffi should have checked reference
        let vfs = self.vfs.upgrade().expect("Vfs is dropped");

        let size = self.size.load(Ordering::Relaxed);
        let mode = self.mode.load();

        if mask & STATX_NLINK != 0 {
            stat.stx_nlink = self.nlink.load(Ordering::Acquire) as _;
            stat.stx_mask |= STATX_NLINK;
        }

        if mask & STATX_ATIME != 0 {
            let atime = *self.atime.lock();
            stat.stx_atime = atime.into();
            stat.stx_mask |= STATX_ATIME;
        }

        if mask & STATX_MTIME != 0 {
            let mtime = *self.mtime.lock();
            stat.stx_mtime = mtime.into();
            stat.stx_mask |= STATX_MTIME;
        }

        if mask & STATX_CTIME != 0 {
            let ctime = *self.ctime.lock();
            stat.stx_ctime = ctime.into();
            stat.stx_mask |= STATX_CTIME;
        }

        if mask & STATX_SIZE != 0 {
            stat.stx_size = self.size.load(Ordering::Relaxed) as _;
            stat.stx_mask |= STATX_SIZE;
        }

        stat.stx_mode = 0;
        if mask & STATX_MODE != 0 {
            stat.stx_mode |= mode.non_format_bits() as u16;
            stat.stx_mask |= STATX_MODE;
        }

        if mask & STATX_TYPE != 0 {
            stat.stx_mode |= mode.format_bits() as u16;
            if mode.is_blk() || mode.is_chr() {
                let devid = self.devid();
                stat.stx_rdev_major = (devid? >> 8) & 0xff;
                stat.stx_rdev_minor = devid? & 0xff;
            }
            stat.stx_mask |= STATX_TYPE;
        }

        if mask & STATX_INO != 0 {
            stat.stx_ino = self.ino as _;
            stat.stx_mask |= STATX_INO;
        }

        if mask & STATX_BLOCKS != 0 {
            stat.stx_blocks = (size + 512 - 1) / 512;
            stat.stx_blksize = vfs.io_blksize() as _;
            stat.stx_mask |= STATX_BLOCKS;
        }

        if mask & STATX_UID != 0 {
            stat.stx_uid = self.uid.load(Ordering::Relaxed) as _;
            stat.stx_mask |= STATX_UID;
        }

        if mask & STATX_GID != 0 {
            stat.stx_gid = self.gid.load(Ordering::Relaxed) as _;
            stat.stx_mask |= STATX_GID;
        }

        let fsdev = vfs.fs_devid();
        stat.stx_dev_major = (fsdev >> 8) & 0xff;
        stat.stx_dev_minor = fsdev & 0xff;

        // TODO: support more attributes
        stat.stx_attributes_mask = 0;

        Ok(())
    }

    fn new_locked<F>(ino: Ino, vfs: Weak<dyn Vfs>, f: F) -> Arc<Self>
    where
        Self: Sized,
        F: FnOnce(*mut Self, &()),
    {
        let mut uninit = Arc::<Self>::new_uninit();

        let uninit_mut = Arc::get_mut(&mut uninit).unwrap();

        // Safety: `idata` is owned by `uninit`
        let idata = unsafe {
            addr_of_mut!(*(*uninit_mut.as_mut_ptr()).data_mut())
                .cast::<MaybeUninit<InodeData>>()
                .as_mut()
                .unwrap()
        };

        idata.write(InodeData::new(ino, vfs));

        f(
            uninit_mut.as_mut_ptr(),
            // SAFETY: `idata` is initialized and we will never move the lock.
            &block_on(unsafe { idata.assume_init_ref() }.rwsem.read()),
        );

        // Safety: `uninit` is initialized
        unsafe { uninit.assume_init() }
    }
}

// TODO: define multiple inode structs a time
macro_rules! define_struct_inode {
    ($v:vis struct $inode_t:ident;) => {
        $v struct $inode_t {
            /// Do not use this directly
            idata: $crate::kernel::vfs::inode::InodeData,
        }

        impl core::ops::Deref for $inode_t {
            type Target = $crate::kernel::vfs::inode::InodeData;

            fn deref(&self) -> &Self::Target {
                &self.idata
            }
        }

        impl core::ops::DerefMut for $inode_t {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.idata
            }
        }

        impl $crate::kernel::vfs::inode::InodeInner for $inode_t {
            fn data(&self) -> &$crate::kernel::vfs::inode::InodeData {
                &self.idata
            }

            fn data_mut(&mut self) -> &mut $crate::kernel::vfs::inode::InodeData {
                &mut self.idata
            }
        }
    };
    ($v:vis struct $inode_t:ident { $($vis:vis $name:ident: $type:ty,)* }) => {
        $v struct $inode_t {
            /// Do not use this directly
            idata: $crate::kernel::vfs::inode::InodeData,
            $($vis $name: $type,)*
        }

        impl core::ops::Deref for $inode_t {
            type Target = $crate::kernel::vfs::inode::InodeData;

            fn deref(&self) -> &Self::Target {
                &self.idata
            }
        }

        impl core::ops::DerefMut for $inode_t {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.idata
            }
        }

        impl $crate::kernel::vfs::inode::InodeInner for $inode_t {
            fn data(&self) -> &$crate::kernel::vfs::inode::InodeData {
                &self.idata
            }

            fn data_mut(&mut self) -> &mut $crate::kernel::vfs::inode::InodeData {
                &mut self.idata
            }
        }
    };
}

pub(crate) use define_struct_inode;

impl Mode {
    pub const REG: Self = Self(S_IFREG);
    pub const DIR: Self = Self(S_IFDIR);
    pub const LNK: Self = Self(S_IFLNK);
    pub const BLK: Self = Self(S_IFBLK);
    pub const CHR: Self = Self(S_IFCHR);

    pub const fn new(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn is_blk(&self) -> bool {
        (self.0 & S_IFMT) == S_IFBLK
    }

    pub const fn is_chr(&self) -> bool {
        (self.0 & S_IFMT) == S_IFCHR
    }

    pub const fn is_reg(&self) -> bool {
        (self.0 & S_IFMT) == S_IFREG
    }

    pub const fn is_dir(&self) -> bool {
        (self.0 & S_IFMT) == S_IFDIR
    }

    pub const fn is_lnk(&self) -> bool {
        (self.0 & S_IFMT) == S_IFLNK
    }

    pub const fn bits(&self) -> u32 {
        self.0
    }

    pub const fn format_bits(&self) -> u32 {
        self.0 & S_IFMT
    }

    pub const fn format(&self) -> Self {
        Self::new(self.format_bits())
    }

    pub const fn non_format_bits(&self) -> u32 {
        self.0 & !S_IFMT
    }

    pub const fn non_format(&self) -> Self {
        Self::new(self.non_format_bits())
    }

    pub const fn perm(self, perm: u32) -> Self {
        Self::new((self.0 & !0o777) | (perm & 0o777))
    }

    pub const fn set_perm(&mut self, perm: u32) {
        *self = self.perm(perm);
    }

    pub const fn mask_perm(&mut self, perm_mask: u32) {
        let perm_mask = perm_mask & 0o777;
        let self_perm = self.non_format_bits() & 0o777;

        *self = self.perm(self_perm & perm_mask);
    }
}

impl AtomicMode {
    pub const fn new(bits: u32) -> Self {
        Self(AtomicU32::new(bits))
    }

    pub const fn from(mode: Mode) -> Self {
        Self::new(mode.0)
    }

    pub fn load(&self) -> Mode {
        Mode(self.0.load(Ordering::Relaxed))
    }

    pub fn store(&self, mode: Mode) {
        self.0.store(mode.0, Ordering::Relaxed);
    }
}

impl core::fmt::Debug for AtomicMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AtomicMode")
            .field("bits", &self.load().0)
            .finish()
    }
}

impl core::fmt::Debug for Mode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let format_name = match self.format() {
            Mode::REG => "REG",
            Mode::DIR => "DIR",
            Mode::LNK => "LNK",
            Mode::BLK => "BLK",
            Mode::CHR => "CHR",
            _ => "UNK",
        };

        match self.non_format_bits() & !0o777 {
            0 => write!(
                f,
                "Mode({format_name}, {perm:#o})",
                perm = self.non_format_bits()
            )?,
            rem => write!(
                f,
                "Mode({format_name}, {perm:#o}, rem={rem:#x})",
                perm = self.non_format_bits() & 0o777
            )?,
        }

        Ok(())
    }
}

impl FromSyscallArg for Mode {
    fn from_arg(value: usize) -> Self {
        Mode::new(value as u32)
    }
}

impl SyscallRetVal for Mode {
    fn into_retval(self) -> Option<usize> {
        Some(self.bits() as usize)
    }
}
