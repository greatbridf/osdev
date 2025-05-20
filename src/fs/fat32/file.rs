use super::{ClusterIterator, FatFs};
use crate::{
    kernel::mem::{AsMemoryBlock as _, Page},
    KResult,
};

pub trait ClusterReadIterator<'data>: Iterator<Item = KResult<&'data [u8]>> + 'data {}
impl<'a, I> ClusterReadIterator<'a> for I where I: Iterator<Item = KResult<&'a [u8]>> + 'a {}

pub(super) trait ClusterRead<'data> {
    fn read<'vfs>(self, vfs: &'vfs FatFs, offset: usize) -> impl ClusterReadIterator<'data>
    where
        Self: Sized,
        'vfs: 'data;
}

impl<'data, 'fat: 'data> ClusterRead<'data> for ClusterIterator<'fat> {
    fn read<'vfs: 'data>(self, vfs: &'vfs FatFs, offset: usize) -> impl ClusterReadIterator<'data> {
        const SECTOR_SIZE: usize = 512;

        let cluster_size = vfs.sectors_per_cluster as usize * SECTOR_SIZE;
        assert!(cluster_size <= 0x1000, "Cluster size is too large");

        let skip_clusters = offset / cluster_size;
        let mut inner_offset = offset % cluster_size;

        // TODO: Use block cache.
        let buffer_page = Page::alloc();

        self.skip(skip_clusters).map(move |cluster| {
            vfs.read_cluster(cluster, &buffer_page)?;
            let data = unsafe {
                // SAFETY: No one could be writing to it.
                &buffer_page.as_memblk().as_bytes()[inner_offset..]
            };
            inner_offset = 0;
            Ok(data)
        })
    }
}
