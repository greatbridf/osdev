use super::{
    file::{File, InodeFile, TerminalFile},
    inode::Mode,
    s_ischr, Spin,
};
use crate::{
    kernel::{
        console::get_console,
        constants::ENXIO,
        vfs::{
            dentry::Dentry,
            file::{FileType, Pipe},
            s_isdir, s_isreg,
        },
        CharDevice,
    },
    prelude::*,
};
use crate::{
    kernel::{
        constants::{
            EBADF, EISDIR, ENOTDIR, F_DUPFD, F_DUPFD_CLOEXEC, F_GETFD, F_GETFL, F_SETFD, F_SETFL,
        },
        syscall::{FromSyscallArg, SyscallRetVal},
    },
    net::socket::Socket,
};
use alloc::{
    collections::btree_map::{BTreeMap, Entry},
    sync::Arc,
};
use core::sync::atomic::{AtomicU32, Ordering};
use itertools::{
    FoldWhile::{Continue, Done},
    Itertools,
};
use posix_types::open::{FDFlags, OpenFlags};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FD(u32);

impl From<u32> for FD {
    fn from(fd: u32) -> Self {
        FD(fd)
    }
}

#[derive(Clone)]
struct OpenFile {
    flags: FDFlags,
    file: Arc<File>,
}

#[derive(Clone)]
struct FileArrayInner {
    files: BTreeMap<FD, OpenFile>,
    fd_min_avail: FD,
}

pub struct FileArray {
    inner: Spin<FileArrayInner>,
}

impl OpenFile {
    pub fn close_on_exec(&self) -> bool {
        self.flags.contains(FDFlags::FD_CLOEXEC)
    }
}

impl FileArray {
    pub fn new() -> Arc<Self> {
        Arc::new(FileArray {
            inner: Spin::new(FileArrayInner {
                files: BTreeMap::new(),
                fd_min_avail: FD(0),
            }),
        })
    }

    #[allow(dead_code)]
    pub fn new_shared(other: &Arc<Self>) -> Arc<Self> {
        other.clone()
    }

    pub fn new_cloned(other: &Self) -> Arc<Self> {
        Arc::new(Self {
            inner: Spin::new(other.inner.lock().clone()),
        })
    }

    /// Acquires the file array lock.
    pub fn get(&self, fd: FD) -> Option<Arc<File>> {
        self.inner.lock().get(fd)
    }

    pub fn close_all(&self) {
        let _old_files = {
            let mut inner = self.inner.lock();
            inner.fd_min_avail = FD(0);
            core::mem::take(&mut inner.files)
        };
    }

    pub fn close(&self, fd: FD) -> KResult<()> {
        let _file = {
            let mut inner = self.inner.lock();
            let file = inner.files.remove(&fd).ok_or(EBADF)?;
            inner.release_fd(fd);
            file
        };
        Ok(())
    }

    pub fn on_exec(&self) -> () {
        let mut inner = self.inner.lock();

        // TODO: This is not efficient. We should avoid cloning.
        let fds_to_close = inner
            .files
            .iter()
            .filter(|(_, ofile)| ofile.close_on_exec())
            .map(|(&fd, _)| fd)
            .collect::<Vec<_>>();

        inner.files.retain(|_, ofile| !ofile.close_on_exec());
        fds_to_close.into_iter().for_each(|fd| inner.release_fd(fd));
    }
}

impl FileArray {
    pub fn dup(&self, old_fd: FD) -> KResult<FD> {
        let mut inner = self.inner.lock();
        let old_file = inner.files.get(&old_fd).ok_or(EBADF)?;

        let new_file_data = old_file.file.clone();
        let new_file_flags = old_file.flags;
        let new_fd = inner.next_fd();

        inner.do_insert(new_fd, new_file_flags, new_file_data);

        Ok(new_fd)
    }

    pub fn dup_to(&self, old_fd: FD, new_fd: FD, flags: OpenFlags) -> KResult<FD> {
        let fdflags = flags.as_fd_flags();

        let mut inner = self.inner.lock();
        let old_file = inner.files.get(&old_fd).ok_or(EBADF)?;

        let new_file_data = old_file.file.clone();

        match inner.files.entry(new_fd) {
            Entry::Vacant(_) => {}
            Entry::Occupied(entry) => {
                let new_file = entry.into_mut();
                let mut file_swap = new_file_data;

                new_file.flags = fdflags;
                core::mem::swap(&mut file_swap, &mut new_file.file);

                drop(inner);
                return Ok(new_fd);
            }
        }

        assert_eq!(new_fd, inner.allocate_fd(new_fd));
        inner.do_insert(new_fd, fdflags, new_file_data);

        Ok(new_fd)
    }

    /// # Return
    /// `(read_fd, write_fd)`
    pub fn pipe(&self, flags: OpenFlags) -> KResult<(FD, FD)> {
        let mut inner = self.inner.lock();

        let read_fd = inner.next_fd();
        let write_fd = inner.next_fd();

        let fdflag = flags.as_fd_flags();

        let (read_end, write_end) = Pipe::new(flags);
        inner.do_insert(read_fd, fdflag, read_end);
        inner.do_insert(write_fd, fdflag, write_end);

        Ok((read_fd, write_fd))
    }

