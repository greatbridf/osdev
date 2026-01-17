use alloc::collections::btree_map::{BTreeMap, Entry};
use core::future::Future;
use core::ops::{Deref, DerefMut};

use eonix_mm::paging::{Folio as _, PAGE_SIZE, PAGE_SIZE_BITS, PFN};
use eonix_sync::Mutex;

use super::page_alloc::PageFlags;
use super::{Folio, FolioOwned};
use crate::io::{Buffer, Stream};
use crate::kernel::constants::EINVAL;
use crate::kernel::vfs::inode::InodeUse;
use crate::prelude::KResult;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageOffset(usize);

pub struct PageCache {
    pages: Mutex<BTreeMap<PageOffset, CachePage>>,
    inode: InodeUse,
}

pub struct CachePage(Folio);

impl PageOffset {
    pub const fn from_byte_floor(offset: usize) -> Self {
        Self(offset >> PAGE_SIZE_BITS)
    }

    pub const fn from_byte_ceil(offset: usize) -> Self {
        Self((offset + PAGE_SIZE - 1) >> PAGE_SIZE_BITS)
    }

    pub fn iter_till(self, end: PageOffset) -> impl Iterator<Item = PageOffset> {
        (self.0..end.0).map(PageOffset)
    }

    pub fn page_count(self) -> usize {
        self.0
    }

    pub fn byte_count(self) -> usize {
        self.page_count() * PAGE_SIZE
    }
}

impl CachePage {
    pub fn new() -> Self {
        CachePage(Folio::alloc())
    }

    pub fn new_zeroed() -> Self {
        CachePage({
            let mut folio = FolioOwned::alloc();
            folio.as_bytes_mut().fill(0);

            folio.share()
        })
    }

    pub fn is_dirty(&self) -> bool {
        self.flags.has(PageFlags::DIRTY)
    }

    pub fn set_dirty(&self, dirty: bool) {
        if dirty {
            self.flags.set(PageFlags::DIRTY);
        } else {
            self.flags.clear(PageFlags::DIRTY);
        }
    }

    pub fn add_mapping(&self) -> PFN {
        // TODO: Increase map_count
        self.0.clone().into_raw()
    }
}

impl Deref for CachePage {
    type Target = Folio;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CachePage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PageCache {
    pub fn new(inode: InodeUse) -> Self {
        Self {
            pages: Mutex::new(BTreeMap::new()),
            inode,
        }
    }

    pub fn get_page_locked<'a>(
        &self,
        pages: &'a mut BTreeMap<PageOffset, CachePage>,
        pgoff: PageOffset,
    ) -> impl Future<Output = KResult<&'a mut CachePage>> + Send + use<'_, 'a> {
        async move {
            match pages.entry(pgoff) {
                Entry::Occupied(ent) => Ok(ent.into_mut()),
                Entry::Vacant(vacant_entry) => {
                    let mut new_page = CachePage::new();
                    self.inode.read_page(&mut new_page, pgoff).await?;

                    Ok(vacant_entry.insert(new_page))
                }
            }
        }
    }

    fn len(&self) -> usize {
        self.inode.info.lock().size as usize
    }

    // TODO: Remove this.
    pub async fn with_page(&self, pgoff: PageOffset, func: impl FnOnce(&CachePage)) -> KResult<()> {
        let mut pages = self.pages.lock().await;
        if pgoff > PageOffset::from_byte_ceil(self.len()) {
            return Err(EINVAL);
        }

        let cache_page = self.get_page_locked(&mut pages, pgoff).await?;

        func(cache_page);

        Ok(())
    }

    pub async fn read(&self, buffer: &mut dyn Buffer, mut offset: usize) -> KResult<usize> {
        let mut pages = self.pages.lock().await;
        let total_len = self.len();

        if offset >= total_len {
            return Ok(0);
        }

        let pgoff_start = PageOffset::from_byte_floor(offset);
        let pgoff_end = PageOffset::from_byte_ceil(total_len);

        for pgoff in pgoff_start.iter_till(pgoff_end) {
            let page = self.get_page_locked(&mut pages, pgoff).await?;

            let end_offset = (offset + PAGE_SIZE) / PAGE_SIZE * PAGE_SIZE;
            let real_end = end_offset.min(total_len);

            let inner_offset = offset % PAGE_SIZE;
            let data_len = real_end - offset;

            if buffer
                .fill(&page.lock().as_bytes()[inner_offset..inner_offset + data_len])?
                .should_stop()
                || buffer.available() == 0
            {
                break;
            }

            offset = real_end;
        }

        Ok(buffer.wrote())
    }

    pub async fn write(&self, stream: &mut dyn Stream, mut offset: usize) -> KResult<usize> {
        let mut pages = self.pages.lock().await;
        let mut total_written = 0;

        loop {
            let end_offset = (offset + PAGE_SIZE) / PAGE_SIZE * PAGE_SIZE;
            let len = end_offset - offset;

            // TODO: Rewrite to return a write state object.
            let page = self
                .inode
                .write_begin(self, &mut pages, offset, len)
                .await?;

            let inner_offset = offset % PAGE_SIZE;
            let written = stream
                .poll_data(&mut page.lock().as_bytes_mut()[inner_offset..])?
                .map(|b| b.len())
                .unwrap_or(0);

            page.set_dirty(true);
            self.inode
                .write_end(self, &mut pages, offset, len, written)
                .await?;

            if written == 0 {
                break;
            }

            total_written += written;
            offset += written;
        }

        Ok(total_written)
    }

    pub async fn fsync(&self) -> KResult<()> {
        let mut pages = self.pages.lock().await;

        for (&pgoff, page) in pages.iter_mut() {
            if !page.is_dirty() {
                continue;
            }

            self.inode.write_page(page, pgoff).await?;
            page.set_dirty(false);
        }

        Ok(())
    }
}

impl core::fmt::Debug for PageCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PageCache").finish()
    }
}

impl Drop for PageCache {
    fn drop(&mut self) {
        // XXX: Send the PageCache to some flusher worker.
        let _ = self.fsync();
    }
}
