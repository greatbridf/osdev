use super::{
    file::{File, InodeFile, Pipe},
    inode::Mode,
    Spin, TerminalFile,
};
use crate::{
    kernel::vfs::file::FileType,
    kernel::{console::get_console, constants::ENXIO, vfs::dentry::Dentry, CharDevice},
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
use alloc::sync::Arc;
use intrusive_collections::{
    intrusive_adapter, rbtree::Entry, Bound, KeyAdapter, RBTree, RBTreeAtomicLink,
};
use itertools::{
    FoldWhile::{Continue, Done},
    Itertools,
};
use posix_types::open::{FDFlags, OpenFlags};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FD(u32);

impl From<u32> for FD {
    fn from(fd: u32) -> Self {
        FD(fd)
    }
}

#[derive(Clone)]
struct OpenFile {
    fd: FD,
    flags: FDFlags,
    file: File,

    link: RBTreeAtomicLink,
}

intrusive_adapter!(
    OpenFileAdapter = Box<OpenFile>: OpenFile { link: RBTreeAtomicLink }
);

impl<'a> KeyAdapter<'a> for OpenFileAdapter {
    type Key = FD;

    fn get_key(&self, value: &'a OpenFile) -> Self::Key {
        value.fd
    }
}

#[derive(Clone)]
struct FDAllocator {
    min_avail: FD,
}

struct FileArrayInner {
    files: RBTree<OpenFileAdapter>,
    fd_alloc: FDAllocator,
}

pub struct FileArray {
    inner: Spin<FileArrayInner>,
}

impl OpenFile {
    fn new(fd: FD, flags: FDFlags, file: File) -> Box<Self> {
        Box::new(Self {
            fd,
            flags,
            file,
            link: RBTreeAtomicLink::new(),
        })
    }

    pub fn close_on_exec(&self) -> bool {
        self.flags.contains(FDFlags::FD_CLOEXEC)
    }
}

impl FDAllocator {
    const fn new() -> Self {
        Self { min_avail: FD(0) }
    }

    fn reinit(&mut self) {
        self.min_avail = FD(0);
    }

    fn find_available(&mut self, from: FD, files: &RBTree<OpenFileAdapter>) -> FD {
        files
            .range(Bound::Included(&from), Bound::Unbounded)
            .fold_while(from, |current, OpenFile { fd, .. }| {
                if current == *fd {
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
    fn allocate_fd(&mut self, from: FD, files: &RBTree<OpenFileAdapter>) -> FD {
        let from = FD::max(from, self.min_avail);

        if from == self.min_avail {
            let next_min_avail = self.find_available(FD(from.0 + 1), files);
            let allocated = self.min_avail;
            self.min_avail = next_min_avail;
            allocated
        } else {
            self.find_available(from, files)
        }
    }

    fn release_fd(&mut self, fd: FD) {
        if fd < self.min_avail {
            self.min_avail = fd;
        }
    }

    fn next_fd(&mut self, files: &RBTree<OpenFileAdapter>) -> FD {
        self.allocate_fd(self.min_avail, files)
    }
}

impl FileArray {
    pub fn new() -> Arc<Self> {
        Arc::new(FileArray {
            inner: Spin::new(FileArrayInner {
                files: RBTree::new(OpenFileAdapter::new()),
                fd_alloc: FDAllocator::new(),
            }),
        })
    }

    pub fn new_shared(other: &Arc<Self>) -> Arc<Self> {
        other.clone()
    }

    pub fn new_cloned(other: &Self) -> Arc<Self> {
        Arc::new(Self {
            inner: Spin::new({
                let (new_files, new_fd_alloc) = {
                    let mut new_files = RBTree::new(OpenFileAdapter::new());
                    let other_inner = other.inner.lock();

                    for file in other_inner.files.iter() {
                        let new_file = OpenFile::new(file.fd, file.flags, file.file.dup());
                        new_files.insert(new_file);
                    }
                    (new_files, other_inner.fd_alloc.clone())
                };

                FileArrayInner {
                    files: new_files,
                    fd_alloc: new_fd_alloc,
                }
            }),
        })
    }

    /// Acquires the file array lock.
    pub fn get(&self, fd: FD) -> Option<File> {
        self.inner.lock().get(fd)
    }

    pub async fn close_all(&self) {
        let old_files = {
            let mut inner = self.inner.lock();
            inner.fd_alloc.reinit();
            inner.files.take()
        };

        for file in old_files.into_iter() {
            file.file.close().await;
        }
    }

    pub async fn close(&self, fd: FD) -> KResult<()> {
        let file = {
            let mut inner = self.inner.lock();
            let file = inner.files.find_mut(&fd).remove().ok_or(EBADF)?;
            inner.fd_alloc.release_fd(file.fd);
            file.file
        };

        file.close().await;
        Ok(())
    }

    pub async fn on_exec(&self) {
        let files_to_close = {
            let mut inner = self.inner.lock();
            let (files, fd_alloc) = inner.split_borrow();

            files.pick(|ofile| {
                if ofile.close_on_exec() {
                    fd_alloc.release_fd(ofile.fd);
                    true
                } else {
                    false
                }
            })
        };

        for open_file in files_to_close.into_iter() {
            open_file.file.close().await;
        }
    }

    pub fn dup(&self, old_fd: FD) -> KResult<FD> {
        let mut inner = self.inner.lock();
        let (files, fd_alloc) = inner.split_borrow();

        let old_file = files.get_fd(old_fd).ok_or(EBADF)?;

        let new_file_data = old_file.file.dup();
        let new_file_flags = old_file.flags;
        let new_fd = fd_alloc.next_fd(files);

        inner.do_insert(new_fd, new_file_flags, new_file_data);

        Ok(new_fd)
    }

    /// Duplicates the file to a new file descriptor, returning the old file
    /// description to be dropped.
    fn dup_to_no_close(&self, old_fd: FD, new_fd: FD, fd_flags: FDFlags) -> KResult<Option<File>> {
        let mut inner = self.inner.lock();
        let (files, fd_alloc) = inner.split_borrow();

        let old_file = files.get_fd(old_fd).ok_or(EBADF)?;
        let new_file_data = old_file.file.dup();

        match files.entry(&new_fd) {
            Entry::Vacant(_) => {
                assert_eq!(new_fd, fd_alloc.allocate_fd(new_fd, files));
                inner.do_insert(new_fd, fd_flags, new_file_data);

                Ok(None)
            }
            Entry::Occupied(mut entry) => {
                let mut file = entry.remove().unwrap();
                file.flags = fd_flags;
                let old_file = core::mem::replace(&mut file.file, new_file_data);

                entry.insert(file);

                Ok(Some(old_file))
            }
        }
    }

    pub async fn dup_to(&self, old_fd: FD, new_fd: FD, flags: OpenFlags) -> KResult<FD> {
        if let Some(old_file) = self.dup_to_no_close(old_fd, new_fd, flags.as_fd_flags())? {
            old_file.close().await;
        }

        Ok(new_fd)
    }

    /// # Return
    /// `(read_fd, write_fd)`
    pub fn pipe(&self, flags: OpenFlags) -> KResult<(FD, FD)> {
        let mut inner = self.inner.lock();
        let (files, fd_alloc) = inner.split_borrow();

        let read_fd = fd_alloc.next_fd(files);
        let write_fd = fd_alloc.next_fd(files);

        let fdflag = flags.as_fd_flags();

        let (read_end, write_end) = Pipe::new(flags);
        inner.do_insert(read_fd, fdflag, read_end);
        inner.do_insert(write_fd, fdflag, write_end);

        Ok((read_fd, write_fd))
    }

    pub fn socket(&self, socket: Arc<dyn Socket>) -> KResult<FD> {
        let mut inner = self.inner.lock();
        let (files, fd_alloc) = inner.split_borrow();

        let sockfd = fd_alloc.next_fd(files);
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
        let file_format = inode.mode.load().format();

        match (flags.directory(), file_format, flags.write()) {
            (true, Mode::DIR, _) => {}
            (true, _, _) => return Err(ENOTDIR),
            (false, Mode::DIR, true) => return Err(EISDIR),
            _ => {}
        }

        if flags.truncate() && flags.write() && file_format.is_reg() {
            inode.truncate(0)?;
        }

        let file = if file_format.is_chr() {
            let device = CharDevice::get(inode.devid()?).ok_or(ENXIO)?;
            device.open(flags)?
        } else {
            InodeFile::new(dentry.clone(), flags)
        };

        let mut inner = self.inner.lock();
        let (files, fd_alloc) = inner.split_borrow();
        let fd = fd_alloc.next_fd(files);
        inner.do_insert(fd, fdflag, file);

        Ok(fd)
    }

    pub fn fcntl(&self, fd: FD, cmd: u32, arg: usize) -> KResult<usize> {
        let mut inner = self.inner.lock();
        let (files, fd_alloc) = inner.split_borrow();

        let mut cursor = files.find_mut(&fd);

        let ret = match cmd {
            F_DUPFD | F_DUPFD_CLOEXEC => {
                let ofile = cursor.get().ok_or(EBADF)?;

                let cloexec = cmd == F_DUPFD_CLOEXEC || ofile.flags.close_on_exec();
                let flags = cloexec
                    .then_some(FDFlags::FD_CLOEXEC)
                    .unwrap_or(FDFlags::empty());

                let new_file_data = ofile.file.dup();
                let new_fd = fd_alloc.allocate_fd(FD(arg as u32), files);

                inner.do_insert(new_fd, flags, new_file_data);

                new_fd.0 as usize
            }
            F_GETFD => cursor.get().ok_or(EBADF)?.flags.bits() as usize,
            F_SETFD => {
                let mut ofile = cursor.remove().ok_or(EBADF)?;
                ofile.flags = FDFlags::from_bits_truncate(arg as u32);
                cursor.insert(ofile);
                0
            }
            F_GETFL => cursor.get().ok_or(EBADF)?.file.get_flags().bits() as usize,
            F_SETFL => {
                cursor
                    .get()
                    .ok_or(EBADF)?
                    .file
                    .set_flags(OpenFlags::from_bits_retain(arg as u32));

                0
            }
            _ => unimplemented!("fcntl: cmd={}", cmd),
        };

        Ok(ret)
    }

    /// Only used for init process.
    pub fn open_console(&self) {
        let mut inner = self.inner.lock();
        let (files, fd_alloc) = inner.split_borrow();

        let (stdin, stdout, stderr) = (
            fd_alloc.next_fd(files),
            fd_alloc.next_fd(files),
            fd_alloc.next_fd(files),
        );
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
    fn get(&mut self, fd: FD) -> Option<File> {
        self.files.get_fd(fd).map(|open| open.file.clone())
    }

    /// Insert a file description to the file array.
    fn do_insert(&mut self, fd: FD, flags: FDFlags, file: File) {
        match self.files.entry(&fd) {
            Entry::Occupied(_) => {
                panic!("File descriptor {fd:?} already exists in the file array.");
            }
            Entry::Vacant(insert_cursor) => {
                insert_cursor.insert(OpenFile::new(fd, flags, file));
            }
        }
    }

    fn split_borrow(&mut self) -> (&mut RBTree<OpenFileAdapter>, &mut FDAllocator) {
        let Self { files, fd_alloc } = self;
        (files, fd_alloc)
    }
}

impl FD {
    pub const AT_FDCWD: FD = FD(-100i32 as u32);
}

impl core::fmt::Debug for FD {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            &Self::AT_FDCWD => f.write_str("FD(AT_FDCWD)"),
            FD(no) => f.debug_tuple("FD").field(&no).finish(),
        }
    }
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

trait FilesExt {
    fn get_fd(&self, fd: FD) -> Option<&OpenFile>;

    fn pick<P>(&mut self, pred: P) -> Self
    where
        P: FnMut(&OpenFile) -> bool;
}

impl FilesExt for RBTree<OpenFileAdapter> {
    fn get_fd(&self, fd: FD) -> Option<&OpenFile> {
        self.find(&fd).get()
    }

    fn pick<P>(&mut self, mut pred: P) -> Self
    where
        P: FnMut(&OpenFile) -> bool,
    {
        let mut picked = RBTree::new(OpenFileAdapter::new());

        // TODO: might be better if we start picking from somewhere else
        //       or using a different approach.
        let mut cursor = self.front_mut();
        while let Some(open_file) = cursor.get() {
            if !pred(open_file) {
                cursor.move_next();
                continue;
            }

            picked.insert(cursor.remove().unwrap());
            cursor.move_next();
        }

        picked
    }
}
