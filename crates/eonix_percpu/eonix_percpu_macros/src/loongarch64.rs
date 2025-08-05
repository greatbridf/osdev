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
                "sub.d     {base}, {base}, {start}",
                "add.d     {base}, {base}, $tp",
                base = inout(reg) &raw const #percpu => base,
                start = in(reg) PERCPU_DATA_START as usize,
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
                "la.global {tmp},    {start}",
                "la.global {output}, {var}",
                "sub.d     {output}, {output}, {tmp}",
                start = sym PERCPU_DATA_START,
                var = sym #percpu,
                tmp = out(reg) _,
                output = out(reg) offset,
                options(nostack, preserves_flags)
            );

            offset
        }
    }
}
