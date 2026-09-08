use alloc::collections::btree_map::{BTreeMap, Entry};
use core::future::Future;

use eonix_macros::TransparentDeref;
use eonix_mm::paging::{PAGE_SIZE, PFN};
use eonix_sync::{atomic, Mutex};

use super::page_alloc::PageFlags;
use crate::io::{Buffer, Stream};
use crate::kernel::constants::EINVAL;
use crate::kernel::mem::{FolioOwned, MapFolio, PageOffset};
use crate::kernel::vfs::inode::InodeUse;
use crate::prelude::KResult;

pub struct PageCache {
    pages: Mutex<BTreeMap<PageOffset, CachePage>>,
    inode: InodeUse,
}

#[repr(transparent)]
#[derive(TransparentDeref)]
pub struct CachePage(MapFolio);

impl CachePage {
    fn new(folio: FolioOwned) -> Self {
        CachePage(folio.into_mappable())
    }

    pub fn is_dirty(&self) -> bool {
        atomic!(self.flags, has, PageFlags::DIRTY)
    }

    pub fn set_dirty(&self, dirty: bool) {
        if dirty {
            atomic!(self.flags, set, PageFlags::DIRTY);
        } else {
            atomic!(self.flags, clear, PageFlags::DIRTY);
        }
    }

    pub fn add_mapping(&self) -> PFN {
        self.0.clone().add_mapping()
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
        &self, pages: &'a mut BTreeMap<PageOffset, CachePage>,
        pgoff: PageOffset,
    ) -> impl Future<Output = KResult<&'a mut CachePage>> + Send + use<'_, 'a>
    {
        async move {
            match pages.entry(pgoff) {
                Entry::Occupied(ent) => Ok(ent.into_mut()),
                Entry::Vacant(vacant_entry) => {
                    let mut new_page = FolioOwned::alloc();
                    self.inode.read_page(&mut new_page, pgoff).await?;

                    Ok(vacant_entry.insert(CachePage::new(new_page)))
                }
            }
        }
    }

    fn len(&self) -> usize {
        self.inode.info.lock().size as usize
    }

    // TODO: Remove this.
    pub async fn with_page(
        &self, pgoff: PageOffset, func: impl FnOnce(&CachePage),
    ) -> KResult<()> {
        let mut pages = self.pages.lock().await;
        if pgoff > PageOffset::from_byte_ceil(self.len()) {
            return Err(EINVAL);
        }

        let cache_page = self.get_page_locked(&mut pages, pgoff).await?;

        func(cache_page);

        Ok(())
    }

    pub async fn read(
        &self, buffer: &mut dyn Buffer, mut offset: usize,
    ) -> KResult<usize> {
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
                .fill(
                    &page.lock().as_bytes()
                        [inner_offset..inner_offset + data_len],
                )?
                .should_stop()
                || buffer.available() == 0
            {
                break;
            }

            offset = real_end;
        }

        Ok(buffer.wrote())
    }

    pub async fn write(
        &self, stream: &mut dyn Stream, mut offset: usize,
    ) -> KResult<usize> {
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
