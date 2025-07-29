use super::access::AsMemoryBlock;
use crate::{
    io::{Buffer, FillResult, Stream},
    kernel::mem::page_alloc::RawPagePtr,
    prelude::KResult,
    GlobalPageAlloc,
};
use align_ext::AlignExt;
use alloc::{collections::btree_map::BTreeMap, sync::Weak};
use eonix_mm::paging::{PageAlloc, RawPage, PAGE_SIZE, PAGE_SIZE_BITS};
use eonix_sync::Mutex;

pub struct PageCache {
    pages: Mutex<BTreeMap<usize, CachePage>>,
    backend: Weak<dyn PageCacheBackend>,
}

unsafe impl Send for PageCache {}
unsafe impl Sync for PageCache {}

#[derive(Clone, Copy)]
pub struct CachePage(RawPagePtr);

impl Buffer for CachePage {
    fn total(&self) -> usize {
        PAGE_SIZE
    }

    fn wrote(&self) -> usize {
        self.valid_size()
    }

    fn fill(&mut self, data: &[u8]) -> KResult<FillResult> {
        let valid_size = self.valid_size();
        let available = &mut self.all_mut()[valid_size..];
        if available.len() == 0 {
            return Ok(FillResult::Full);
        }

        let len = core::cmp::min(data.len(), available.len());
        available[..len].copy_from_slice(&data[..len]);

        *self.0.valid_size() += len;

        if len < data.len() {
            Ok(FillResult::Partial(len))
        } else {
            Ok(FillResult::Done(len))
        }
    }
}

impl CachePage {
    pub fn new() -> Self {
        let page = GlobalPageAlloc.alloc().unwrap();
        page.cache_init();
        Self(page)
    }

    pub fn new_zeroed() -> Self {
        let page = GlobalPageAlloc.alloc().unwrap();
        // SAFETY: We own the page exclusively, so we can safely zero it.
        unsafe {
            page.as_memblk().as_bytes_mut().fill(0);
        }
        page.cache_init();
        Self(page)
    }

    pub fn valid_size(&self) -> usize {
        *self.0.valid_size()
    }

    pub fn set_valid_size(&mut self, valid_size: usize) {
        *self.0.valid_size() = valid_size;
    }

    pub fn all(&self) -> &[u8] {
        unsafe {
            self.0.as_memblk().as_bytes()
        }
    }

    pub fn all_mut(&mut self) -> &mut [u8] {
        unsafe {
            self.0.as_memblk().as_bytes_mut()
        }
    }

    pub fn valid_data(&self) -> &[u8] {
        &self.all()[..self.valid_size()]
    }

    pub fn is_dirty(&self) -> bool {
        self.0.is_dirty()
    }

    pub fn set_dirty(&self) {
        self.0.set_dirty();
    }

    pub fn clear_dirty(&self) {
        self.0.clear_dirty();
    }
}

impl PageCache {
    pub fn new(backend: Weak<dyn PageCacheBackend>) -> Self {
        Self {
            pages: Mutex::new(BTreeMap::new()),
            backend: backend,
        }
    }

    pub async fn read(&self, buffer: &mut dyn Buffer, mut offset: usize) -> KResult<usize> {
        let mut pages = self.pages.lock().await;

        loop {
            let page_id = offset >> PAGE_SIZE_BITS;
            let page = pages.get(&page_id);

            match page {
                Some(page) => {
                    let inner_offset = offset % PAGE_SIZE;

                    // TODO: still cause unnecessary IO if valid_size < PAGESIZE
                    //       and fill result is Done
                    if page.valid_size() == 0
                        || buffer
                            .fill(&page.valid_data()[inner_offset..])?
                            .should_stop()
                        || buffer.available() == 0
                    {
                        break;
                    }

                    offset += PAGE_SIZE - inner_offset;
                }
                None => {
                    let mut new_page = CachePage::new();
                    self.backend
                        .upgrade()
                        .unwrap()
                        .read_page(&mut new_page, offset.align_down(PAGE_SIZE))?;
                    pages.insert(page_id, new_page);
                }
            }
        }

        Ok(buffer.wrote())
    }

