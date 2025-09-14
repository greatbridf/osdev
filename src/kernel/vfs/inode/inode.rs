use alloc::boxed::Box;
use core::{
    any::Any,
    future::Future,
    marker::Unsize,
    ops::{CoerceUnsized, Deref},
    pin::Pin,
};
use eonix_sync::Spin;

use alloc::sync::{Arc, Weak};
use async_trait::async_trait;

use crate::{
    io::{Buffer, Stream},
    kernel::{
        constants::{EINVAL, EPERM},
        mem::PageCache,
        timer::Instant,
        vfs::{
            dentry::Dentry,
            types::{DeviceId, Format, Mode, Permission},
            SbRef, SbUse, SuperBlock,
        },
    },
    prelude::KResult,
};

use super::{Ino, RenameData, WriteOffset};

pub trait InodeOps: Sized + Send + Sync + 'static {
    type SuperBlock: SuperBlock + Sized;

    fn ino(&self) -> Ino;
    fn format(&self) -> Format;
    fn info(&self) -> &Spin<InodeInfo>;

    fn super_block(&self) -> &SbRef<Self::SuperBlock>;

    fn page_cache(&self) -> Option<&PageCache>;
}

#[allow(unused_variables)]
pub trait InodeDirOps: InodeOps {
    fn lookup(
        &self,
        dentry: &Arc<Dentry>,
    ) -> impl Future<Output = KResult<Option<InodeUse<dyn Inode>>>> + Send {
        async { Err(EPERM) }
    }

    /// Read directory entries and call the given closure for each entry.
    ///
    /// # Returns
    /// - Ok(count): The number of entries read.
    /// - Ok(Err(err)): Some error occurred while calling the given closure.
    /// - Err(err): An error occurred while reading the directory.
    fn readdir<'r, 'a: 'r, 'b: 'r>(
        &'a self,
        offset: usize,
        for_each_entry: &'b mut (dyn FnMut(&[u8], Ino) -> KResult<bool> + Send),
    ) -> impl Future<Output = KResult<KResult<usize>>> + Send + 'r {
        async { Err(EPERM) }
    }

    fn create(
        &self,
        at: &Arc<Dentry>,
        mode: Permission,
    ) -> impl Future<Output = KResult<()>> + Send {
        async { Err(EPERM) }
    }

    fn mkdir(&self, at: &Dentry, mode: Permission) -> impl Future<Output = KResult<()>> + Send {
        async { Err(EPERM) }
    }

    fn mknod(
        &self,
        at: &Dentry,
        mode: Mode,
        dev: DeviceId,
    ) -> impl Future<Output = KResult<()>> + Send {
        async { Err(EPERM) }
    }

    fn unlink(&self, at: &Arc<Dentry>) -> impl Future<Output = KResult<()>> + Send {
        async { Err(EPERM) }
    }

    fn symlink(&self, at: &Arc<Dentry>, target: &[u8]) -> impl Future<Output = KResult<()>> + Send {
        async { Err(EPERM) }
    }

    fn rename(&self, rename_data: RenameData<'_, '_>) -> impl Future<Output = KResult<()>> + Send {
        async { Err(EPERM) }
    }
}

#[allow(unused_variables)]
pub trait InodeFileOps: InodeOps {
    fn read(
        &self,
        buffer: &mut dyn Buffer,
        offset: usize,
    ) -> impl Future<Output = KResult<usize>> + Send {
        async { Err(EINVAL) }
    }

    fn read_direct(
        &self,
        buffer: &mut dyn Buffer,
        offset: usize,
    ) -> impl Future<Output = KResult<usize>> + Send {
        async { Err(EINVAL) }
    }

    fn write(
        &self,
        stream: &mut dyn Stream,
        offset: WriteOffset<'_>,
    ) -> impl Future<Output = KResult<usize>> + Send {
        async { Err(EINVAL) }
    }

    fn write_direct(
        &self,
        stream: &mut dyn Stream,
        offset: usize,
    ) -> impl Future<Output = KResult<usize>> + Send {
        async { Err(EINVAL) }
    }

    fn devid(&self) -> KResult<DeviceId> {
        Err(EINVAL)
    }

    fn readlink(&self, buffer: &mut dyn Buffer) -> impl Future<Output = KResult<usize>> + Send {
        async { Err(EINVAL) }
    }

    fn truncate(&self, length: usize) -> impl Future<Output = KResult<()>> + Send {
        async { Err(EPERM) }
    }

    fn chmod(&self, perm: Permission) -> impl Future<Output = KResult<()>> + Send {
        async { Err(EPERM) }
    }

    fn chown(&self, uid: u32, gid: u32) -> impl Future<Output = KResult<()>> + Send {
        async { Err(EPERM) }
    }
}

#[async_trait]
pub trait InodeDir {
    async fn lookup(&self, dentry: &Arc<Dentry>) -> KResult<Option<InodeUse<dyn Inode>>>;
    async fn create(&self, at: &Arc<Dentry>, perm: Permission) -> KResult<()>;
    async fn mkdir(&self, at: &Dentry, perm: Permission) -> KResult<()>;
    async fn mknod(&self, at: &Dentry, mode: Mode, dev: DeviceId) -> KResult<()>;
    async fn unlink(&self, at: &Arc<Dentry>) -> KResult<()>;
    async fn symlink(&self, at: &Arc<Dentry>, target: &[u8]) -> KResult<()>;
    async fn rename(&self, rename_data: RenameData<'_, '_>) -> KResult<()>;

    fn readdir<'r, 'a: 'r, 'b: 'r>(
        &'a self,
        offset: usize,
        callback: &'b mut (dyn FnMut(&[u8], Ino) -> KResult<bool> + Send),
    ) -> Pin<Box<dyn Future<Output = KResult<KResult<usize>>> + Send + 'r>>;
}

