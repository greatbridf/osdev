use alloc::sync::Arc;

use eonix_sync::AsProof;
use posix_types::open::OpenFlags;

use super::{File, FileType, PollEvent};
use crate::io::{Buffer, Stream, StreamRead};
use crate::kernel::constants::{
    EINVAL, TCGETS, TCSETS, TIOCGPGRP, TIOCGWINSZ, TIOCSPGRP,
};
use crate::kernel::task::{ProcessList, Thread};
use crate::kernel::terminal::TerminalIORequest;
use crate::kernel::user::{UserPointer, UserPointerMut};
use crate::kernel::Terminal;
use crate::prelude::KResult;

pub struct TerminalFile {
    terminal: Arc<Terminal>,
}

impl TerminalFile {
    pub async fn open(
        thread: &Thread,
        terminal: &Arc<Terminal>,
        flags: OpenFlags,
    ) -> File {
        let set_control_tty = !flags.contains(OpenFlags::O_NOCTTY);

        let procs = ProcessList::get().read().await;
        let session = thread.process.session(procs.prove());

        // We only set the control terminal if the process is the session leader.
        if set_control_tty && session.sid == thread.process.pid {
            // Silently fail if we can't set the control terminal.
            let _ = terminal.set_session(&session, false, procs.prove()).await;
        }

        File::new(
            flags,
            FileType::Terminal(TerminalFile {
                terminal: terminal.clone(),
            }),
        )
    }

    pub async fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        self.terminal.read(buffer).await
    }

    pub fn write(&self, stream: &mut dyn Stream) -> KResult<usize> {
        stream.read_till_end(&mut [0; 128], |data| {
            self.terminal.write(data);
            Ok(())
        })
    }

    pub async fn poll(&self, event: PollEvent) -> KResult<PollEvent> {
        if !event.contains(PollEvent::Readable) {
            unimplemented!("Poll event not supported.")
        }

        self.terminal.poll_in().await.map(|_| PollEvent::Readable)
    }

    pub async fn ioctl(&self, request: usize, arg3: usize) -> KResult<()> {
        self.terminal
            .ioctl(match request as u32 {
                TCGETS => TerminalIORequest::GetTermios(
                    UserPointerMut::with_addr(arg3)?,
                ),
                TCSETS => {
                    TerminalIORequest::SetTermios(UserPointer::with_addr(arg3)?)
                }
                TIOCGPGRP => TerminalIORequest::GetProcessGroup(
                    UserPointerMut::with_addr(arg3)?,
                ),
                TIOCSPGRP => TerminalIORequest::SetProcessGroup(
                    UserPointer::with_addr(arg3)?,
                ),
                TIOCGWINSZ => TerminalIORequest::GetWindowSize(
                    UserPointerMut::with_addr(arg3)?,
                ),
                _ => return Err(EINVAL),
            })
            .await
    }
}
