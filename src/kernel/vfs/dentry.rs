pub mod dcache;

use core::{
    hash::{BuildHasher, BuildHasherDefault, Hasher},
    sync::atomic::AtomicPtr,
};

use crate::{
    hash::KernelHasher,
    io::{ByteBuffer, RawBuffer},
    path::{Path, PathComponent},
    prelude::*,
    rcu::{RCUNode, RCUPointer},
};

use alloc::sync::Arc;
use bindings::{EINVAL, ELOOP, ENOENT, ENOTDIR};

use super::inode::Inode;

struct DentryData {
    inode: Arc<Inode>,
    flags: u64,
}

pub struct Dentry {
    // Const after insertion into dcache
    parent: Arc<Dentry>,
    name: Arc<[u8]>,
    hash: u64,

    // Used by the dentry cache
    prev: AtomicPtr<Dentry>,
    next: AtomicPtr<Dentry>,

    // RCU Mutable
    data: RCUPointer<DentryData>,
}

const D_DIRECTORY: u64 = 1;
const D_MOUNTPOINT: u64 = 2;
const D_SYMLINK: u64 = 4;
const D_REGULAR: u64 = 8;

impl RCUNode<Dentry> for Dentry {
    fn rcu_prev(&self) -> &AtomicPtr<Self> {
        &self.prev
    }

    fn rcu_next(&self) -> &AtomicPtr<Self> {
        &self.next
    }
}

impl Dentry {
    fn rehash(self: &Arc<Self>) -> u64 {
        let builder: BuildHasherDefault<KernelHasher> = Default::default();
        let mut hasher = builder.build_hasher();

        hasher.write_usize(self.parent_addr() as usize);
        hasher.write(self.name.as_ref());

        hasher.finish()
    }

    fn find(self: &Arc<Self>, name: &[u8]) -> KResult<Arc<Self>> {
        let data = self.data.load();
        let data = data.as_ref().ok_or(ENOENT)?;

        if data.flags & D_DIRECTORY == 0 {
            return Err(ENOTDIR);
        }

        match name {
            b"." => Ok(self.clone()),
            b".." => Ok(self.parent.clone()),
            _ => {
                let dentry = Dentry::create(self.clone(), name);
                Ok(dcache::d_find_fast(&dentry).unwrap_or_else(|| {
                    dcache::d_try_revalidate(&dentry);
                    dcache::d_add(&dentry);

                    dentry
                }))
            }
        }
    }
}

impl Dentry {
    pub fn create(parent: Arc<Dentry>, name: &[u8]) -> Arc<Self> {
        let mut val = Arc::new(Self {
            parent,
            name: Arc::from(name),
            hash: 0,
            prev: AtomicPtr::default(),
            next: AtomicPtr::default(),
            data: RCUPointer::empty(),
        });
        let hash = val.rehash();
        let val_mut = Arc::get_mut(&mut val).unwrap();
        val_mut.hash = hash;

        val
    }

    /// Check the equality of two denties inside the same dentry cache hash group
    /// where `other` is identified by `hash`, `parent` and `name`
    ///
    fn hash_eq(self: &Arc<Self>, other: &Arc<Self>) -> bool {
        self.hash == other.hash
            && self.parent_addr() == other.parent_addr()
            && self.name == other.name
    }

    pub fn name(&self) -> &Arc<[u8]> {
        &self.name
    }

    pub fn parent(&self) -> &Arc<Self> {
        &self.parent
    }

    pub fn parent_addr(&self) -> *const Self {
        Arc::as_ptr(&self.parent)
    }

    fn save_data(&self, inode: Arc<Inode>, flags: u64) -> KResult<()> {
        let new = DentryData { inode, flags };

        let old = self.data.swap(Some(Arc::new(new)));
        assert!(old.is_none());

        Ok(())
    }

    pub fn save_reg(&self, file: Arc<Inode>) -> KResult<()> {
        self.save_data(file, D_REGULAR)
    }

    pub fn save_symlink(&self, link: Arc<Inode>) -> KResult<()> {
        self.save_data(link, D_SYMLINK)
    }

    pub fn save_dir(&self, dir: Arc<Inode>) -> KResult<()> {
        self.save_data(dir, D_DIRECTORY)
    }

    pub fn invalidate(&self) -> KResult<()> {
        let old = self.data.swap(None);
        assert!(old.is_some());

        Ok(())
    }

    pub fn get_inode(&self) -> KResult<Arc<Inode>> {
        self.data
            .load()
            .as_ref()
            .ok_or(EINVAL)
            .map(|data| data.inode.clone())
    }

    /// This function is used to get the **borrowed** dentry from a raw pointer
    pub fn from_raw(raw: &*const Self) -> BorrowedArc<Self> {
        assert!(!raw.is_null());

        BorrowedArc::new(raw)
    }

    pub fn is_directory(&self) -> bool {
        let data = self.data.load();
        data.as_ref()
            .map_or(false, |data| data.flags & D_DIRECTORY != 0)
    }
}

#[repr(C)]
pub struct FsContext {
    root: *const Dentry,
}

impl Dentry {
    fn resolve_directory(
        context: &FsContext,
        dentry: Arc<Self>,
        nrecur: u32,
    ) -> KResult<Arc<Self>> {
        if nrecur >= 16 {
            return Err(ELOOP);
        }

        let data = dentry.data.load();
        let data = data.as_ref().ok_or(ENOENT)?;

        match data.flags {
            flags if flags & D_REGULAR != 0 => Err(ENOTDIR),
            flags if flags & D_DIRECTORY != 0 => Ok(dentry),
            flags if flags & D_SYMLINK != 0 => {
                let mut buffer = [0u8; 256];
                let mut buffer = ByteBuffer::new(&mut buffer);

                data.inode.readlink(&data.inode, &mut buffer)?;
                let path = Path::new(buffer.data())?;

                let dentry = Self::open_recursive(
                    context,
                    &dentry.parent,
                    path,
                    true,
                    nrecur + 1,
                )?;

                Self::resolve_directory(context, dentry, nrecur + 1)
            }
            _ => panic!("Invalid dentry flags"),
        }
    }

