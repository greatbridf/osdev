use alloc::{
    collections::vec_deque::VecDeque,
    sync::{Arc, Weak},
};
use bindings::{EINTR, ENOTTY, EPERM};
use bitflags::bitflags;

use crate::{io::Buffer, prelude::*, sync::CondVar};

use super::{
    task::{Session, Signal, Thread},
    user::{UserPointer, UserPointerMut},
};

const BUFFER_SIZE: usize = 4096;

const NCCS: usize = 32;

// taken from linux kernel code

/* c_cc characters */
const VINTR: usize = 0;
const VQUIT: usize = 1;
const VERASE: usize = 2;
const VKILL: usize = 3;
const VEOF: usize = 4;
const VTIME: usize = 5;
const VMIN: usize = 6;
const VSWTC: usize = 7;
const VSTART: usize = 8;
const VSTOP: usize = 9;
const VSUSP: usize = 10;
const VEOL: usize = 11;
const VREPRINT: usize = 12;
const VDISCARD: usize = 13;
const VWERASE: usize = 14;
const VLNEXT: usize = 15;
const VEOL2: usize = 16;

/* c_iflag bits */

bitflags! {
    #[derive(Debug)]
    pub struct TermioIFlags: u16 {
        /// Ignore break condition
        const IGNBRK = 0x0001;
        /// Signal interrupt on break
        const BRKINT = 0x0002;
        /// Ignore characters with parity errors
        const IGNPAR = 0x0004;
        /// Mark parity and framing errors
        const PARMRK = 0x0008;
        /// Enable input parity check
        const INPCK = 0x0010;
        /// Strip 8th bit off characters
        const ISTRIP = 0x0020;
        /// Map NL to CR on input
        const INLCR = 0x0040;
        /// Ignore CR
        const IGNCR = 0x0080;
        /// Map CR to NL on input
        const ICRNL = 0x0100;
        const IUCLC = 0x0200;
        const IXON = 0x0400;
        /// Any character will restart after stop
        const IXANY = 0x0800;
        const IXOFF = 0x1000;
        const IMAXBEL = 0x2000;
        const IUTF8 = 0x4000;
    }

    #[derive(Debug)]
    pub struct TermioOFlags: u16 {
        /// Perform output processing
        const OPOST = 0x0001;
        const OLCUC = 0x0002;
        const ONLCR = 0x0004;
        const OCRNL = 0x0008;
        const ONOCR = 0x0010;
        const ONLRET = 0x0020;
        const OFILL = 0x0040;
        const OFDEL = 0x0080;
    }
}

bitflags! {
    #[derive(Debug)]
    pub struct TermioLFlags: u16 {
        const ISIG = 0x0001;
        const ICANON = 0x0002;
        const XCASE = 0x0004;
        const ECHO = 0x0008;
        const ECHOE = 0x0010;
        const ECHOK = 0x0020;
        const ECHONL = 0x0040;
        const NOFLSH = 0x0080;
        const TOSTOP = 0x0100;
        const ECHOCTL = 0x0200;
        const ECHOPRT = 0x0400;
        const ECHOKE = 0x0800;
        const FLUSHO = 0x1000;
        const PENDIN = 0x4000;
        const IEXTEN = 0x8000;
    }
}

/* c_cflag bit meaning */
const B38400: u32 = 0x0000000f;
const CS8: u32 = 0x00000030;
const CREAD: u32 = 0x00000080;
const HUPCL: u32 = 0x00000400;

// line disciplines

const N_TTY: u8 = 0;

#[derive(Debug)]
pub struct Termios {
    iflag: TermioIFlags,
    oflag: TermioOFlags,
    cflag: u32,
    lflag: TermioLFlags,

    line: u8,
    cc: [u8; NCCS],
}

macro_rules! CTRL {
    ('A') => {
        0x01
    };
    ('B') => {
        0x02
    };
    ('C') => {
        0x03
    };
    ('D') => {
        0x04
    };
    ('E') => {
        0x05
    };
    ('F') => {
        0x06
    };
    ('G') => {
        0x07
    };
    ('H') => {
        0x08
    };
    ('I') => {
        0x09
    };
    ('J') => {
        0x0A
    };
    ('K') => {
        0x0B
    };
    ('L') => {
        0x0C
    };
    ('M') => {
        0x0D
    };
    ('N') => {
        0x0E
    };
    ('O') => {
        0x0F
    };
    ('P') => {
        0x10
    };
    ('Q') => {
        0x11
    };
    ('R') => {
        0x12
    };
    ('S') => {
        0x13
    };
    ('T') => {
        0x14
    };
    ('U') => {
        0x15
    };
    ('V') => {
        0x16
    };
    ('W') => {
        0x17
    };
    ('X') => {
        0x18
    };
    ('Y') => {
        0x19
    };
    ('Z') => {
        0x1A
    };
    ('\\') => {
        0x1c
    };
}

