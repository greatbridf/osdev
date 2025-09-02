use futures::Stream;

use crate::{kernel::mem::Page, prelude::KResult};

use super::{ClusterIterator, FatFs};

pub trait ReadClusters {
    fn read_clusters(self, fs: &FatFs) -> impl Stream<Item = KResult<Page>> + Send;
}

impl ReadClusters for ClusterIterator<'_> {
    fn read_clusters(self, fs: &FatFs) -> impl Stream<Item = KResult<Page>> + Send {
        futures::stream::unfold(self, move |mut me| async {
            let cluster = me.next()?;
            let page = Page::alloc();

            if let Err(err) = fs.read_cluster(cluster, &page).await {
                return Some((Err(err), me));
            }

            Some((Ok(page), me))
        })
    }
}
