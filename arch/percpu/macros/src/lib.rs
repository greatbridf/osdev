extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, ItemStatic};

mod arch;

#[proc_macro_attribute]
pub fn define_percpu(attrs: TokenStream, item: TokenStream) -> TokenStream {
    if !attrs.is_empty() {
        panic!("`define_percpu` attribute does not take any arguments");
    }

    let item = parse_macro_input!(item as ItemStatic);
    let vis = &item.vis;
    let ident = &item.ident;
    let ty = &item.ty;
    let expr = &item.expr;

    if !["bool", "u8", "u16", "u32", "u64", "usize"].contains(&quote!(#ty).to_string().as_str()) {
        panic!("`define_percpu` only supports bool, u8, u16, u32, u64 and usize");
    }

    let inner_ident = format_ident!("_percpu_inner_{}", ident);
    let access_ident = format_ident!("_access_{}", ident);

    let integer_methods = match quote!(#ty).to_string().as_str() {
        "bool" => quote! {},
        _ => quote! {
            pub fn add(&self, value: #ty) {
                *unsafe { self.as_mut() } += value;
            }

            pub fn sub(&self, value: #ty) {
                *unsafe { self.as_mut() } -= value;
            }
        },
    };

    let as_ptr = arch::get_percpu_pointer(&inner_ident, &ty);

    quote! {
        #[link_section = ".percpu"]
        #[allow(non_upper_case_globals)]
        static mut #inner_ident: #ty = #expr;
        #[allow(non_camel_case_types)]
        #vis struct #access_ident;
        #vis static #ident: #access_ident = #access_ident;

        impl #access_ident {
            pub unsafe fn as_ptr(&self) -> *mut #ty {
                #as_ptr
            }

            pub fn get(&self) -> #ty {
                unsafe { self.as_ptr().read() }
            }

            pub fn set(&self, value: #ty) {
                unsafe { self.as_ptr().write(value) }
            }

            /// # Safety
            /// This function is unsafe because it allows for immutable aliasing of the percpu
            /// variable.
            /// Make sure that preempt is disabled when calling this function.
            pub unsafe fn as_ref(&self) -> & #ty {
                // SAFETY: This is safe because `as_ptr()` is guaranteed to be valid.
                self.as_ptr().as_ref().unwrap()
            }

            /// # Safety
            /// This function is unsafe because it allows for mutable aliasing of the percpu
            /// variable.
            /// Make sure that preempt is disabled when calling this function.
            pub unsafe fn as_mut(&self) -> &mut #ty {
                // SAFETY: This is safe because `as_ptr()` is guaranteed to be valid.
                self.as_ptr().as_mut().unwrap()
            }

            #integer_methods
        }
    }
    .into()
}
