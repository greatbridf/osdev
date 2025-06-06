use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, LitStr, Type};

/// Generate the assembly code to load the address of a symbol and add it to a register.
pub fn load_addr_of_to(symbol: &LitStr, target: &LitStr) -> TokenStream {
    quote! {
        concat!("mov %gs:0, ", #target),
        concat!("add $", #symbol, ", ", #target)
    }
}

/// Get the base address for percpu variables of the current thread.
pub fn get_percpu_pointer(percpu: &Ident, ty: &Type) -> TokenStream {
    let stmt = load_addr_of_to(
        &LitStr::new("{ident}", Span::call_site()),
        &LitStr::new("{address}", Span::call_site()),
    );

    quote! {
        {
            let base: *mut #ty;
            ::core::arch::asm!(
                #stmt,
                ident = sym #percpu,
                address = out(reg) base,
                options(att_syntax, nostack, preserves_flags)
            );
            base
        }
    }
}