impl Termios {
    pub fn veof(&self) -> u8 {
        self.cc[VEOF]
    }

    pub fn veol(&self) -> u8 {
        self.cc[VEOL]
    }

    pub fn veol2(&self) -> u8 {
        self.cc[VEOL2]
    }

    pub fn vintr(&self) -> u8 {
        self.cc[VINTR]
    }

    pub fn vquit(&self) -> u8 {
        self.cc[VQUIT]
    }

    pub fn vsusp(&self) -> u8 {
        self.cc[VSUSP]
    }

    pub fn verase(&self) -> u8 {
        self.cc[VERASE]
    }

    pub fn vkill(&self) -> u8 {
        self.cc[VKILL]
    }

    pub fn echo(&self) -> bool {
        self.lflag.contains(TermioLFlags::ECHO)
    }

    pub fn echoe(&self) -> bool {
        self.lflag.contains(TermioLFlags::ECHOE)
    }

    pub fn echoctl(&self) -> bool {
        self.lflag.contains(TermioLFlags::ECHOCTL)
    }

    pub fn echoke(&self) -> bool {
        self.lflag.contains(TermioLFlags::ECHOKE)
    }

    pub fn echok(&self) -> bool {
        self.lflag.contains(TermioLFlags::ECHOK)
    }

    pub fn echonl(&self) -> bool {
        self.lflag.contains(TermioLFlags::ECHONL)
    }

    pub fn isig(&self) -> bool {
        self.lflag.contains(TermioLFlags::ISIG)
    }

    pub fn icanon(&self) -> bool {
        self.lflag.contains(TermioLFlags::ICANON)
    }

    pub fn iexten(&self) -> bool {
        self.lflag.contains(TermioLFlags::IEXTEN)
    }

    pub fn igncr(&self) -> bool {
        self.iflag.contains(TermioIFlags::IGNCR)
    }

    pub fn icrnl(&self) -> bool {
        self.iflag.contains(TermioIFlags::ICRNL)
    }

    pub fn inlcr(&self) -> bool {
        self.iflag.contains(TermioIFlags::INLCR)
    }

    pub fn noflsh(&self) -> bool {
        self.lflag.contains(TermioLFlags::NOFLSH)
    }

    pub fn new_standard() -> Self {
        let cc = core::array::from_fn(|idx| match idx {
            VINTR => CTRL!('C'),
            VQUIT => CTRL!('\\'),
            VERASE => 0x7f,
            VKILL => CTRL!('U'),
            VEOF => CTRL!('D'),
            VSUSP => CTRL!('Z'),
            VMIN => 1,
            _ => 0,
        });

        Self {
            iflag: TermioIFlags::ICRNL | TermioIFlags::IXOFF,
            oflag: TermioOFlags::OPOST | TermioOFlags::ONLCR,
            cflag: B38400 | CS8 | CREAD | HUPCL,
            lflag: TermioLFlags::ISIG
                | TermioLFlags::ICANON
                | TermioLFlags::ECHO
                | TermioLFlags::ECHOE
                | TermioLFlags::ECHOK
                | TermioLFlags::ECHOCTL
                | TermioLFlags::ECHOKE
                | TermioLFlags::IEXTEN,
            line: N_TTY,
            cc,
        }
    }

    fn get_user(&self) -> UserTermios {
        UserTermios {
            iflag: self.iflag.bits() as u32,
            oflag: self.oflag.bits() as u32,
            cflag: self.cflag,
            lflag: self.lflag.bits() as u32,
            line: self.line,
            cc: self.cc,
        }
    }
}

pub trait TerminalDevice: Send + Sync {
    fn putchar(&self, ch: u8);
}

struct TerminalInner {
    termio: Termios,
    session: Weak<Session>,
    buffer: VecDeque<u8>,
}

