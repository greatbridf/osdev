use crate::{
    bindings::root::{interrupt_stack, mmx_registers},
    kernel::task::{ProcessList, Signal},
    println_warn,
};

mod file_rw;
mod mm;
mod net;
mod procops;
mod sysinfo;

pub(self) struct MapArgumentImpl;
pub(self) trait MapArgument<'a, T: 'a> {
    fn map_arg(value: u64) -> T;
}

pub(self) trait MapReturnValue {
    fn map_ret(self) -> usize;
}

impl MapReturnValue for () {
    fn map_ret(self) -> usize {
        0
    }
}

impl MapReturnValue for u32 {
    fn map_ret(self) -> usize {
        self as usize
    }
}

impl MapReturnValue for usize {
    fn map_ret(self) -> usize {
        self
    }
}

impl MapArgument<'_, u64> for MapArgumentImpl {
    fn map_arg(value: u64) -> u64 {
        value as u64
    }
}

impl MapArgument<'_, u32> for MapArgumentImpl {
    fn map_arg(value: u64) -> u32 {
        value as u32
    }
}

impl MapArgument<'_, i32> for MapArgumentImpl {
    fn map_arg(value: u64) -> i32 {
        value as i32
    }
}

impl MapArgument<'_, usize> for MapArgumentImpl {
    fn map_arg(value: u64) -> usize {
        value as usize
    }
}

impl<'a, T: 'a> MapArgument<'a, *const T> for MapArgumentImpl {
    fn map_arg(value: u64) -> *const T {
        value as *const _
    }
}

impl<'a, T: 'a> MapArgument<'a, *mut T> for MapArgumentImpl {
    fn map_arg(value: u64) -> *mut T {
        value as *mut _
    }
}

macro_rules! arg_register {
    (0, $is:ident) => {
        $is.regs.rbx
    };
    (1, $is:ident) => {
        $is.regs.rcx
    };
    (2, $is:ident) => {
        $is.regs.rdx
    };
    (3, $is:ident) => {
        $is.regs.rsi
    };
    (4, $is:ident) => {
        $is.regs.rdi
    };
    (5, $is:ident) => {
        $is.regs.rbp
    };
}

macro_rules! format_expand {
    ($name:ident, $arg:tt) => {
        format_args!("{}: {:x?}", stringify!($name), $arg)
    };
    ($name1:ident, $arg1:tt, $($name:ident, $arg:tt),*) => {
        format_args!("{}: {:x?}, {}", stringify!($name1), $arg1, format_expand!($($name, $arg),*))
    }
}

macro_rules! syscall32_call {
    ($is:ident, $handler:ident, $($arg:ident: $type:ty),*) => {{
        use $crate::kernel::syscall::{MapArgument, MapArgumentImpl, arg_register};
        use $crate::kernel::syscall::{MapReturnValue, format_expand};
        use $crate::{kernel::task::Thread, println_info};

        $(
            let $arg: $type =
                MapArgumentImpl::map_arg(arg_register!(${index()}, $is));
        )*

        if cfg!(feature = "debug_syscall") {
            println_info!(
                "tid{}: {}({}) => {{",
                Thread::current().tid,
                stringify!($handler),
                format_expand!($($arg, $arg),*),
            );
        }

        let result = $handler($($arg),*);

        if cfg!(feature = "debug_syscall") {
            println_info!(
                "tid{}: {}({}) => }} = {:x?}",
                Thread::current().tid,
                stringify!($handler),
                format_expand!($($arg, $arg),*),
                result
            );
        }

        match result {
            Ok(val) => MapReturnValue::map_ret(val),
            Err(err) => (-(err as i32)) as usize,
        }
    }};
}

macro_rules! define_syscall32 {
    ($name:ident, $handler:ident) => {
        fn $name(_int_stack: &mut $crate::bindings::root::interrupt_stack,
            _mmxregs: &mut $crate::bindings::root::mmx_registers) -> usize {
            use $crate::kernel::syscall::MapReturnValue;

            match $handler() {
                Ok(val) => MapReturnValue::map_ret(val),
                Err(err) => (-(err as i32)) as usize,
            }
        }
    };
    ($name:ident, $handler:ident, $($arg:ident: $argt:ty),*) => {
        fn $name(
            int_stack: &mut $crate::bindings::root::interrupt_stack,
            _mmxregs: &mut $crate::bindings::root::mmx_registers) -> usize {
            use $crate::kernel::syscall::syscall32_call;

            syscall32_call!(int_stack, $handler, $($arg: $argt),*)
        }
    };
}

macro_rules! register_syscall {
    ($no:expr, $name:ident) => {
        $crate::kernel::syscall::register_syscall_handler(
            $no,
            concat_idents!(sys_, $name),
            stringify!($name),
        );
    };
}

use super::task::Thread;

pub(self) use {arg_register, define_syscall32, format_expand, register_syscall, syscall32_call};

pub(self) struct SyscallHandler {
    handler: fn(&mut interrupt_stack, &mut mmx_registers) -> usize,
    name: &'static str,
}

pub(self) fn register_syscall_handler(
    no: usize,
    handler: fn(&mut interrupt_stack, &mut mmx_registers) -> usize,
    name: &'static str,
) {
    // SAFETY: `SYSCALL_HANDLERS` is never modified after initialization.
    let syscall = unsafe { SYSCALL_HANDLERS.get_mut(no) }.unwrap();
    assert!(
        syscall.replace(SyscallHandler { handler, name }).is_none(),
        "Syscall {} is already registered",
        no
    );
}

pub fn register_syscalls() {
    file_rw::register();
    procops::register();
    mm::register();
    net::register();
    sysinfo::register();
}

const SYSCALL_HANDLERS_SIZE: usize = 404;
static mut SYSCALL_HANDLERS: [Option<SyscallHandler>; SYSCALL_HANDLERS_SIZE] =
    [const { None }; SYSCALL_HANDLERS_SIZE];

pub fn handle_syscall32(no: usize, int_stack: &mut interrupt_stack, mmxregs: &mut mmx_registers) {
    // SAFETY: `SYSCALL_HANDLERS` are never modified after initialization.
    let syscall = unsafe { SYSCALL_HANDLERS.get(no) }.and_then(Option::as_ref);

    match syscall {
        None => {
            println_warn!("Syscall {no}({no:#x}) isn't implemented.");
            ProcessList::kill_current(Signal::SIGSYS);
        }
        Some(handler) => {
            arch::interrupt::enable();
            let retval = (handler.handler)(int_stack, mmxregs);

            // SAFETY: `int_stack` is always valid.
            int_stack.regs.rax = retval as u64;
            int_stack.regs.r8 = 0;
            int_stack.regs.r9 = 0;
            int_stack.regs.r10 = 0;
            int_stack.regs.r11 = 0;
            int_stack.regs.r12 = 0;
            int_stack.regs.r13 = 0;
            int_stack.regs.r14 = 0;
            int_stack.regs.r15 = 0;
        }
    }

    if Thread::current().signal_list.has_pending_signal() {
        Thread::current().signal_list.handle(int_stack, mmxregs);
    }
}
