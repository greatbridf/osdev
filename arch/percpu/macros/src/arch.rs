use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

/// Get the base address for percpu variables of the current thread.
pub fn get_percpu_pointer(percpu: &Ident, ty: &Type) -> TokenStream {
    quote! {
        #[cfg(target_arch = "x86_64")]
        {
            let base: *mut #ty;
            ::core::arch::asm!(
                "mov %gs:0, {address}",
                "add ${percpu_pointer}, {address}",
                percpu_pointer = sym #percpu,
                address = out(reg) base,
                options(att_syntax)
            );
            base
        }
    }
    .into()
}
