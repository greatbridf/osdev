use alloc::boxed::Box;
use alloc::sync::Arc;
use core::future::Future;
use core::hash::{BuildHasher, BuildHasherDefault, Hasher};
use core::ops::Deref;
use core::pin::Pin;

use arcref::{ArcRef, AsArcRef};
use posix_types::result::PosixError;

use super::dcache::{self, DCacheItem};
use super::{Dentry, DentryKind};
use crate::hash::KernelHasher;
use crate::io::ByteBuffer;
use crate::kernel::constants::ELOOP;
use crate::kernel::vfs::inode::InodeUse;
use crate::kernel::vfs::FsContext;
use crate::path::{Path, PathComponent, PathIterator};
use crate::prelude::KResult;
use crate::rcu::{rcu_read_lock, RCUReadLock};

struct DentryFind<'a, 'b> {
    parent: &'a Dentry,
    name: &'b [u8],
    hash: usize,
}

pub enum WalkResultRcu<'rcu, 'path> {
    Err(PosixError),
    Ok(ArcRef<'rcu, Dentry>),
    Symlink {
        symlink: ArcRef<'rcu, Dentry>,
        inode: InodeUse,
    },
    Miss {
        parent: ArcRef<'rcu, Dentry>,
        name: &'path [u8],
    },
}

pub enum WalkResult {
    Err(PosixError),
    Ok(Arc<Dentry>),
    Symlink {
        symlink: Arc<Dentry>,
        inode: InodeUse,
    },
}

impl Dentry {
    /// Quick path of the dentry find operation.
    ///
    /// Check invalid and non-directory dentries, return immediately on dot and
    /// dotdot component, and do a quick rcu dcache lookup.
    ///
    /// Note that while `Some(dentry)` guarantees present and valid dentry,
    /// returning `None` is acceptable if the actual file exists but is not in
    /// the dentry cache. If so, we should check again with `lookup`.
    fn find_rcu<'r, 's: 'r>(
        self: ArcRef<'s, Self>,
        name: &[u8],
        rcu_read: &'r RCUReadLock,
    ) -> Result<Option<ArcRef<'r, Self>>, PosixError> {
        match self.inode.load() {
            Some((DentryKind::Directory, _)) => {}
            Some(_) => return Err(PosixError::ENOTDIR),
            None => return Err(PosixError::ENOENT),
        }

        match name {
            b"." => Ok(Some(self)),
            b".." => Ok(Some(
                self.parent
                    .dereference(rcu_read)
                    .expect("The field `parent` should be non-null"),
            )),
            _ => {
                let dentry_find = DentryFind::new(&self, name);
                Ok(dcache::d_find_rcu(&dentry_find, rcu_read))
            }
        }
    }

    async fn find_slow(self: &Arc<Self>, name: &[u8]) -> Result<Arc<Self>, PosixError> {
        let dentry = Dentry::create(self.clone(), name);

        let _ = dcache::d_try_revalidate(&dentry).await;
        dcache::d_add(dentry.clone());

        Ok(dentry)
    }

    pub async fn find_full(self: &Arc<Self>, name: &[u8]) -> Result<Arc<Self>, PosixError> {
        if let Some(dentry) = self.aref().find_rcu(name, &rcu_read_lock())? {
            return Ok(dentry.clone_arc());
        }

        self.find_slow(name).await
    }
}

impl FsContext {
    /// Walk the pathname and try to find the corresponding dentry FAST without
    /// consulting the VFS for invalid dentries encountered.
    fn walk_rcu<'rcu, 'path>(
        &self,
        mut current: ArcRef<'rcu, Dentry>,
        iter: &mut PathIterator<'path>,
        rcu_read: &'rcu RCUReadLock,
    ) -> WalkResultRcu<'rcu, 'path> {
        use PathComponent::*;

        loop {
            let inode = current.inode.load();

            if iter.is_empty() {
                break;
            }

            // Skip symlink resolution in rcu walk without consuming the iter.
            if let Some((DentryKind::Symlink, inode)) = inode {
                return WalkResultRcu::Symlink {
                    symlink: current,
                    inode: inode.clone(),
                };
            }

            let Some(component) = iter.next() else {
                break;
            };

            match (inode, component) {
                // Skip trailing empty and dot for normal directories.
                (Some((DentryKind::Directory, _)), TrailingEmpty | Current) => {}
                // Walk to parent directory unless we are at the filesystem root.
                (Some((DentryKind::Directory, _)), Parent) => {
                    if current.ptr_eq_arc(&self.fsroot) {
                        continue;
                    }

                    current = current
                        .parent
                        .dereference(&rcu_read)
                        .expect("parent should exist");
                }
                // Normal directory traversal
                (Some((DentryKind::Directory, _)), Name(name)) => {
                    match current.find_rcu(name, &rcu_read) {
                        Err(err) => return WalkResultRcu::Err(err),
                        Ok(Some(found)) => {
                            current = found;
                        }
                        Ok(None) => {
                            return WalkResultRcu::Miss {
                                name,
                                parent: current,
                            };
                        }
                    }
                }
                // Not a directory, fail and exit.
                (Some(_), _) => return WalkResultRcu::Err(PosixError::ENOTDIR),
                // Return invalid trailing entries directly.
                (None, TrailingEmpty) => return WalkResultRcu::Ok(current),
                // Invalid intermediate entries are not acceptable.
                (None, _) => return WalkResultRcu::Err(PosixError::ENOENT),
            }
        }

        WalkResultRcu::Ok(current)
    }