#[async_trait]
pub trait InodeFile {
    async fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize>;
    async fn read_direct(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize>;
    async fn write(&self, stream: &mut dyn Stream, offset: WriteOffset<'_>) -> KResult<usize>;
    async fn write_direct(&self, stream: &mut dyn Stream, offset: usize) -> KResult<usize>;
    fn devid(&self) -> KResult<DeviceId>;
    async fn readlink(&self, buffer: &mut dyn Buffer) -> KResult<usize>;
    async fn truncate(&self, length: usize) -> KResult<()>;
    async fn chmod(&self, mode: Mode) -> KResult<()>;
    async fn chown(&self, uid: u32, gid: u32) -> KResult<()>;
}

pub trait Inode: InodeFile + InodeDir + Any + Send + Sync + 'static {
    fn ino(&self) -> Ino;
    fn format(&self) -> Format;
    fn info(&self) -> &Spin<InodeInfo>;

    // TODO: This might should be removed... Temporary workaround for now.
    fn page_cache(&self) -> Option<&PageCache>;

    fn sbref(&self) -> SbRef<dyn SuperBlock>;
    fn sbget(&self) -> KResult<SbUse<dyn SuperBlock>>;
}

#[async_trait]
impl<T> InodeFile for T
where
    T: InodeFileOps,
{
    async fn read(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        self.read(buffer, offset).await
    }

    async fn read_direct(&self, buffer: &mut dyn Buffer, offset: usize) -> KResult<usize> {
        self.read_direct(buffer, offset).await
    }

    async fn write(&self, stream: &mut dyn Stream, offset: WriteOffset<'_>) -> KResult<usize> {
        self.write(stream, offset).await
    }

    async fn write_direct(&self, stream: &mut dyn Stream, offset: usize) -> KResult<usize> {
        self.write_direct(stream, offset).await
    }

    fn devid(&self) -> KResult<DeviceId> {
        self.devid()
    }

    async fn readlink(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        self.readlink(buffer).await
    }

    async fn truncate(&self, length: usize) -> KResult<()> {
        self.truncate(length).await
    }

    async fn chmod(&self, mode: Mode) -> KResult<()> {
        self.chmod(Permission::new(mode.non_format_bits())).await
    }

    async fn chown(&self, uid: u32, gid: u32) -> KResult<()> {
        self.chown(uid, gid).await
    }
}

#[async_trait]
impl<T> InodeDir for T
where
    T: InodeDirOps,
{
    async fn lookup(&self, dentry: &Arc<Dentry>) -> KResult<Option<InodeUse<dyn Inode>>> {
        self.lookup(dentry).await
    }

    async fn create(&self, at: &Arc<Dentry>, perm: Permission) -> KResult<()> {
        self.create(at, perm).await
    }

    async fn mkdir(&self, at: &Dentry, perm: Permission) -> KResult<()> {
        self.mkdir(at, perm).await
    }

    async fn mknod(&self, at: &Dentry, mode: Mode, dev: DeviceId) -> KResult<()> {
        self.mknod(at, mode, dev).await
    }

    async fn unlink(&self, at: &Arc<Dentry>) -> KResult<()> {
        self.unlink(at).await
    }

    async fn symlink(&self, at: &Arc<Dentry>, target: &[u8]) -> KResult<()> {
        self.symlink(at, target).await
    }

    async fn rename(&self, rename_data: RenameData<'_, '_>) -> KResult<()> {
        self.rename(rename_data).await
    }

    fn readdir<'r, 'a: 'r, 'b: 'r>(
        &'a self,
        offset: usize,
        callback: &'b mut (dyn FnMut(&[u8], Ino) -> KResult<bool> + Send),
    ) -> Pin<Box<dyn Future<Output = KResult<KResult<usize>>> + Send + 'r>> {
        Box::pin(self.readdir(offset, callback))
    }
}

impl<T> Inode for T
where
    T: InodeOps + InodeFile + InodeDir,
{
    fn ino(&self) -> Ino {
        self.ino()
    }

    fn format(&self) -> Format {
        self.format()
    }

    fn info(&self) -> &Spin<InodeInfo> {
        self.info()
    }

    fn page_cache(&self) -> Option<&PageCache> {
        self.page_cache()
    }

    fn sbref(&self) -> SbRef<dyn SuperBlock> {
        self.super_block().clone()
    }

    fn sbget(&self) -> KResult<SbUse<dyn SuperBlock>> {
        self.super_block().get().map(|sb| sb as _)
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

pub struct InodeUse<I>(Arc<I>)
where
    I: Inode + ?Sized;

impl<I> InodeUse<I>
where
    I: Inode,
{
    pub fn new(inode: I) -> Self {
        Self(Arc::new(inode))
    }

    pub fn new_cyclic(inode_func: impl FnOnce(&Weak<I>) -> I) -> Self {
        Self(Arc::new_cyclic(inode_func))
    }
}

impl<I> InodeUse<I>
where
    I: Inode + ?Sized,
{
    pub fn as_raw(&self) -> *const I {
        Arc::as_ptr(&self.0)
    }
}

impl<T, U> CoerceUnsized<InodeUse<U>> for InodeUse<T>
where
    T: Inode + Unsize<U> + ?Sized,
    U: Inode + ?Sized,
{
}

impl<I> Clone for InodeUse<I>
where
    I: Inode + ?Sized,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<I> core::fmt::Debug for InodeUse<I>
where
    I: Inode + ?Sized,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "InodeUse(ino={})", self.ino())
    }
}

impl<I> Deref for InodeUse<I>
where
    I: Inode + ?Sized,
{
    type Target = I;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}
