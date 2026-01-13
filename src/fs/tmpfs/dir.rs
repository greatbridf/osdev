use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use eonix_log::println_warn;
use eonix_sync::{LazyLock, RwLock};

use super::file::{DeviceInode, FileInode, SymlinkInode};
use super::TmpFs;
use crate::kernel::constants::{EEXIST, EINVAL, EISDIR, ENOENT, ENOSYS, ENOTDIR};
use crate::kernel::timer::Instant;
use crate::kernel::vfs::dentry::{dcache, Dentry};
use crate::kernel::vfs::inode::{Ino, InodeInfo, InodeOps, InodeUse, RenameData};
use crate::kernel::vfs::types::{DeviceId, Format, Mode, Permission};
use crate::kernel::vfs::{SbRef, SbUse};
use crate::prelude::KResult;

pub struct DirectoryInode {
    entries: RwLock<Vec<(Arc<[u8]>, Ino)>>,
}

fn link(dir: &InodeUse, entries: &mut Vec<(Arc<[u8]>, Ino)>, name: Arc<[u8]>, file: &InodeUse) {
    let mut dir_info = dir.info.lock();
    let mut file_info = file.info.lock();

    let now = Instant::now();

    file_info.nlink += 1;
    file_info.ctime = now;

    dir_info.size += 1;
    dir_info.mtime = now;
    dir_info.ctime = now;

    entries.push((name, file.ino));
}

impl DirectoryInode {
    pub fn new(ino: Ino, sb: SbRef<TmpFs>, perm: Permission) -> InodeUse {
        static DOT: LazyLock<Arc<[u8]>> = LazyLock::new(|| Arc::from(b".".as_slice()));

        let now = Instant::now();

        InodeUse::new(
            sb,
            ino,
            Format::DIR,
            InodeInfo {
                size: 1,
                nlink: 1, // link from `.` to itself
                perm,
                ctime: now,
                mtime: now,
                atime: now,
                uid: 0,
                gid: 0,
            },
            Self {
                entries: RwLock::new(vec![(DOT.clone(), ino)]),
            },
        )
    }

    fn do_unlink(
        &self,
        file: &InodeUse,
        filename: &[u8],
        entries: &mut Vec<(Arc<[u8]>, Ino)>,
        now: Instant,
        decrease_size: bool,
        self_info: &mut InodeInfo,
        file_info: &mut InodeInfo,
    ) -> KResult<()> {
        // SAFETY: `file_lock` has done the synchronization
        if file.format == Format::DIR {
            return Err(EISDIR);
        }

        let file_ino = file.ino;
        entries.retain(|(name, ino)| *ino != file_ino || name.as_ref() != filename);

        if decrease_size {
            self_info.size -= 1;
        }

        self_info.mtime = now;
        self_info.ctime = now;

        // The last reference to the inode is held by some dentry
        // and will be released when the dentry is released

        file_info.nlink -= 1;
        file_info.ctime = now;

        // TODO!!!: Remove the file if nlink == 1

        Ok(())
    }
}

impl InodeOps for DirectoryInode {
    type SuperBlock = TmpFs;

    async fn readdir(
        &self,
        sb: SbUse<Self::SuperBlock>,
        _: &InodeUse,
        offset: usize,
        for_each_entry: &mut (dyn FnMut(&[u8], Ino) -> KResult<bool> + Send),
    ) -> KResult<KResult<usize>> {
        let _sb = sb;
        let entries = self.entries.read().await;

        let mut count = 0;
        for entry in entries.iter().skip(offset) {
            match for_each_entry(&entry.0, entry.1) {
                Err(err) => return Ok(Err(err)),
                Ok(false) => break,
                Ok(true) => count += 1,
            }
        }

        Ok(Ok(count))
    }

    async fn create(
        &self,
        sb: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        at: &Arc<Dentry>,
        perm: Permission,
    ) -> KResult<()> {
        let mut entries = self.entries.write().await;

        let ino = sb.backend.assign_ino();
        let file = FileInode::new(ino, sb.get_ref(), 0, perm);

        link(inode, &mut entries, at.get_name(), &file);
        at.fill(file);

        Ok(())
    }

    async fn mknod(
        &self,
        sb: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        at: &Dentry,
        mode: Mode,
        dev: DeviceId,
    ) -> KResult<()> {
        if !mode.is_chr() && !mode.is_blk() {
            return Err(EINVAL);
        }

        let mut entries = self.entries.write().await;

        let ino = sb.backend.assign_ino();
        let file = DeviceInode::new(ino, sb.get_ref(), mode, dev);

        link(inode, &mut entries, at.get_name(), &file);
        at.fill(file);

        Ok(())
    }

    async fn symlink(
        &self,
        sb: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        at: &Arc<Dentry>,
        target: &[u8],
    ) -> KResult<()> {
        let mut entries = self.entries.write().await;

        let ino = sb.backend.assign_ino();
        let file = SymlinkInode::new(ino, sb.get_ref(), target.into());

        link(inode, &mut entries, at.get_name(), &file);
        at.fill(file);

        Ok(())
    }

    async fn mkdir(
        &self,
        sb: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        at: &Dentry,
        perm: Permission,
    ) -> KResult<()> {
        let mut entries = self.entries.write().await;

        let ino = sb.backend.assign_ino();
        let new_dir = DirectoryInode::new(ino, sb.get_ref(), perm);

        link(inode, &mut entries, at.get_name(), &new_dir);
        at.fill(new_dir);

        Ok(())
    }

