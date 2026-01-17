use alloc::collections::btree_map::BTreeMap;
use alloc::sync::Arc;

use super::TmpFs;
use crate::io::{Buffer, Stream};
use crate::kernel::mem::{CachePage, PageCache, PageOffset};
use crate::kernel::timer::Instant;
use crate::kernel::vfs::inode::{Ino, InodeInfo, InodeOps, InodeUse, WriteOffset};
use crate::kernel::vfs::types::{DeviceId, Format, Mode, Permission};
use crate::kernel::vfs::{SbRef, SbUse};
use crate::prelude::KResult;

pub struct FileInode;

impl FileInode {
    pub fn new(ino: Ino, sb: SbRef<TmpFs>, size: usize, perm: Permission) -> InodeUse {
        let now = Instant::now();

        InodeUse::new(
            sb,
            ino,
            Format::REG,
            InodeInfo {
                size: size as _,
                nlink: 1,
                uid: 0,
                gid: 0,
                perm,
                atime: now,
                ctime: now,
                mtime: now,
            },
            Self,
        )
    }
}

impl InodeOps for FileInode {
    type SuperBlock = TmpFs;

    async fn read(
        &self,
        _: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        buffer: &mut dyn Buffer,
        offset: usize,
    ) -> KResult<usize> {
        let _lock = inode.rwsem.read().await;
        inode.get_page_cache().read(buffer, offset).await
    }

    async fn write(
        &self,
        _: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        stream: &mut dyn Stream,
        offset: WriteOffset<'_>,
    ) -> KResult<usize> {
        let _lock = inode.rwsem.write().await;

        let mut store_new_end = None;
        let offset = match offset {
            WriteOffset::Position(offset) => offset,
            WriteOffset::End(end) => {
                store_new_end = Some(end);

                // `info.size` won't change since we are holding the write lock.
                inode.info.lock().size as usize
            }
        };

        let page_cache = inode.get_page_cache();

        if Arc::strong_count(&page_cache) == 1 {
            // XXX: A temporary workaround here. Change this ASAP...
            // Prevent the page cache from being dropped during the write.
            let _ = Arc::into_raw(page_cache.clone());
        }

        let wrote = page_cache.write(stream, offset).await?;
        let cursor_end = offset + wrote;

        if let Some(store_end) = store_new_end {
            *store_end = cursor_end;
        }

        Ok(wrote)
    }

    async fn truncate(
        &self,
        _: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        length: usize,
    ) -> KResult<()> {
        let _lock = inode.rwsem.write().await;

        let now = Instant::now();
        let mut info = inode.info.lock();
        info.mtime = now;
        info.ctime = now;
        info.size = length as u64;

        Ok(())
    }

    async fn chmod(
        &self,
        _sb: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        perm: Permission,
    ) -> KResult<()> {
        let mut info = inode.info.lock();

        info.perm = perm;
        info.ctime = Instant::now();

        Ok(())
    }

    async fn read_page(
        &self,
        _: SbUse<Self::SuperBlock>,
        _: &InodeUse,
        page: &mut CachePage,
        _: PageOffset,
    ) -> KResult<()> {
        page.lock().as_bytes_mut().fill(0);
        Ok(())
    }

    async fn write_page(
        &self,
        _: SbUse<Self::SuperBlock>,
        _: &InodeUse,
        _: &mut CachePage,
        _: PageOffset,
    ) -> KResult<()> {
        // XXX: actually we should refuse to do the writeback.
        //      think of a way to inform that of the page cache.
        Ok(())
    }

    async fn write_begin<'a>(
        &self,
        _: SbUse<Self::SuperBlock>,
        _: &InodeUse,
        page_cache: &PageCache,
        pages: &'a mut BTreeMap<PageOffset, CachePage>,
        offset: usize,
        _: usize,
    ) -> KResult<&'a mut CachePage> {
        // TODO: Remove dependency on `page_cache`.
        page_cache
            .get_page_locked(pages, PageOffset::from_byte_floor(offset))
            .await
    }

    async fn write_end(
        &self,
        _: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        _: &PageCache,
        _: &mut BTreeMap<PageOffset, CachePage>,
        offset: usize,
        _: usize,
        copied: usize,
    ) -> KResult<()> {
        let now = Instant::now();
        let mut info = inode.info.lock();
        info.mtime = now;
        info.ctime = now;
        info.size = info.size.max((offset + copied) as u64);

        Ok(())
    }
}

pub struct DeviceInode {
    devid: DeviceId,
}

impl DeviceInode {
    pub fn new(ino: Ino, sb: SbRef<TmpFs>, mode: Mode, devid: DeviceId) -> InodeUse {
        let now = Instant::now();

        InodeUse::new(
            sb,
            ino,
            mode.format(),
            InodeInfo {
                size: 0,
                nlink: 1,
                uid: 0,
                gid: 0,
                perm: Permission::new(mode.non_format_bits()),
                atime: now,
                ctime: now,
                mtime: now,
            },
            Self { devid },
        )
    }
}

impl InodeOps for DeviceInode {
    type SuperBlock = TmpFs;

    async fn chmod(
        &self,
        _sb: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        perm: Permission,
    ) -> KResult<()> {
        let mut info = inode.info.lock();
        info.perm = perm;
        info.ctime = Instant::now();

        Ok(())
    }

    fn devid(&self, _: SbUse<Self::SuperBlock>, _: &InodeUse) -> KResult<DeviceId> {
        Ok(self.devid)
    }
}

pub struct SymlinkInode {
    target: Arc<[u8]>,
}

impl SymlinkInode {
    pub fn new(ino: Ino, sb: SbRef<TmpFs>, target: Arc<[u8]>) -> InodeUse {
        let now = Instant::now();

        InodeUse::new(
            sb,
            ino,
            Format::LNK,
            InodeInfo {
                size: target.len() as _,
                nlink: 1,
                uid: 0,
                gid: 0,
                perm: Permission::new(0o777),
                atime: now,
                ctime: now,
                mtime: now,
            },
            Self { target },
        )
    }
}

impl InodeOps for SymlinkInode {
    type SuperBlock = TmpFs;

    async fn readlink(
        &self,
        _sb: SbUse<Self::SuperBlock>,
        _inode: &InodeUse,
        buffer: &mut dyn Buffer,
    ) -> KResult<usize> {
        buffer
            .fill(self.target.as_ref())
            .map(|result| result.allow_partial())
    }
}
