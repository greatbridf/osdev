use super::{
    dentry::{dcache, Dentry, DROOT},
    inode::Inode,
    vfs::Vfs,
};
use crate::kernel::constants::{EEXIST, ENODEV, ENOTDIR};
use crate::prelude::*;
use alloc::{collections::btree_map::BTreeMap, string::ToString as _, sync::Arc};
use eonix_sync::LazyLock;

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

static MOUNT_CREATORS: Spin<BTreeMap<String, Arc<dyn MountCreator>>> = Spin::new(BTreeMap::new());
static MOUNTS: Spin<Vec<(Arc<Dentry>, MountPointData)>> = Spin::new(vec![]);

pub struct Mount {
    _vfs: Arc<dyn Vfs>,
    root: Arc<Dentry>,
}

impl Mount {
    pub fn new(mp: &Dentry, vfs: Arc<dyn Vfs>, root_inode: Arc<dyn Inode>) -> KResult<Self> {
        let root_dentry = Dentry::create(mp.parent().clone(), mp.name());
        root_dentry.save_dir(root_inode)?;

        Ok(Self {
            _vfs: vfs,
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
    fn create_mount(&self, source: &str, flags: u64, mp: &Arc<Dentry>) -> KResult<Mount>;
}

pub fn register_filesystem(fstype: &str, creator: Arc<dyn MountCreator>) -> KResult<()> {
    let mut creators = MOUNT_CREATORS.lock();
    if !creators.contains_key(fstype) {
        creators.insert(fstype.to_string(), creator);
        Ok(())
    } else {
        Err(EEXIST)
    }
}

#[allow(dead_code)]
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
    let mount = creator.create_mount(source, flags, mountpoint)?;

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

impl Dentry {
    pub fn root() -> &'static Arc<Dentry> {
        static ROOT: LazyLock<Arc<Dentry>> = LazyLock::new(|| {
            let source = String::from("(rootfs)");
            let fstype = String::from("tmpfs");
            let mount_flags = MS_NOATIME;

            let creator = MOUNT_CREATORS
                .lock()
                .get(&fstype)
                .cloned()
                .expect("tmpfs not registered.");

            let mount = creator
                .create_mount(&source, mount_flags, &DROOT)
                .expect("Failed to create root mount.");

            let root_dentry = mount.root().clone();

            dcache::d_add(&root_dentry);

            MOUNTS.lock().push((
                DROOT.clone(),
                MountPointData {
                    mount,
                    source,
                    mountpoint: String::from("/"),
                    fstype,
                    flags: mount_flags,
                },
            ));

            root_dentry
        });

        &ROOT
    }
}
