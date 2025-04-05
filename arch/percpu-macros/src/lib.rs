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

    let is_bool = quote!(#ty).to_string().as_str() == "bool";
    let is_integer =
        ["u8", "u16", "u32", "u64", "usize"].contains(&quote!(#ty).to_string().as_str());

    let is_atomic_like = is_bool || is_integer || quote!(#ty).to_string().contains("NonNull");

    let inner_ident = format_ident!("_percpu_inner_{}", ident);
    let access_ident = format_ident!("_access_{}", ident);

    let integer_methods = if is_integer {
        quote! {
            pub fn add(&self, value: #ty) {
                *unsafe { self.as_mut() } += value;
            }

            pub fn sub(&self, value: #ty) {
                *unsafe { self.as_mut() } -= value;
            }
        }
    } else {
        quote! {}
    };

    let preempt_disable = if !is_atomic_like {
        quote! { eonix_preempt::disable(); }
    } else {
        quote! {}
    };

    let preempt_enable = if !is_atomic_like {
        quote! { eonix_preempt::enable(); }
    } else {
        quote! {}
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
            /// # Safety
            /// This function is unsafe because it allows for mutable aliasing of the percpu
            /// variable.
            /// Make sure that preempt is disabled when calling this function.
            pub unsafe fn as_ptr(&self) -> *mut #ty {
                #as_ptr
            }

            pub fn get(&self) -> #ty {
                #preempt_disable
                let value = unsafe { self.as_ptr().read() };
                #preempt_enable
                value
            }

            pub fn set(&self, value: #ty) {
                #preempt_disable
                unsafe { self.as_ptr().write(value) }
                #preempt_enable
            }

            pub fn swap(&self, mut value: #ty) -> #ty {
                #preempt_disable
                unsafe { self.as_ptr().swap(&mut value) }
                #preempt_enable
                value
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