    pub fn socket(&self, socket: Arc<dyn Socket>) -> KResult<FD> {
        let mut inner = self.inner.lock();
        let sockfd = inner.next_fd();
        inner.do_insert(
            sockfd,
            FDFlags::default(),
            File::new(OpenFlags::default(), FileType::Socket(socket)),
        );
        Ok(sockfd)
    }

    pub fn open(&self, dentry: &Arc<Dentry>, flags: OpenFlags, mode: Mode) -> KResult<FD> {
        dentry.open_check(flags, mode)?;

        let fdflag = flags.as_fd_flags();

        let inode = dentry.get_inode()?;
        let filemode = inode.mode.load(Ordering::Relaxed);

        if flags.directory() {
            if !s_isdir(filemode) {
                return Err(ENOTDIR);
            }
        } else {
            if s_isdir(filemode) && flags.write() {
                return Err(EISDIR);
            }
        }

        if flags.truncate() && flags.write() && s_isreg(filemode) {
            inode.truncate(0)?;
        }

        let file = if s_ischr(filemode) {
            let device = CharDevice::get(inode.devid()?).ok_or(ENXIO)?;
            device.open(flags)?
        } else {
            InodeFile::new(dentry.clone(), flags)
        };

        let mut inner = self.inner.lock();
        let fd = inner.next_fd();
        inner.do_insert(fd, fdflag, file);

        Ok(fd)
    }

    pub fn fcntl(&self, fd: FD, cmd: u32, arg: usize) -> KResult<usize> {
        let mut inner = self.inner.lock();
        let ofile = inner.files.get_mut(&fd).ok_or(EBADF)?;

        match cmd {
            F_DUPFD | F_DUPFD_CLOEXEC => {
                let cloexec = cmd == F_DUPFD_CLOEXEC || ofile.flags.close_on_exec();
                let flags = cloexec
                    .then_some(FDFlags::FD_CLOEXEC)
                    .unwrap_or(FDFlags::empty());

                let new_file_data = ofile.file.clone();
                let new_fd = inner.allocate_fd(FD(arg as u32));

                inner.do_insert(new_fd, flags, new_file_data);

                Ok(new_fd.0 as usize)
            }
            F_GETFD => Ok(ofile.flags.bits() as usize),
            F_SETFD => {
                ofile.flags = FDFlags::from_bits_truncate(arg as u32);
                Ok(0)
            }
            F_GETFL => Ok(ofile.file.get_flags().bits() as usize),
            F_SETFL => {
                ofile
                    .file
                    .set_flags(OpenFlags::from_bits_retain(arg as u32));

                Ok(0)
            }
            _ => unimplemented!("fcntl: cmd={}", cmd),
        }
    }

    /// Only used for init process.
    pub fn open_console(&self) {
        let mut inner = self.inner.lock();
        let (stdin, stdout, stderr) = (inner.next_fd(), inner.next_fd(), inner.next_fd());
        let console_terminal = get_console().expect("No console terminal");

        inner.do_insert(
            stdin,
            FDFlags::FD_CLOEXEC,
            TerminalFile::new(console_terminal.clone(), OpenFlags::empty()),
        );
        inner.do_insert(
            stdout,
            FDFlags::FD_CLOEXEC,
            TerminalFile::new(console_terminal.clone(), OpenFlags::empty()),
        );
        inner.do_insert(
            stderr,
            FDFlags::FD_CLOEXEC,
            TerminalFile::new(console_terminal.clone(), OpenFlags::empty()),
        );
    }
}

impl FileArrayInner {
    fn get(&mut self, fd: FD) -> Option<Arc<File>> {
        self.files.get(&fd).map(|f| f.file.clone())
    }

    fn find_available(&mut self, from: FD) -> FD {
        self.files
            .range(&from..)
            .fold_while(from, |current, (&key, _)| {
                if current == key {
                    Continue(FD(current.0 + 1))
                } else {
                    Done(current)
                }
            })
            .into_inner()
    }

    /// Allocate a new file descriptor starting from `from`.
    ///
    /// Returned file descriptor should be used immediately.
    ///
    fn allocate_fd(&mut self, from: FD) -> FD {
        let from = FD::max(from, self.fd_min_avail);

        if from == self.fd_min_avail {
            let next_min_avail = self.find_available(FD(from.0 + 1));
            let allocated = self.fd_min_avail;
            self.fd_min_avail = next_min_avail;
            allocated
        } else {
            self.find_available(from)
        }
    }

    fn release_fd(&mut self, fd: FD) {
        if fd < self.fd_min_avail {
            self.fd_min_avail = fd;
        }
    }

    fn next_fd(&mut self) -> FD {
        self.allocate_fd(self.fd_min_avail)
    }

    /// Insert a file description to the file array.
    fn do_insert(&mut self, fd: FD, flags: FDFlags, file: Arc<File>) {
        assert!(self.files.insert(fd, OpenFile { flags, file }).is_none());
    }
}

impl FD {
    pub const AT_FDCWD: FD = FD(-100i32 as u32);
}

impl FromSyscallArg for FD {
    fn from_arg(value: usize) -> Self {
        Self(value as u32)
    }
}

impl SyscallRetVal for FD {
    fn into_retval(self) -> Option<usize> {
        Some(self.0 as usize)
    }
}