    /// Walk the pathname slowly with refcounts held and VFS lookups.
    async fn walk_slow(&self, mut current: Arc<Dentry>, iter: &mut PathIterator<'_>) -> WalkResult {
        use PathComponent::*;

        loop {
            // `current` should be the parent directory and `component` is the
            // next path component we are stepping into.

            if iter.is_empty() {
                break;
            }

            if let Some((DentryKind::Symlink, inode)) = current.inode.load() {
                return WalkResult::Symlink {
                    inode: inode.clone(),
                    symlink: current,
                };
            }

            let Some(component) = iter.next() else {
                break;
            };

            match (current.inode.load(), &component) {
                // Normal directory traversal
                (Some((DentryKind::Directory, _)), _) => {}
                // Not a directory, fail and exit.
                (Some(_), _) => return WalkResult::Err(PosixError::ENOTDIR),
                // Return invalid trailing entries directly.
                (None, TrailingEmpty) => return WalkResult::Ok(current),
                // Invalid intermediate entries are not acceptable.
                (None, _) => return WalkResult::Err(PosixError::ENOENT),
            }

            match component {
                PathComponent::TrailingEmpty => {}
                PathComponent::Current => {}
                PathComponent::Parent => {
                    if current.hash_eq(&self.fsroot) {
                        continue;
                    }

                    let parent = current.parent().clone();
                    current = parent;
                }
                PathComponent::Name(name) => {
                    match current.find_full(name).await {
                        Ok(found) => current = found,
                        Err(err) => return WalkResult::Err(err),
                    };
                }
            }
        }

        WalkResult::Ok(current)
    }

    /// Walk the pathname and get an accurate answer. Stop at symlinks.
    async fn walk_full(
        &self,
        current: ArcRef<'_, Dentry>,
        iter: &mut PathIterator<'_>,
    ) -> WalkResult {
        let (parent_slow, name_slow);

        match self.walk_rcu(current, iter, &rcu_read_lock()) {
            WalkResultRcu::Err(error) => return WalkResult::Err(error.into()),
            WalkResultRcu::Ok(dentry) => return WalkResult::Ok(dentry.clone_arc()),
            WalkResultRcu::Symlink { symlink, inode } => {
                return WalkResult::Symlink {
                    symlink: symlink.clone_arc(),
                    inode,
                };
            }
            WalkResultRcu::Miss { parent, name } => {
                // Fallback to regular refcounted lookup
                parent_slow = parent.clone_arc();
                name_slow = name;
            }
        }

        match parent_slow.find_slow(name_slow).await {
            Ok(found) => self.walk_slow(found, iter).await,
            Err(err) => return WalkResult::Err(err),
        }
    }

    pub async fn follow_symlink(
        &self,
        symlink: ArcRef<'_, Dentry>,
        inode: &InodeUse,
        nr_follows: u32,
    ) -> KResult<Arc<Dentry>> {
        let mut target = [0; 256];
        let mut target = ByteBuffer::new(&mut target);
        inode.readlink(&mut target).await?;

        self.walk_recursive(
            &symlink.parent().clone(),
            Path::new(target.data()).unwrap(),
            nr_follows + 1,
        )
        .await
    }

    fn follow_symlink_boxed<'r, 'a: 'r, 'b: 'r, 'c: 'r>(
        &'a self,
        symlink: ArcRef<'b, Dentry>,
        inode: &'c InodeUse,
        nr_follows: u32,
    ) -> Pin<Box<dyn Future<Output = KResult<Arc<Dentry>>> + Send + 'r>> {
        Box::pin(self.follow_symlink(symlink, inode, nr_follows))
    }

    async fn walk_recursive(
        &self,
        cwd: &Arc<Dentry>,
        path: &Path,
        nr_follows: u32,
    ) -> KResult<Arc<Dentry>> {
        const MAX_NR_FOLLOWS: u32 = 16;

        let mut current_owned;
        let mut current;
        if path.is_absolute() {
            current = self.fsroot.aref();
        } else {
            current = cwd.aref();
        }

        let mut path_iter = path.iter();

        loop {
            match self.walk_full(current, &mut path_iter).await {
                WalkResult::Err(posix_error) => return Err(posix_error.into()),
                WalkResult::Ok(dentry) => return Ok(dentry),
                WalkResult::Symlink { symlink, inode } => {
                    if nr_follows >= MAX_NR_FOLLOWS {
                        return Err(ELOOP);
                    }

                    current_owned = self
                        .follow_symlink_boxed(symlink.aref(), &inode, nr_follows)
                        .await?;
                    current = current_owned.aref();
                }
            }
        }
    }

    pub async fn start_recursive_walk(
        &self,
        cwd: &Arc<Dentry>,
        path: &Path,
    ) -> KResult<Arc<Dentry>> {
        self.walk_recursive(cwd, path, 0).await
    }
}

impl<'a, 'b> DentryFind<'a, 'b> {
    fn new(parent: &'a Dentry, name: &'b [u8]) -> Self {
        let builder: BuildHasherDefault<KernelHasher> = Default::default();
        let mut hasher = builder.build_hasher();

        hasher.write_usize(parent as *const _ as usize);
        hasher.write(name);
        let hash = hasher.finish() as usize;

        Self { parent, name, hash }
    }
}

impl DCacheItem for DentryFind<'_, '_> {
    fn d_hash(&self) -> usize {
        self.hash
    }

    fn d_parent(&self) -> *const Dentry {
        self.parent as *const _
    }

    fn d_name<'r, 'a: 'r, 'b: 'a>(
        &'a self,
        _rcu_read: &'b RCUReadLock,
    ) -> impl Deref<Target = [u8]> + 'r {
        self.name
    }
}
