use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
use alloc::sync::{Arc, Weak};
use core::any::Any;
use core::future::Future;
use core::ops::Deref;

use async_trait::async_trait;
use eonix_sync::{RwLock, Spin};

use super::{Ino, RenameData, WriteOffset};
use crate::io::{Buffer, Stream};
use crate::kernel::constants::{EINVAL, EPERM};
use crate::kernel::mem::{CachePage, PageCache, PageOffset};
use crate::kernel::timer::Instant;
use crate::kernel::vfs::dentry::Dentry;
use crate::kernel::vfs::types::{DeviceId, Format, Mode, Permission};
use crate::kernel::vfs::{SbRef, SbUse, SuperBlock};
use crate::prelude::KResult;

pub struct Inode {
    pub ino: Ino,
    pub format: Format,
    pub info: Spin<InodeInfo>,
    pub rwsem: RwLock<()>,
    page_cache: Spin<Weak<PageCache>>,
    sb: SbRef<dyn SuperBlock>,
    ops: Box<dyn InodeOpsErased>,
}

macro_rules! return_type {
    ($type:ty) => {
        $type
    };
    () => {
        ()
    };
}

macro_rules! define_inode_ops {
    {
        $(
            $(#[$attr:meta])*
            async fn $method:ident $(<$($lt:lifetime),+>)? (&self $(,)? $($name:ident : $type:ty $(,)?)*) $(-> $ret:ty)?
                $body:block
        )*

        ---

        $(
            $(#[$attr1:meta])*
            fn $method1:ident $(<$($lt1:lifetime),+>)? (&self $(,)? $($name1:ident : $type1:ty $(,)?)*) $(-> $ret1:ty)?
                $body1:block
        )*
    } => {
        #[allow(unused_variables)]
        pub trait InodeOps: Sized + Send + Sync + 'static {
            type SuperBlock: SuperBlock + Sized;

            $(
                $(#[$attr])*
                fn $method $(<$($lt),+>)? (
                &self,
                sb: SbUse<Self::SuperBlock>,
                inode: &InodeUse,
                $($name : $type),*
            ) -> impl Future<Output = return_type!($($ret)?)> + Send {
                async { $body }
            })*

            $(
                $(#[$attr1])*
                fn $method1 $(<$($lt1),+>)? (
                &self,
                sb: SbUse<Self::SuperBlock>,
                inode: &InodeUse,
                $($name1 : $type1),*
            ) -> return_type!($($ret1)?) {
                $body1
            })*
        }

        #[async_trait]
        trait InodeOpsErased: Any + Send + Sync + 'static {
            $(async fn $method $(<$($lt),+>)? (
                &self,
                sb: SbUse<dyn SuperBlock>,
                inode: &InodeUse,
                $($name : $type),*
            ) -> return_type!($($ret)?);)*

            $(fn $method1 $(<$($lt1),+>)? (
                &self,
                sb: SbUse<dyn SuperBlock>,
                inode: &InodeUse,
                $($name1 : $type1),*
            ) -> return_type!($($ret1)?);)*
        }

        #[async_trait]
        impl<T> InodeOpsErased for T
        where
            T: InodeOps,
        {
            $(async fn $method $(<$($lt),+>)? (
                &self,
                sb: SbUse<dyn SuperBlock>,
                inode: &InodeUse,
                $($name : $type),*
            ) -> return_type!($($ret)?) {
                self.$method(sb.downcast(), inode, $($name),*).await
            })*

            $(fn $method1 $(<$($lt1),+>)? (
                &self,
                sb: SbUse<dyn SuperBlock>,
                inode: &InodeUse,
                $($name1 : $type1),*
            ) -> return_type!($($ret1)?) {
                self.$method1(sb.downcast(), inode, $($name1),*)
            })*
        }

        impl InodeUse {
            $(pub async fn $method $(<$($lt),+>)? (
                &self,
                $($name : $type),*
            ) -> return_type!($($ret)?) {
                self.ops.$method(self.sbget()?, self, $($name),*).await
            })*

            $(pub fn $method1 $(<$($lt1),+>)? (
                &self,
                $($name1 : $type1),*
            ) -> return_type!($($ret1)?) {
                self.ops.$method1(self.sbget()?, self, $($name1),*)
            })*
        }
    };
}

define_inode_ops! {
    // DIRECTORY OPERATIONS

    async fn lookup(&self, dentry: &Arc<Dentry>) -> KResult<Option<InodeUse>> {
        Err(EPERM)
    }

    /// Read directory entries and call the given closure for each entry.
    ///
    /// # Returns
    /// - Ok(count): The number of entries read.
    /// - Ok(Err(err)): Some error occurred while calling the given closure.
    /// - Err(err): An error occurred while reading the directory.
    async fn readdir(
        &self,
        offset: usize,
        for_each_entry: &mut (dyn (for<'a> FnMut(&'a [u8], Ino) -> KResult<bool>) + Send),
    ) -> KResult<KResult<usize>> {
        Err(EPERM)
    }

    async fn create(&self, at: &Arc<Dentry>, mode: Permission) -> KResult<()> {
        Err(EPERM)
    }

    async fn mkdir(&self, at: &Dentry, mode: Permission) -> KResult<()> {
        Err(EPERM)
    }

    async fn mknod(&self, at: &Dentry, mode: Mode, dev: DeviceId) -> KResult<()> {
        Err(EPERM)
    }

    async fn unlink(&self, at: &Arc<Dentry>) -> KResult<()> {
        Err(EPERM)
    }

    async fn symlink(&self, at: &Arc<Dentry>, target: &[u8]) -> KResult<()> {
        Err(EPERM)
    }

    async fn rename(&self, rename_data: RenameData<'_, '_>) -> KResult<()> {
        Err(EPERM)
    }

    // FILE OPERATIONS

    async fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        Err(EINVAL)
    }

    async fn read_direct(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        Err(EINVAL)
    }

    async fn write(
        &self,
        stream: &mut dyn Stream,
        offset: WriteOffset<'_>
    ) -> KResult<usize> {
        Err(EINVAL)
    }

    async fn write_direct(
        &self,
        stream: &mut dyn Stream,
        offset: usize,
    ) -> KResult<usize> {
        Err(EINVAL)
    }

    async fn readlink(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        Err(EINVAL)
    }

    async fn truncate(&self, length: usize) -> KResult<()> {
        Err(EPERM)
    }

    async fn chmod(&self, perm: Permission) -> KResult<()> {
        Err(EPERM)
    }

    async fn chown(&self, uid: u32, gid: u32) -> KResult<()> {
        Err(EPERM)
    }

    // PAGE CACHE OPERATIONS
    async fn read_page(&self, page: &mut CachePage, offset: PageOffset) -> KResult<()> {
        Err(EINVAL)
    }

    async fn write_page(&self, page: &mut CachePage, offset: PageOffset) -> KResult<()> {
        Err(EINVAL)
    }

    async fn write_begin<'a>(
        &self,
        page_cache: &PageCache,
        pages: &'a mut BTreeMap<PageOffset, CachePage>,
        offset: usize,
        len: usize,
    ) -> KResult<&'a mut CachePage> {
        Err(EINVAL)
    }

    async fn write_end(
        &self,
        page_cache: &PageCache,
        pages: &mut BTreeMap<PageOffset, CachePage>,
        offset: usize,
        len: usize,
        copied: usize
    ) -> KResult<()> {
        Err(EINVAL)
    }

    ---

    fn devid(&self) -> KResult<DeviceId> {
        Err(EINVAL)
    }
}

