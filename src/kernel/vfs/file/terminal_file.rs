use super::{File, FileType, PollEvent};
use crate::{
    io::{Buffer, Stream, StreamRead},
    kernel::{
        constants::{EINVAL, TCGETS, TCSETS, TIOCGPGRP, TIOCGWINSZ, TIOCSPGRP},
        terminal::TerminalIORequest,
        user::{UserPointer, UserPointerMut},
        Terminal,
    },
    prelude::KResult,
};
use alloc::sync::Arc;
use posix_types::open::OpenFlags;

pub struct TerminalFile {
    terminal: Arc<Terminal>,
}

impl TerminalFile {
    pub fn new(tty: Arc<Terminal>, flags: OpenFlags) -> File {
        File::new(flags, FileType::Terminal(TerminalFile { terminal: tty }))
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
                TCGETS => TerminalIORequest::GetTermios(UserPointerMut::with_addr(arg3)?),
                TCSETS => TerminalIORequest::SetTermios(UserPointer::with_addr(arg3)?),
                TIOCGPGRP => TerminalIORequest::GetProcessGroup(UserPointerMut::with_addr(arg3)?),
                TIOCSPGRP => TerminalIORequest::SetProcessGroup(UserPointer::with_addr(arg3)?),
                TIOCGWINSZ => TerminalIORequest::GetWindowSize(UserPointerMut::with_addr(arg3)?),
                _ => return Err(EINVAL),
            })
            .await
    }
}
