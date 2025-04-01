use core::sync::atomic::Ordering;

use crate::{
    kernel::{
        console::CONSOLE,
        constants::ENXIO,
        task::Thread,
        vfs::{dentry::Dentry, file::Pipe, s_isdir, s_isreg},
        CharDevice,
    },
    path::Path,
    prelude::*,
};

use alloc::{
    collections::btree_map::{BTreeMap, Entry},
    sync::Arc,
};
use bindings::{
    EBADF, EISDIR, ENOTDIR, FD_CLOEXEC, F_DUPFD, F_DUPFD_CLOEXEC, F_GETFD, F_SETFD, O_APPEND,
    O_CLOEXEC, O_DIRECTORY, O_RDWR, O_TRUNC, O_WRONLY,
};
use itertools::{
    FoldWhile::{Continue, Done},
    Itertools,
};

use super::{
    file::{File, InodeFile, TerminalFile},
    inode::Mode,
    s_ischr, FsContext, Spin,
};

type FD = u32;

#[derive(Clone)]
struct OpenFile {
    /// File descriptor flags, only for `FD_CLOEXEC`.
    flags: u64,
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
        self.flags & O_CLOEXEC as u64 != 0
    }
}

impl FileArray {
    pub fn get_current<'lt>() -> &'lt Arc<Self> {
        &Thread::current().borrow().files
    }

    pub fn new() -> Arc<Self> {
        Arc::new(FileArray {
            inner: Spin::new(FileArrayInner {
                files: BTreeMap::new(),
                fd_min_avail: 0,
            }),
        })
    }

    #[allow(dead_code)]
    pub fn new_shared(other: &Arc<Self>) -> Arc<Self> {
        other.clone()
    }

    pub fn new_cloned(other: &Self) -> Arc<Self> {
        Arc::new(Self {
            inner: Spin::clone(&other.inner),
        })
    }

    /// Acquires the file array lock.
    pub fn get(&self, fd: FD) -> Option<Arc<File>> {
        self.inner.lock().get(fd)
    }

    pub fn close_all(&self) {
        let mut inner = self.inner.lock();
        inner.fd_min_avail = 0;
        inner.files.clear();
    }

    pub fn close(&self, fd: FD) -> KResult<()> {
        let mut inner = self.inner.lock();
        inner.files.remove(&fd).ok_or(EBADF)?;
        inner.release_fd(fd);
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

    pub fn dup_to(&self, old_fd: FD, new_fd: FD, flags: u64) -> KResult<FD> {
        let mut inner = self.inner.lock();
        let old_file = inner.files.get(&old_fd).ok_or(EBADF)?;

        let new_file_data = old_file.file.clone();

        match inner.files.entry(new_fd) {
            Entry::Vacant(_) => {}
            Entry::Occupied(entry) => {
                let new_file = entry.into_mut();

                new_file.flags = flags;
                new_file.file = new_file_data;

                return Ok(new_fd);
            }
        }

        assert_eq!(new_fd, inner.allocate_fd(new_fd));
        inner.do_insert(new_fd, flags, new_file_data);

        Ok(new_fd)
    }

    /// # Return
    /// `(read_fd, write_fd)`
    pub fn pipe(&self, flags: u32) -> KResult<(FD, FD)> {
        let mut inner = self.inner.lock();

        let read_fd = inner.next_fd();
        let write_fd = inner.next_fd();

        let fdflag = if flags & O_CLOEXEC != 0 { FD_CLOEXEC } else { 0 };

        let pipe = Pipe::new();
        let (read_end, write_end) = pipe.split();
        inner.do_insert(read_fd, fdflag as u64, read_end);
        inner.do_insert(write_fd, fdflag as u64, write_end);

        Ok((read_fd, write_fd))
    }

    pub fn open(&self, fs_context: &FsContext, path: Path, flags: u32, mode: Mode) -> KResult<FD> {
        let dentry = Dentry::open(fs_context, path, true)?;
        dentry.open_check(flags, mode)?;

        let fdflag = if flags & O_CLOEXEC != 0 { FD_CLOEXEC } else { 0 };
        let can_read = flags & O_WRONLY == 0;
        let can_write = flags & (O_WRONLY | O_RDWR) != 0;
        let append = flags & O_APPEND != 0;

        let inode = dentry.get_inode()?;
        let filemode = inode.mode.load(Ordering::Relaxed);

        if flags & O_DIRECTORY != 0 {
            if !s_isdir(filemode) {
                return Err(ENOTDIR);
            }
        } else {
            if s_isdir(filemode) && can_write {
                return Err(EISDIR);
            }
        }

        if flags & O_TRUNC != 0 {
            if can_write && s_isreg(filemode) {
                inode.truncate(0)?;
            }
        }

        let mut inner = self.inner.lock();
        let fd = inner.next_fd();

        if s_ischr(filemode) {
            let device = CharDevice::get(inode.devid()?).ok_or(ENXIO)?;
            let file = device.open()?;
            inner.do_insert(fd, fdflag as u64, file);
        } else {
            inner.do_insert(
                fd,
                fdflag as u64,
                InodeFile::new(dentry, (can_read, can_write, append)),
            );
        }

        Ok(fd)
    }

    pub fn fcntl(&self, fd: FD, cmd: u32, arg: usize) -> KResult<usize> {
        let mut inner = self.inner.lock();
        let ofile = inner.files.get_mut(&fd).ok_or(EBADF)?;

        match cmd {
            F_DUPFD | F_DUPFD_CLOEXEC => {
                let cloexec = cmd == F_DUPFD_CLOEXEC || (ofile.flags & FD_CLOEXEC as u64 != 0);
                let flags = if cloexec { O_CLOEXEC } else { 0 };

                let new_file_data = ofile.file.clone();
                let new_fd = inner.allocate_fd(arg as FD);

                inner.do_insert(new_fd, flags as u64, new_file_data);

                Ok(new_fd as usize)
            }
            F_GETFD => Ok(ofile.flags as usize),
            F_SETFD => {
                ofile.flags = arg as u64;
                Ok(0)
            }
            _ => unimplemented!("fcntl: cmd={}", cmd),
        }
    }

    /// Only used for init process.
    pub fn open_console(&self) {
        let mut inner = self.inner.lock();
        let (stdin, stdout, stderr) = (inner.next_fd(), inner.next_fd(), inner.next_fd());
        let console_terminal = CONSOLE.lock_irq().get_terminal().unwrap();

        inner.do_insert(
            stdin,
            O_CLOEXEC as u64,
            TerminalFile::new(console_terminal.clone()),
        );
        inner.do_insert(
            stdout,
            O_CLOEXEC as u64,
            TerminalFile::new(console_terminal.clone()),
        );
        inner.do_insert(
            stderr,
            O_CLOEXEC as u64,
            TerminalFile::new(console_terminal.clone()),
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
                    Continue(current + 1)
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
            let next_min_avail = self.find_available(from + 1);
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
    fn do_insert(&mut self, fd: FD, flags: u64, file: Arc<File>) {
        assert!(self.files.insert(fd, OpenFile { flags, file }).is_none());
    }
}
