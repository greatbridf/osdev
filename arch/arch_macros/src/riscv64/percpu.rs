use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

/// Get the base address for percpu variables of the current thread.
pub fn get_percpu_pointer(percpu: &Ident, ty: &Type) -> TokenStream {
    quote! {
        {
            let base: *mut #ty;

            unsafe extern "C" {
                fn PERCPU_DATA_START();
            }

            ::core::arch::asm!(
                "la t0, {start}",
                "la {base}, {var}",
                "sub {base}, {base}, t0",
                "add {base}, {base}, tp",
                base = out(reg) base,
                start = sym PERCPU_DATA_START,
                out("t0") _,
                var = sym #percpu,
                options(nostack, preserves_flags)
            );

            base
        }
    }
}

pub fn get_percpu_offset(percpu: &Ident) -> TokenStream {
    quote! {
        unsafe {
            let offset: usize;

            unsafe extern "C" {
                fn PERCPU_DATA_START();
            }

            ::core::arch::asm!(
                "la t0, {start}",
                "la t1, {var}",
                "sub t1, t1, t0",
                start = sym PERCPU_DATA_START,
                var = sym #percpu,
                out("t0") _,
                out("t1") offset,
                options(nostack, preserves_flags)
            );

            offset
        }
    }
}