    pub async fn write(&self, stream: &mut dyn Stream, mut offset: usize) -> KResult<usize> {
        let mut pages = self.pages.lock().await;
        let old_size = self.backend.upgrade().unwrap().size();
        let mut wrote = 0;

        loop {
            let page_id = offset >> PAGE_SIZE_BITS;
            let page = pages.get_mut(&page_id);

            match page {
                Some(page) => {
                    let inner_offset = offset % PAGE_SIZE;
                    let cursor_end = match stream.poll_data(&mut page.all_mut()[inner_offset..])? {
                        Some(buf) => {
                            wrote += buf.len();
                            inner_offset + buf.len()
                        }
                        None => {
                            break;
                        }
                    };

                    if page.valid_size() < cursor_end {
                        page.set_valid_size(cursor_end);
                    }
                    page.set_dirty();
                    offset += PAGE_SIZE - inner_offset;
                }
                None => {
                    let new_page = if (offset >> PAGE_SIZE_BITS) > (old_size >> PAGE_SIZE_BITS) {
                        let new_page = CachePage::new_zeroed();
                        new_page
                    } else {
                        let mut new_page = CachePage::new();
                        self.backend
                            .upgrade()
                            .unwrap()
                            .read_page(&mut new_page, offset.align_down(PAGE_SIZE))?;
                        new_page
                    };

                    pages.insert(page_id, new_page);
                }
            }
        }

        Ok(wrote)
    }

    pub async fn fsync(&self) -> KResult<()> {
        let pages = self.pages.lock().await;
        for (page_id, page) in pages.iter() {
            if page.is_dirty() {
                self.backend
                    .upgrade()
                    .unwrap()
                    .write_page(page, page_id << PAGE_SIZE_BITS)?;
                page.clear_dirty();
            }
        }
        Ok(())
    }

    // This function is used for extend write or truncate
    pub async fn resize(&self, new_size: usize) -> KResult<()> {
        let mut pages = self.pages.lock().await;
        let old_size = self.backend.upgrade().unwrap().size();

        if new_size < old_size {
            let begin = new_size.align_down(PAGE_SIZE) >> PAGE_SIZE_BITS;
            let end = old_size.align_up(PAGE_SIZE) >> PAGE_SIZE_BITS;

            for page_id in begin..end {
                pages.remove(&page_id);
            }
        } else if new_size > old_size {
            let begin = old_size.align_down(PAGE_SIZE) >> PAGE_SIZE_BITS;
            let end = new_size.align_up(PAGE_SIZE) >> PAGE_SIZE_BITS;

            pages.remove(&begin);

            for page_id in begin..end {
                let mut new_page = CachePage::new_zeroed();

                if page_id != end - 1 {
                    new_page.set_valid_size(PAGE_SIZE);
                } else {
                    new_page.set_valid_size(new_size % PAGE_SIZE);
                }
                new_page.set_dirty();
                pages.insert(page_id, new_page);
            }
        }

        Ok(())
    }

    pub async fn get_page(&self, offset: usize) -> KResult<Option<RawPagePtr>> {
        let offset_aligin = offset.align_down(PAGE_SIZE);
        let page_id = offset_aligin >> PAGE_SIZE_BITS;
        let size = self.backend.upgrade().unwrap().size();

        if offset_aligin > size {
            return Ok(None);
        }

        let mut pages = self.pages.lock().await;

        if let Some(page) = pages.get(&page_id) {
            Ok(Some(page.0))
        } else {
            let mut new_page = CachePage::new();
            self.backend
                .upgrade()
                .unwrap()
                .read_page(&mut new_page, offset_aligin)?;
            pages.insert(page_id, new_page);
            Ok(Some(new_page.0))
        }
    }
}

// with this trait, "page cache" and "block cache" are unified,
// for fs, offset is file offset (floor algin to PAGE_SIZE)
// for blkdev, offset is block idx (floor align to PAGE_SIZE / BLK_SIZE)
// Oh no, this would make unnecessary cache
pub trait PageCacheBackend {
    fn read_page(&self, page: &mut CachePage, offset: usize) -> KResult<usize>;

    fn write_page(&self, page: &CachePage, offset: usize) -> KResult<usize>;

    fn size(&self) -> usize;
}

pub trait PageCacheRawPage: RawPage {
    fn valid_size(&self) -> &mut usize;

    fn is_dirty(&self) -> bool;

    fn set_dirty(&self);

    fn clear_dirty(&self);

    fn cache_init(&self);
}

impl Drop for PageCache {
    fn drop(&mut self) {
        let _ = self.fsync();
    }
}