    fn open_recursive(
        context: &FsContext,
        cwd: &Arc<Self>,
        path: Path,
        follow: bool,
        nrecur: u32,
    ) -> KResult<Arc<Self>> {
        // too many recursive search layers will cause stack overflow
        // so we use 16 for now
        if nrecur >= 16 {
            return Err(ELOOP);
        }

        let mut cwd = if path.is_absolute() {
            Dentry::from_raw(&context.root).clone()
        } else {
            cwd.clone()
        };

        let root_dentry = Dentry::from_raw(&context.root);

        for item in path.iter() {
            if let PathComponent::TrailingEmpty = item {
                if cwd.data.load().as_ref().is_none() {
                    return Ok(cwd);
                }
            }

            cwd = Self::resolve_directory(context, cwd, nrecur)?;

            match item {
                PathComponent::TrailingEmpty | PathComponent::Current => {} // pass
                PathComponent::Parent => {
                    if !cwd.hash_eq(root_dentry.as_ref()) {
                        cwd = Self::resolve_directory(
                            context,
                            cwd.parent.clone(),
                            nrecur,
                        )?;
                    }
                    continue;
                }
                PathComponent::Name(name) => {
                    cwd = cwd.find(name)?;
                }
            }
        }

        if follow {
            let data = cwd.data.load();

            if let Some(data) = data.as_ref() {
                if data.flags & D_SYMLINK != 0 {
                    let data = cwd.data.load();
                    let data = data.as_ref().unwrap();
                    let mut buffer = [0u8; 256];
                    let mut buffer = ByteBuffer::new(&mut buffer);

                    data.inode.readlink(&data.inode, &mut buffer)?;
                    let path = Path::new(buffer.data())?;

                    cwd = Self::open_recursive(
                        context,
                        &cwd.parent,
                        path,
                        true,
                        nrecur + 1,
                    )?;
                }
            }
        }

        Ok(cwd)
    }
}

#[no_mangle]
pub extern "C" fn dentry_open(
    context_root: *const Dentry,
    cwd: *const Dentry, // borrowed
    path: *const u8,
    path_len: usize,
    follow: bool,
) -> *const Dentry {
    match (|| -> KResult<Arc<Dentry>> {
        let path =
            Path::new(unsafe { core::slice::from_raw_parts(path, path_len) })?;

        let context = FsContext { root: context_root };

        Dentry::open_recursive(
            &context,
            Dentry::from_raw(&cwd).as_ref(),
            path,
            follow,
            0,
        )
    })() {
        Ok(dentry) => Arc::into_raw(dentry),
        Err(err) => (-(err as i32) as usize) as *const Dentry,
    }
}

#[no_mangle]
pub extern "C" fn d_path(
    dentry: *const Dentry,
    root: *const Dentry,
    mut buffer: *mut u8,
    bufsize: usize,
) -> i32 {
    let mut buffer = RawBuffer::new_from_raw(&mut buffer, bufsize);

    match (|| {
        let mut dentry = Dentry::from_raw(&dentry).clone();
        let root = Dentry::from_raw(&root);

        let mut path = vec![];

        while Arc::as_ptr(&dentry) != Arc::as_ptr(root.as_ref()) {
            if path.len() > 32 {
                return Err(ELOOP);
            }

            path.push(dentry.name().clone());
            dentry = dentry.parent().clone();
        }

        const ERANGE: u32 = 34;
        buffer.fill(b"/")?.ok_or(ERANGE)?;
        for item in path.iter().rev().map(|name| name.as_ref()) {
            buffer.fill(item)?.ok_or(ERANGE)?;
            buffer.fill(b"/")?.ok_or(ERANGE)?;
        }

        buffer.fill(&[0])?.ok_or(ERANGE)?;

        Ok(())
    })() {
        Ok(_) => 0,
        Err(err) => -(err as i32),
    }
}

#[no_mangle]
pub extern "C" fn r_dget(dentry: *const Dentry) -> *const Dentry {
    debug_assert!(!dentry.is_null());

    unsafe { Arc::increment_strong_count(dentry) };
    dentry
}

#[no_mangle]
pub extern "C" fn r_dput(dentry: *const Dentry) {
    debug_assert!(!dentry.is_null());

    unsafe { Arc::from_raw(dentry) };
}

#[no_mangle]
pub extern "C" fn r_dentry_get_inode(dentry: *const Dentry) -> *const Inode {
    let dentry = Dentry::from_raw(&dentry);

    match dentry.get_inode() {
        Ok(inode) => Arc::into_raw(inode),
        Err(err) => {
            dont_check!(println!(
                "[kernel:warn] r_dentry_get_inode: {:?}",
                err
            ));
            core::ptr::null()
        }
    }
}

#[no_mangle]
pub extern "C" fn r_dentry_is_directory(dentry: *const Dentry) -> bool {
    let dentry = Dentry::from_raw(&dentry);

    dentry
        .data
        .load()
        .as_ref()
        .map_or(false, |data| data.flags & D_DIRECTORY != 0)
}

#[no_mangle]
pub extern "C" fn r_dentry_is_invalid(dentry: *const Dentry) -> bool {
    let dentry = Dentry::from_raw(&dentry);

    dentry.data.load().is_none()
}