#[derive(Debug, Clone)]
pub struct InodeInfo {
    pub size: u64,
    pub nlink: u64,

    pub uid: u32,
    pub gid: u32,
    pub perm: Permission,

    pub atime: Instant,
    pub ctime: Instant,
    pub mtime: Instant,
}

#[repr(transparent)]
pub struct InodeUse(Arc<Inode>);

impl InodeUse {
    pub fn new(
        sb: SbRef<dyn SuperBlock>,
        ino: Ino,
        format: Format,
        info: InodeInfo,
        ops: impl InodeOps,
    ) -> Self {
        let inode = Inode {
            sb,
            ino,
            format,
            info: Spin::new(info),
            rwsem: RwLock::new(()),
            page_cache: Spin::new(Weak::new()),
            ops: Box::new(ops),
        };

        Self(Arc::new(inode))
    }

    pub fn sbref(&self) -> SbRef<dyn SuperBlock> {
        self.sb.clone()
    }

    pub fn sbget(&self) -> KResult<SbUse<dyn SuperBlock>> {
        self.sb.get().map(|sb| sb as _)
    }

    pub fn get_priv<I>(&self) -> &I
    where
        I: InodeOps,
    {
        let ops = (&*self.ops) as &dyn Any;

        ops.downcast_ref()
            .expect("InodeUse::private: InodeOps type mismatch")
    }

    pub fn get_page_cache(&self) -> Arc<PageCache> {
        if let Some(cache) = self.page_cache.lock().upgrade() {
            return cache;
        }

        // Slow path...
        let cache = Arc::new(PageCache::new(self.clone()));
        let mut page_cache = self.page_cache.lock();
        if let Some(cache) = page_cache.upgrade() {
            return cache;
        }

        *page_cache = Arc::downgrade(&cache);
        cache
    }
}

impl Clone for InodeUse {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl core::fmt::Debug for InodeUse {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "InodeUse(ino={})", self.ino)
    }
}

impl Deref for InodeUse {
    type Target = Inode;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl PartialEq for InodeUse {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
