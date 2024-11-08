use crate::bindings::root::{interrupt_stack, mmx_registers};

mod file_rw;
mod procops;

pub(self) trait MapReturnValue {
    fn map(self) -> u32;
}

impl MapReturnValue for () {
    fn map(self) -> u32 {
        0
    }
}

impl MapReturnValue for u32 {
    fn map(self) -> u32 {
        self
    }
}

impl MapReturnValue for usize {
    fn map(self) -> u32 {
        self as u32
    }
}

macro_rules! syscall32_call {
    ($int_stack:ident, $handler:ident, $arg1:ident: $argt1:ty) => {{
        let $arg1: $argt1 = $int_stack.regs.rbx as $argt1;
        match $handler($arg1) {
            Ok(val) => $crate::kernel::syscall::MapReturnValue::map(val),
            Err(err) => (-(err as i32)) as u32,
        }
    }};
    ($int_stack:ident, $handler:ident, $arg1:ident: $argt1:ty, $arg2:ident: $argt2:ty) => {{
        let $arg1: $argt1 = $int_stack.regs.rbx as $argt1;
        let $arg2: $argt2 = $int_stack.regs.rcx as $argt2;
        match $handler($arg1, $arg2) {
            Ok(val) => $crate::kernel::syscall::MapReturnValue::map(val),
            Err(err) => (-(err as i32)) as u32,
        }
    }};
    ($int_stack:ident, $handler:ident, $arg1:ident: $argt1:ty, $arg2:ident: $argt2:ty, $arg3:ident: $argt3:ty) => {{
        let $arg1: $argt1 = $int_stack.regs.rbx as $argt1;
        let $arg2: $argt2 = $int_stack.regs.rcx as $argt2;
        let $arg3: $argt3 = $int_stack.regs.rdx as $argt3;
        match $handler($arg1, $arg2, $arg3) {
            Ok(val) => $crate::kernel::syscall::MapReturnValue::map(val),
            Err(err) => (-(err as i32)) as u32,
        }
    }};
    ($int_stack:ident, $handler:ident,
     $arg1:ident: $argt1:ty,
     $arg2:ident: $argt2:ty,
     $arg3:ident: $argt3:ty,
     $arg4:ident: $argt4:ty) => {{
        let $arg1: $argt1 = $int_stack.regs.rbx as $argt1;
        let $arg2: $argt2 = $int_stack.regs.rcx as $argt2;
        let $arg3: $argt3 = $int_stack.regs.rdx as $argt3;
        let $arg4: $argt4 = $int_stack.regs.rsi as $argt4;
        match $handler($arg1, $arg2, $arg3, $arg4) {
            Ok(val) => $crate::kernel::syscall::MapReturnValue::map(val),
            Err(err) => (-(err as i32)) as u32,
        }
    }};
    ($int_stack:ident, $handler:ident,
     $arg1:ident: $argt1:ty,
     $arg2:ident: $argt2:ty,
     $arg3:ident: $argt3:ty,
     $arg4:ident: $argt4:ty,
     $arg5:ident: $argt5:ty) => {{
        let $arg1: $argt1 = $int_stack.regs.rbx as $argt1;
        let $arg2: $argt2 = $int_stack.regs.rcx as $argt2;
        let $arg3: $argt3 = $int_stack.regs.rdx as $argt3;
        let $arg4: $argt4 = $int_stack.regs.rsi as $argt4;
        let $arg5: $argt5 = $int_stack.regs.rdi as $argt5;
        match $handler($arg1, $arg2, $arg3, $arg4, $arg5) {
            Ok(val) => $crate::kernel::syscall::MapReturnValue::map(val),
            Err(err) => (-(err as i32)) as u32,
        }
    }};
    ($int_stack:ident, $handler:ident,
     $arg1:ident: $argt1:ty,
     $arg2:ident: $argt2:ty,
     $arg3:ident: $argt3:ty,
     $arg4:ident: $argt4:ty,
     $arg5:ident: $argt5:ty,
     $arg6:ident: $argt6:ty) => {{
        let $arg1: $argt1 = $int_stack.regs.rbx as $argt1;
        let $arg2: $argt2 = $int_stack.regs.rcx as $argt2;
        let $arg3: $argt3 = $int_stack.regs.rdx as $argt3;
        let $arg4: $argt4 = $int_stack.regs.rsi as $argt4;
        let $arg5: $argt5 = $int_stack.regs.rdi as $argt5;
        let $arg6: $argt6 = $int_stack.regs.rbp as $argt6;
        match $handler($arg1, $arg2, $arg3, $arg4, $arg5, $arg6) {
            Ok(val) => $crate::kernel::syscall::MapReturnValue::map(val),
            Err(err) => (-(err as i32)) as u32,
        }
    }};
}

macro_rules! define_syscall32 {
    ($name:ident, $handler:ident, $($arg:ident: $argt:ty),*) => {
        unsafe extern "C" fn $name(
            int_stack: *mut $crate::bindings::root::interrupt_stack,
            _mmxregs: *mut $crate::bindings::root::mmx_registers) -> u32 {
            let int_stack = int_stack.as_mut().unwrap();
            $crate::kernel::syscall::syscall32_call!(int_stack, $handler, $($arg: $argt),*)
        }
    };
}

pub(self) use {define_syscall32, syscall32_call};

extern "C" {
    fn register_syscall_handler(
        no: u32,
        handler: unsafe extern "C" fn(*mut interrupt_stack, *mut mmx_registers) -> u32,
        name: *const i8,
    );
}

#[no_mangle]
pub unsafe extern "C" fn r_register_syscall() {
    file_rw::register();
    procops::register();
}
