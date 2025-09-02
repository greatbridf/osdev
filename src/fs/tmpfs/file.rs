use alloc::sync::Arc;
use eonix_mm::paging::PAGE_SIZE;
use eonix_sync::{RwLock, Spin};

use crate::{
    io::{Buffer, Stream},
    kernel::{
        mem::{CachePage, CachePageStream, PageCache, PageCacheBackendOps},
        timer::Instant,
        vfs::{
            inode::{Ino, InodeDirOps, InodeFileOps, InodeInfo, InodeOps, InodeUse, WriteOffset},
            types::{DeviceId, Format, Mode, Permission},
            SbRef,
        },
    },
    prelude::KResult,
};

use super::TmpFs;

pub struct FileInode {
    sb: SbRef<TmpFs>,
    ino: Ino,
    info: Spin<InodeInfo>,
    rwsem: RwLock<()>,
    pages: PageCache,
}

impl FileInode {
    pub fn new(ino: Ino, sb: SbRef<TmpFs>, size: usize, perm: Permission) -> InodeUse<Self> {
        let now = Instant::now();

        InodeUse::new_cyclic(|weak| Self {
            sb,
            ino,
            info: Spin::new(InodeInfo {
                size: size as _,
                nlink: 1,
                uid: 0,
                gid: 0,
                perm,
                atime: now,
                ctime: now,
                mtime: now,
            }),
            rwsem: RwLock::new(()),
            pages: PageCache::new(weak.clone() as _),
        })
    }
}

impl PageCacheBackendOps for FileInode {
    async fn read_page(&self, _cache_page: &mut CachePage, _offset: usize) -> KResult<usize> {
        Ok(PAGE_SIZE)
    }

    async fn write_page(&self, _page: &mut CachePageStream, _offset: usize) -> KResult<usize> {
        Ok(PAGE_SIZE)
    }

    fn size(&self) -> usize {
        self.info.lock().size as usize
    }
}

impl InodeOps for FileInode {
    type SuperBlock = TmpFs;

    fn ino(&self) -> Ino {
        self.ino
    }

    fn format(&self) -> Format {
        Format::REG
    }

    fn info(&self) -> &Spin<InodeInfo> {
        &self.info
    }

    fn super_block(&self) -> &SbRef<Self::SuperBlock> {
        &self.sb
    }

    fn page_cache(&self) -> Option<&PageCache> {
        Some(&self.pages)
    }
}

impl InodeDirOps for FileInode {}
impl InodeFileOps for FileInode {
    async fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        let _lock = self.rwsem.read().await;
        self.pages.read(buffer, offset).await
    }

    async fn write(&self, stream: &mut dyn Stream, offset: WriteOffset<'_>) -> KResult<usize> {
        let _lock = self.rwsem.write().await;

        let mut store_new_end = None;
        let offset = match offset {
            WriteOffset::Position(offset) => offset,
            WriteOffset::End(end) => {
                store_new_end = Some(end);

                // `info.size` won't change since we are holding the write lock.
                self.info.lock().size as usize
            }
        };

        let wrote = self.pages.write(stream, offset).await?;
        let cursor_end = offset + wrote;

        if let Some(store_end) = store_new_end {
            *store_end = cursor_end;
        }

        {
            let now = Instant::now();
            let mut info = self.info.lock();
            info.mtime = now;
            info.ctime = now;
            info.size = info.size.max(cursor_end as u64);
        }

        Ok(wrote)
    }

    async fn truncate(&self, length: usize) -> KResult<()> {
        let _lock = self.rwsem.write().await;

        self.pages.resize(length).await?;

        {
            let now = Instant::now();
            let mut info = self.info.lock();
            info.mtime = now;
            info.ctime = now;
            info.size = length as u64;
        }

        Ok(())
    }

    async fn chmod(&self, perm: Permission) -> KResult<()> {
        let _sb = self.sb.get()?;

        {
            let mut info = self.info.lock();

            info.perm = perm;
            info.ctime = Instant::now();
        }

        Ok(())
    }
}

pub struct DeviceInode {
    sb: SbRef<TmpFs>,
    ino: Ino,
    info: Spin<InodeInfo>,
    is_block: bool,
    devid: DeviceId,
}

impl DeviceInode {
    pub fn new(ino: Ino, sb: SbRef<TmpFs>, mode: Mode, devid: DeviceId) -> InodeUse<Self> {
        let now = Instant::now();

        InodeUse::new(Self {
            sb,
            ino,
            info: Spin::new(InodeInfo {
                size: 0,
                nlink: 1,
                uid: 0,
                gid: 0,
                perm: Permission::new(mode.non_format_bits()),
                atime: now,
                ctime: now,
                mtime: now,
            }),
            is_block: mode.format() == Format::BLK,
            devid,
        })
    }
}

impl InodeOps for DeviceInode {
    type SuperBlock = TmpFs;

    fn ino(&self) -> Ino {
        self.ino
    }

    fn format(&self) -> Format {
        if self.is_block {
            Format::BLK
        } else {
            Format::CHR
        }
    }

    fn info(&self) -> &Spin<InodeInfo> {
        &self.info
    }

    fn super_block(&self) -> &SbRef<Self::SuperBlock> {
        &self.sb
    }

    fn page_cache(&self) -> Option<&PageCache> {
        None
    }
}

impl InodeDirOps for DeviceInode {}
impl InodeFileOps for DeviceInode {
    async fn chmod(&self, perm: Permission) -> KResult<()> {
        let _sb = self.sb.get()?;

        {
            let mut info = self.info.lock();

            info.perm = perm;
            info.ctime = Instant::now();
        }

        Ok(())
    }

    fn devid(&self) -> KResult<DeviceId> {
        Ok(self.devid)
    }
}

pub struct SymlinkInode {
    sb: SbRef<TmpFs>,
    ino: Ino,
    info: Spin<InodeInfo>,
    target: Arc<[u8]>,
}

impl SymlinkInode {
    pub fn new(ino: Ino, sb: SbRef<TmpFs>, target: Arc<[u8]>) -> InodeUse<Self> {
        let now = Instant::now();

        InodeUse::new(Self {
            sb,
            ino,
            info: Spin::new(InodeInfo {
                size: target.len() as _,
                nlink: 1,
                uid: 0,
                gid: 0,
                perm: Permission::new(0o777),
                atime: now,
                ctime: now,
                mtime: now,
            }),
            target,
        })
    }
}

impl InodeDirOps for SymlinkInode {}
impl InodeOps for SymlinkInode {
    type SuperBlock = TmpFs;

    fn ino(&self) -> Ino {
        self.ino
    }

    fn format(&self) -> Format {
        Format::LNK
    }

    fn info(&self) -> &Spin<InodeInfo> {
        &self.info
    }

    fn super_block(&self) -> &SbRef<Self::SuperBlock> {
        &self.sb
    }

    fn page_cache(&self) -> Option<&PageCache> {
        None
    }
}

impl InodeFileOps for SymlinkInode {
    async fn readlink(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        buffer
            .fill(self.target.as_ref())
            .map(|result| result.allow_partial())
    }
}