    async fn unlink(
        &self,
        _sb: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        at: &Arc<Dentry>,
    ) -> KResult<()> {
        let mut entries = self.entries.write().await;

        let file = at.get_inode()?;
        let filename = at.get_name();

        self.do_unlink(
            &file,
            &filename,
            &mut entries,
            Instant::now(),
            true,
            &mut inode.info.lock(),
            &mut file.info.lock(),
        )?;

        // Remove the dentry from the dentry cache immediately
        // so later lookup will fail with ENOENT
        dcache::d_remove(at);

        Ok(())
    }

    async fn rename(
        &self,
        sb: SbUse<Self::SuperBlock>,
        inode: &InodeUse,
        rename_data: RenameData<'_, '_>,
    ) -> KResult<()> {
        let _rename_lock = sb.backend.rename_lock.lock().await;
        let mut self_entries = self.entries.write().await;

        let RenameData {
            old_dentry,
            new_dentry,
            new_parent,
            is_exchange,
            no_replace,
        } = rename_data;

        if is_exchange {
            println_warn!("TmpFs does not support exchange rename for now");
            return Err(ENOSYS);
        }

        let old_file = old_dentry.get_inode()?;
        let new_file = new_dentry.inode();

        if no_replace && new_file.is_some() {
            return Err(EEXIST);
        }

        if inode == &new_parent {
            // Same directory rename
            // Remove from old location and add to new location
            let old_ino = old_file.ino;
            let new_ino = new_file.as_ref().map(|f| f.ino);
            let old_name = old_dentry.get_name();
            let new_name = new_dentry.get_name();

            // Find the old and new entries in the directory after we've locked the directory.
            let (mut old_ent_idx, mut new_ent_idx) = (None, None);
            for (idx, (name, ino)) in self_entries.iter().enumerate() {
                if *ino == old_ino && *name == old_name {
                    old_ent_idx = Some(idx);
                }

                if Some(*ino) == new_ino && *name == new_name {
                    new_ent_idx = Some(idx);
                }
            }

            let Some(old_ent_idx) = old_ent_idx else {
                return Err(ENOENT);
            };

            if Some(old_ent_idx) == new_ent_idx {
                return Ok(());
            }

            let now = Instant::now();
            if let Some(new_idx) = new_ent_idx {
                // Replace existing file (i.e. rename the old and unlink the new)
                let new_file = new_file.unwrap();

                match (new_file.format, old_file.format) {
                    (Format::DIR, _) => return Err(EISDIR),
                    (_, Format::DIR) => return Err(ENOTDIR),
                    _ => {}
                }

                self_entries.remove(new_idx);

                inode.info.lock().size -= 1;

                // The last reference to the inode is held by some dentry
                // and will be released when the dentry is released

                let mut new_info = new_file.info.lock();

                new_info.nlink -= 1;
                new_info.mtime = now;
                new_info.ctime = now;
            }

            let (name, _) = &mut self_entries[old_ent_idx];
            *name = new_dentry.get_name();

            let mut self_info = inode.info.lock();
            self_info.mtime = now;
            self_info.ctime = now;
        } else {
            // Cross-directory rename - handle similar to same directory case

            // Get new parent directory
            let new_parent = new_dentry.parent().get_inode()?;
            assert_eq!(new_parent.format, Format::DIR);

            let new_parent_priv = new_parent.get_priv::<DirectoryInode>();
            let mut new_entries = new_parent_priv.entries.write().await;

            let old_ino = old_file.ino;
            let new_ino = new_file.as_ref().map(|f| f.ino);
            let old_name = old_dentry.get_name();
            let new_name = new_dentry.get_name();

            // Find the old entry in the old directory
            let old_pos = self_entries
                .iter()
                .position(|(name, ino)| *ino == old_ino && *name == old_name)
                .ok_or(ENOENT)?;

            // Find the new entry in the new directory (if it exists)
            let has_new = new_entries
                .iter()
                .position(|(name, ino)| Some(*ino) == new_ino && *name == new_name)
                .is_some();

            let now = Instant::now();

            if has_new {
                // Replace existing file (i.e. move the old and unlink the new)
                let new_file = new_file.unwrap();

                match (old_file.format, new_file.format) {
                    (Format::DIR, Format::DIR) => {}
                    (Format::DIR, _) => return Err(ENOTDIR),
                    (_, _) => {}
                }

                // Unlink the old file that was replaced
                new_parent_priv.do_unlink(
                    &new_file,
                    &new_name,
                    &mut new_entries,
                    now,
                    false,
                    &mut new_parent.info.lock(),
                    &mut new_file.info.lock(),
                )?;
            } else {
                let mut info = new_parent.info.lock();

                info.size += 1;
                info.mtime = now;
                info.ctime = now;
            }

            // Remove from old directory
            self_entries.remove(old_pos);

            // Add new entry
            new_entries.push((new_name, old_ino));

            let mut self_info = inode.info.lock();
            self_info.size -= 1;
            self_info.mtime = now;
            self_info.ctime = now;
        }

        dcache::d_exchange(old_dentry, new_dentry).await;
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
}