pub struct Terminal {
    /// Lock with IRQ disabled. We might use this in IRQ context.
    inner: Spin<TerminalInner>,
    device: Arc<dyn TerminalDevice>,
    cv: CondVar,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UserWindowSize {
    row: u16,
    col: u16,
    xpixel: u16,
    ypixel: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UserTermios {
    iflag: u32,
    oflag: u32,
    cflag: u32,
    lflag: u32,
    line: u8,
    cc: [u8; NCCS],
}

pub enum TerminalIORequest<'a> {
    GetProcessGroup(UserPointerMut<'a, u32>),
    SetProcessGroup(UserPointer<'a, u32>),
    GetWindowSize(UserPointerMut<'a, UserWindowSize>),
    GetTermios(UserPointerMut<'a, UserTermios>),
    SetTermios(UserPointer<'a, UserTermios>),
}

impl core::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Terminal").finish()
    }
}

impl Terminal {
    pub fn new(device: Arc<dyn TerminalDevice>) -> Arc<Self> {
        Arc::new(Self {
            inner: Spin::new(TerminalInner {
                termio: Termios::new_standard(),
                session: Weak::new(),
                buffer: VecDeque::with_capacity(BUFFER_SIZE),
            }),
            cv: CondVar::new(),
            device,
        })
    }

    /// Clear the read buffer.
    fn clear_read_buffer(&self, inner: &mut TerminalInner) {
        inner.buffer.clear();
    }

    // TODO: Buffer terminal writes.
    pub fn show_char(&self, ch: u8) {
        self.device.putchar(ch)
    }

    fn erase(&self, inner: &mut TerminalInner, echo: bool) -> Option<u8> {
        let back = inner.buffer.back().copied();
        match back {
            None => return None,
            Some(b'\n') => return None,
            Some(back) if back == inner.termio.veof() => return None,
            Some(back) if back == inner.termio.veol() => return None,
            Some(back) if back == inner.termio.veol2() => return None,
            _ => {}
        }
        let back = inner.buffer.pop_back();

        if echo && inner.termio.echo() && inner.termio.echoe() {
            self.show_char(CTRL!('H')); // Backspace
            self.show_char(b' '); // Space
            self.show_char(CTRL!('H')); // Backspace
        }

        return back;
    }

    fn echo_char(&self, inner: &mut TerminalInner, ch: u8) {
        match ch {
            b'\t' | b'\n' | CTRL!('Q') | CTRL!('S') => self.show_char(ch),
            c if c >= 32 => self.show_char(ch),
            _ if !inner.termio.echo() => self.show_char(ch),
            _ if !inner.termio.echoctl() => self.show_char(ch),
            _ if !inner.termio.iexten() => self.show_char(ch),
            _ => {
                self.show_char(b'^');
                self.show_char(ch + 0x40);
            }
        }
    }

    fn signal(&self, inner: &mut TerminalInner, signal: Signal) {
        if let Some(session) = inner.session.upgrade() {
            session.raise_foreground(signal);
        }
        if !inner.termio.noflsh() {
            self.clear_read_buffer(inner);
        }
    }

    fn echo_and_signal(&self, inner: &mut TerminalInner, ch: u8, signal: Signal) {
        self.echo_char(inner, ch);
        self.signal(inner, signal);
    }

    fn do_commit_char(&self, inner: &mut TerminalInner, ch: u8) {
        inner.buffer.push_back(ch);

        if inner.termio.echo() || (ch == b'\n' && inner.termio.echonl()) {
            self.echo_char(inner, ch);
        }

        // If ICANON is not set, we notify all waiting processes.
        // If ICANON is set but we have a new line, there are data ready, we notify as well.
        if ch == b'\n' || !inner.termio.icanon() {
            self.cv.notify_all();
        }
    }

    // TODO: Find a better way to handle this.
    pub fn commit_char(&self, ch: u8) {
        let mut inner = self.inner.lock_irq();
        if inner.termio.isig() {
            match ch {
                0xff => {}
                ch if ch == inner.termio.vintr() => {
                    return self.echo_and_signal(&mut inner, ch, Signal::SIGINT)
                }
                ch if ch == inner.termio.vquit() => {
                    return self.echo_and_signal(&mut inner, ch, Signal::SIGQUIT)
                }
                ch if ch == inner.termio.vsusp() => {
                    return self.echo_and_signal(&mut inner, ch, Signal::SIGTSTP)
                }
                _ => {}
            }
        }

        // If handled, the character is discarded.
        if inner.termio.icanon() {
            match ch {
                0xff => {}
                ch if ch == inner.termio.veof() => return self.cv.notify_all(),
                ch if ch == inner.termio.verase() => {
                    self.erase(&mut inner, true);
                    return;
                }
                ch if ch == inner.termio.vkill() => {
                    if inner.termio.echok() {
                        while self.erase(&mut inner, false).is_some() {}
                        self.show_char(b'\n');
                    } else if inner.termio.echoke() && inner.termio.iexten() {
                        while self.erase(&mut inner, true).is_some() {}
                    }
                    return;
                }
                _ => {}
            }
        }

        match ch {
            b'\r' if inner.termio.igncr() => {}
            b'\r' if inner.termio.icrnl() => return self.do_commit_char(&mut inner, b'\n'),
            b'\n' if inner.termio.inlcr() => return self.do_commit_char(&mut inner, b'\r'),
            _ => self.do_commit_char(&mut inner, ch),
        }
    }

    pub fn poll_in(&self) -> KResult<()> {
        let mut inner = self.inner.lock_irq();
        if inner.buffer.is_empty() {
            self.cv.wait(&mut inner);

            if Thread::current().signal_list.has_pending_signal() {
                return Err(EINTR);
            }
        }
        Ok(())
    }

    pub fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        let mut tmp_buffer = [0u8; 32];

        let data = 'block: {
            if buffer.available() == 0 {
                break 'block &tmp_buffer[..0];
            }

            let mut inner = self.inner.lock_irq();
            if inner.buffer.is_empty() {
                self.cv.wait(&mut inner);

                if Thread::current().signal_list.has_pending_signal() {
                    return Err(EINTR);
                }
            }

            if inner.buffer.is_empty() {
                break 'block &tmp_buffer[..0];
            }

            let length = if inner.termio.icanon() {
                // Canonical mode, return data until we see a newline.
                // TODO!!: We should wait if we don't see one.
                inner
                    .buffer
                    .iter()
                    .position(|&ch| ch == b'\n')
                    .map(|pos| pos + 1)
                    .unwrap_or(inner.buffer.len())
            } else {
                buffer.available().min(inner.buffer.len())
            };

            // TODO!!!!!!: Change this. We need to loop until we've got enough data.
            let length = length.min(tmp_buffer.len());

            for (ch, r) in inner
                .buffer
                .drain(..length)
                .zip(tmp_buffer.iter_mut().take(length))
            {
                *r = ch;
            }

            &tmp_buffer[..length]
        };

        buffer.fill(data).map(|result| result.allow_partial())
    }

    pub fn ioctl(&self, request: TerminalIORequest) -> KResult<()> {
        match request {
            TerminalIORequest::GetProcessGroup(pgid_pointer) => {
                let session = self.inner.lock().session.upgrade();
                let pgid = session.map(|session| session.foreground_pgid()).flatten();

                if let Some(pgid) = pgid {
                    pgid_pointer.write(pgid)
                } else {
                    Err(ENOTTY)
                }
            }
            TerminalIORequest::SetProcessGroup(pgid) => {
                let pgid = pgid.read()?;

                let inner = self.inner.lock();
                let session = inner.session.upgrade();

                if let Some(session) = session {
                    session.set_foreground_pgid(pgid)
                } else {
                    Err(ENOTTY)
                }
            }
            TerminalIORequest::GetWindowSize(ptr) => {
                // TODO: Get the actual window size
                let window_size = UserWindowSize {
                    row: 40,
                    col: 80,
                    xpixel: 0,
                    ypixel: 0,
                };

                ptr.write(window_size)
            }
            TerminalIORequest::GetTermios(ptr) => {
                let termios = self.inner.lock().termio.get_user();
                ptr.write(termios)
            }
            TerminalIORequest::SetTermios(ptr) => {
                let user_termios = ptr.read()?;
                let mut inner = self.inner.lock();

                // TODO: We ignore unknown bits for now.
                inner.termio.iflag = TermioIFlags::from_bits_truncate(user_termios.iflag as u16);
                inner.termio.oflag = TermioOFlags::from_bits_truncate(user_termios.oflag as u16);
                inner.termio.lflag = TermioLFlags::from_bits_truncate(user_termios.lflag as u16);
                inner.termio.cflag = user_termios.cflag;
                inner.termio.line = user_termios.line;
                inner.termio.cc = user_termios.cc;

                Ok(())
            }
        }
    }

    /// Assign the `session` to this terminal. Drop the previous session if `forced` is true.
    pub fn set_session(&self, session: &Arc<Session>, forced: bool) -> KResult<()> {
        let mut inner = self.inner.lock();
        if let Some(session) = inner.session.upgrade() {
            if !forced {
                Err(EPERM)
            } else {
                session.drop_control_terminal();
                inner.session = Arc::downgrade(&session);
                Ok(())
            }
        } else {
            // Sessions should set their `control_terminal` field.
            inner.session = Arc::downgrade(&session);
            Ok(())
        }
    }

    pub fn drop_session(&self) {
        self.inner.lock().session = Weak::new();
    }

    pub fn session(&self) -> Option<Arc<Session>> {
        self.inner.lock().session.upgrade()
    }
}
