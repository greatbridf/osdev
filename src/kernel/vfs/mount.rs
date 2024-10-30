use crate::prelude::*;

use alloc::{
    collections::btree_map::{BTreeMap, Entry},
    sync::Arc,
};
use bindings::{EEXIST, ENODEV, ENOTDIR};

use lazy_static::lazy_static;

use super::{
    dentry::{dcache, Dentry},
    inode::Inode,
    vfs::Vfs,
};

pub const MS_RDONLY: u64 = 1 << 0;
pub const MS_NOSUID: u64 = 1 << 1;
pub const MS_NODEV: u64 = 1 << 2;
pub const MS_NOEXEC: u64 = 1 << 3;
pub const MS_NOATIME: u64 = 1 << 10;
pub const MS_RELATIME: u64 = 1 << 21;
pub const MS_STRICTATIME: u64 = 1 << 24;
pub const MS_LAZYTIME: u64 = 1 << 25;

const MOUNT_FLAGS: [(u64, &str); 6] = [
    (MS_NOSUID, ",nosuid"),
    (MS_NODEV, ",nodev"),
    (MS_NOEXEC, ",noexec"),
    (MS_NOATIME, ",noatime"),
    (MS_RELATIME, ",relatime"),
    (MS_LAZYTIME, ",lazytime"),
];

lazy_static! {
    static ref MOUNT_CREATORS: Spin<BTreeMap<String, Arc<dyn MountCreator>>> =
        Spin::new(BTreeMap::new());
    static ref MOUNTS: Spin<Vec<(Arc<Dentry>, MountPointData)>> =
        Spin::new(vec![]);
}

static mut ROOTFS: Option<Arc<Dentry>> = None;

pub struct Mount {
    vfs: Arc<dyn Vfs>,
    root: Arc<Dentry>,
}

impl Mount {
    pub fn new(
        mp: &Dentry,
        vfs: Arc<dyn Vfs>,
        root_inode: Arc<dyn Inode>,
    ) -> KResult<Self> {
        let root_dentry = Dentry::create(mp.parent().clone(), mp.name());
        root_dentry.save_dir(root_inode)?;

        Ok(Self {
            vfs,
            root: root_dentry,
        })
    }

    pub fn root(&self) -> &Arc<Dentry> {
        &self.root
    }
}

unsafe impl Send for Mount {}
unsafe impl Sync for Mount {}

pub trait MountCreator: Send + Sync {
    fn create_mount(
        &self,
        source: &str,
        flags: u64,
        data: &[u8],
        mp: &Arc<Dentry>,
    ) -> KResult<Mount>;
}

pub fn register_filesystem(
    fstype: &str,
    creator: Arc<dyn MountCreator>,
) -> KResult<()> {
    let mut creators = MOUNT_CREATORS.lock();
    match creators.entry(String::from(fstype)) {
        Entry::Occupied(_) => Err(EEXIST),
        Entry::Vacant(entry) => {
            entry.insert(creator);
            Ok(())
        }
    }
}

struct MountPointData {
    mount: Mount,
    source: String,
    mountpoint: String,
    fstype: String,
    flags: u64,
}

pub fn do_mount(
    mountpoint: &Arc<Dentry>,
    source: &str,
    mountpoint_str: &str,
    fstype: &str,
    flags: u64,
    data: &[u8],
) -> KResult<()> {
    let mut flags = flags;
    if flags & MS_NOATIME == 0 {
        flags |= MS_RELATIME;
    }

    if flags & MS_STRICTATIME != 0 {
        flags &= !(MS_RELATIME | MS_NOATIME);
    }

    if !mountpoint.is_directory() {
        return Err(ENOTDIR);
    }

    let creator = {
        let creators = { MOUNT_CREATORS.lock() };
        creators.get(fstype).ok_or(ENODEV)?.clone()
    };
    let mount = creator.create_mount(source, flags, data, mountpoint)?;

    let root_dentry = mount.root().clone();

    let mpdata = MountPointData {
        mount,
        source: String::from(source),
        mountpoint: String::from(mountpoint_str),
        fstype: String::from(fstype),
        flags,
    };

    {
        let mut mounts = MOUNTS.lock();
        dcache::d_replace(mountpoint, root_dentry);
        mounts.push((mountpoint.clone(), mpdata));
    }

    Ok(())
}

fn mount_opts(flags: u64) -> String {
    let mut out = String::new();
    if flags & MS_RDONLY != 0 {
        out += "ro";
    } else {
        out += "rw";
    }

    for (flag, name) in MOUNT_FLAGS {
        if flags & flag != 0 {
            out += name;
        }
    }

    out
}

pub fn dump_mounts(buffer: &mut dyn core::fmt::Write) {
    for (_, mpdata) in MOUNTS.lock().iter() {
        dont_check!(writeln!(
            buffer,
            "{} {} {} {} 0 0",
            mpdata.source,
            mpdata.mountpoint,
            mpdata.fstype,
            mount_opts(mpdata.flags)
        ))
    }
}

pub fn create_rootfs() {
    let source = String::from("rootfs");
    let fstype = String::from("tmpfs");
    let flags = MS_NOATIME;

    let mount = {
        let creators = MOUNT_CREATORS.lock();
        let creator = creators.get(&fstype).ok_or(ENODEV).unwrap();

        creator
            .create_mount(&source, flags, &[], dcache::_looped_droot())
            .unwrap()
    };

    let root_dentry = mount.root().clone();
    dcache::d_add(&root_dentry);

    unsafe { ROOTFS = Some(root_dentry) };

    let mpdata = MountPointData {
        mount,
        source,
        mountpoint: String::from("/"),
        fstype,
        flags,
    };

    MOUNTS
        .lock()
        .push((dcache::_looped_droot().clone(), mpdata));
}

#[no_mangle]
pub extern "C" fn r_get_root_dentry() -> *const Dentry {
    unsafe { ROOTFS.as_ref().cloned().map(Arc::into_raw).unwrap() }
}
