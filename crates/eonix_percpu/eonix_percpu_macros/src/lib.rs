extern crate proc_macro;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse2, Ident, ItemStatic, Type};

fn define_percpu_impl(
    attrs: TokenStream,
    item: TokenStream,
    get_percpu_pointer: fn(&Ident, &Type) -> TokenStream,
) -> TokenStream {
    if !attrs.is_empty() {
        panic!("`define_percpu` attribute does not take any arguments");
    }

    let item = parse2::<ItemStatic>(item).unwrap();
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

    let as_ptr = get_percpu_pointer(&inner_ident, &ty);

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

fn define_percpu_shared_impl(
    attrs: TokenStream,
    item: TokenStream,
    get_percpu_pointer: fn(&Ident, &Type) -> TokenStream,
) -> TokenStream {
    if !attrs.is_empty() {
        panic!("`define_percpu_shared` attribute does not take any arguments");
    }

    let item = parse2::<ItemStatic>(item).unwrap();
    let vis = &item.vis;
    let ident = &item.ident;
    let ty = &item.ty;
    let expr = &item.expr;

    let inner_ident = format_ident!("_percpu_shared_inner_{}", ident);
    let access_ident = format_ident!("_access_shared_{}", ident);

    let as_ptr = get_percpu_pointer(&inner_ident, &ty);

    quote! {
        #[link_section = ".percpu"]
        #[allow(non_upper_case_globals)]
        static #inner_ident: #ty = #expr;
        #[allow(non_camel_case_types)]
        #vis struct #access_ident;
        #vis static #ident: #access_ident = #access_ident;

        impl #access_ident {
            fn as_ptr(&self) -> *const #ty {
                unsafe { ( #as_ptr ) }
            }

            pub fn get_ref(&self) -> & #ty {
                // SAFETY: This is safe because `as_ptr()` is guaranteed to be valid.
                unsafe { self.as_ptr().as_ref().unwrap() }
            }

            pub fn get_for_cpu(&self, cpuid: usize) -> Option<& #ty > {
                let offset = & #inner_ident as *const _ as usize;
                let base = ::eonix_percpu::PercpuArea::get_for(cpuid);
                base.map(|base| unsafe { base.byte_add(offset).cast().as_ref() })
            }
        }

        impl ::core::ops::Deref for #access_ident {
            type Target = #ty;

            fn deref(&self) -> &Self::Target {
                self.get_ref()
            }
        }

        impl<T> ::core::convert::AsRef<T> for #access_ident
        where
            <Self as ::core::ops::Deref>::Target: ::core::convert::AsRef<T>,
        {
            fn as_ref(&self) -> &T {
                use ::core::ops::Deref;

                self.deref().as_ref()
            }
        }
    }
}

#[proc_macro_attribute]
pub fn define_percpu_x86_64(
    attrs: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    define_percpu_impl(
        attrs.into(),
        item.into(),
        arch_macros::x86_64::percpu::get_percpu_pointer,
    )
    .into()
}

#[proc_macro_attribute]
pub fn define_percpu_shared_x86_64(
    attrs: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    define_percpu_shared_impl(
        attrs.into(),
        item.into(),
        arch_macros::x86_64::percpu::get_percpu_pointer,
    )
    .into()
}
