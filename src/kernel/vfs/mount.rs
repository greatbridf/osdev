use alloc::{
    collections::btree_map::{BTreeMap, Entry},
    sync::Arc,
};
use bindings::{
    fs::{D_DIRECTORY, D_MOUNTPOINT, D_PRESENT},
    EEXIST, ENODEV, ENOENT, ENOTDIR,
};

use crate::{io::operator_eql_cxx_std_string, prelude::*};

use super::{
    dentry::{Dentry, DentryCache, DentryInner},
    inode::Inode,
    vfs::Vfs,
    Mutex,
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

static MOUNT_CREATORS: Mutex<BTreeMap<String, Box<dyn MountCreator>>> =
    Mutex::new(BTreeMap::new());

static MOUNTS: Mutex<BTreeMap<Dentry, MountPointData>> =
    Mutex::new(BTreeMap::new());

static mut ROOT_DENTRY: Option<Dentry> = None;

pub struct Mount {
    dcache: Mutex<DentryCache>,
    vfs: Arc<Mutex<dyn Vfs>>,
    root: Dentry,
}

impl Mount {
    pub fn new(vfs: Arc<Mutex<dyn Vfs>>, root_inode: Arc<dyn Inode>) -> Self {
        let mut dcache = DentryCache::new();

        // register root dentry
        let mut dent = dcache.alloc();

        dent.save_inode(root_inode);
        dent.flags = D_DIRECTORY | D_PRESENT;

        dcache.insert_root(&mut dent);

        Self {
            dcache: Mutex::new(dcache),
            vfs,
            root: dent,
        }
    }

    pub fn root(&self) -> Dentry {
        self.root.clone()
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
    ) -> KResult<Mount>;
}

pub fn register_filesystem(
    fstype: &str,
    creator: Box<dyn MountCreator>,
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
    mut mountpoint: Dentry,
    source: &str,
    mountpoint_str: &str,
    fstype: &str,
    flags: u64,
    data: &[u8],
) -> KResult<()> {
    if mountpoint.flags & D_DIRECTORY == 0 {
        return Err(ENOTDIR);
    }

    let mut flags = flags;
    if flags & MS_NOATIME == 0 {
        flags |= MS_RELATIME;
    }

    if flags & MS_STRICTATIME != 0 {
        flags &= !(MS_RELATIME | MS_NOATIME);
    }

    let mount = {
        let creators = { MOUNT_CREATORS.lock() };
        let creator = creators.get(fstype).ok_or(ENODEV)?;
        creator.create_mount(source, flags, data)?
    };

    let mut root = mount.root();

    root.parent = mountpoint.parent;
    operator_eql_cxx_std_string(&mut root.name, &mountpoint.name);
    root.hash = mountpoint.hash;

    let mpdata = MountPointData {
        mount,
        source: String::from(source),
        mountpoint: String::from(mountpoint_str),
        fstype: String::from(fstype),
        flags,
    };

    {
        let mut mounts = MOUNTS.lock();
        mountpoint.flags |= D_MOUNTPOINT;
        mounts.insert(mountpoint.clone(), mpdata);
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

pub fn get_mountpoint(mnt: &Dentry) -> KResult<Dentry> {
    let mounts = MOUNTS.lock();
    let mpdata = mounts.get(mnt).ok_or(ENOENT)?;

    Ok(mpdata.mount.root().clone())
}

#[no_mangle]
pub extern "C" fn r_get_mountpoint(mnt: *mut DentryInner) -> *mut DentryInner {
    let mnt = Dentry::from_raw(mnt).unwrap();

    match get_mountpoint(&mnt) {
        Ok(mnt) => mnt.leak(),
        Err(_) => core::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn r_get_root_dentry() -> *mut DentryInner {
    let root = unsafe { ROOT_DENTRY.as_ref().unwrap() };

    root.clone().leak()
}

pub fn create_rootfs() {
    let source = String::from("none");
    let fstype = String::from("tmpfs");
    let flags = MS_NOATIME;

    let mount = {
        let creators = MOUNT_CREATORS.lock();
        let creator = creators.get(&fstype).ok_or(ENODEV).unwrap();
        creator.create_mount(&source, flags, &[]).unwrap()
    };

    let root = mount.root();
    unsafe { ROOT_DENTRY = Some(root.clone()) };

    let mpdata = MountPointData {
        mount,
        source,
        mountpoint: String::from("/"),
        fstype,
        flags,
    };

    {
        let mut mounts = MOUNTS.lock();
        mounts.insert(root, mpdata);
    }
}
