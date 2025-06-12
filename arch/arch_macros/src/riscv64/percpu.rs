use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

/// Get the base address for percpu variables of the current thread.
pub fn get_percpu_pointer(percpu: &Ident, ty: &Type) -> TokenStream {
    quote! {
        {
            let base: *mut #ty;
            ::core::arch::asm!(
                "la {base}, {offset}",
                base = out(reg) base,
                offset = sym #percpu,
                options(nostack, preserves_flags)
            );
            base
        }
    }
}
