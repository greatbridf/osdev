extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse, parse_macro_input, spanned::Spanned as _, FnArg, ItemFn};

/// Define the default trap handler. The function should take exactly one argument
/// of type `&mut TrapContext`.
///
/// # Usage
/// ```no_run
/// #[eonix_hal::default_trap_handler]
/// fn interrupt_handler(ctx: &mut TrapContext) {
///     println!("Trap {} received!", ctx.trap_no());
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn default_trap_handler(attrs: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as ItemFn);

    if !attrs.is_empty() {
        return parse::Error::new(
            item.span(),
            "`default_trap_handler` attribute does not take any arguments",
        )
        .into_compile_error()
        .into();
    }

    if item.sig.inputs.len() > 1 {
        return parse::Error::new(
            item.span(),
            "`default_trap_handler` only takes one argument",
        )
        .into_compile_error()
        .into();
    }

    let attrs = &item.attrs;
    let arg = item.sig.inputs.first().unwrap();
    let block = &item.block;

    quote! {
        #(#attrs)*
        #[no_mangle]
        pub unsafe extern "C" fn _default_trap_handler(#arg) #block
    }
    .into()
}

/// Define the entry point. The function should have signature like
///
/// ```ignore
/// [unsafe] fn ident(ident: eonix_hal::bootstrap::BootStrapData) -> !
/// ```
///
/// # Usage
/// ```no_run
/// #[eonix_hal::main]
/// fn kernel_main(data: eonix_hal::bootstrap::BootStrapData) -> ! {
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn main(attrs: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as ItemFn);

    if !attrs.is_empty() {
        return parse::Error::new(item.span(), "`main` attribute does not take any arguments")
            .into_compile_error()
            .into();
    }

    if item.sig.inputs.len() != 1 {
        return parse::Error::new(item.span(), "`main` should have exactly one argument.")
            .into_compile_error()
            .into();
    }

    let arg_ident = match item.sig.inputs.first().unwrap() {
        FnArg::Receiver(_) => {
            return parse::Error::new(
                item.span(),
                "`main` function cannot take `self` as an argument",
            )
            .into_compile_error()
            .into();
        }
        FnArg::Typed(ty) => &ty.pat,
    };

    let ident = &item.sig.ident;
    let attrs = item.attrs;
    let unsafety = item.sig.unsafety;
    let block = &item.block;

    quote! {
        #(#attrs)*
        #[export_name = "_eonix_hal_main"]
        pub #unsafety fn #ident(
            #arg_ident: eonix_hal::bootstrap::BootStrapData,
        ) -> ! #block
    }
    .into()
}

/// Define the AP entry point. The function should have signature like
///
/// ```ignore
/// [unsafe] fn ident(ident: eonix_mm::address::PRange) -> !
/// ```
///
/// # Usage
/// ```no_run
/// #[eonix_hal::main]
/// fn ap_main(stack_range: eonix_mm::address::PRange) -> ! {
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn ap_main(attrs: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as ItemFn);

    if !attrs.is_empty() {
        return parse::Error::new(
            item.span(),
            "`ap_main` attribute does not take any arguments",
        )
        .into_compile_error()
        .into();
    }

    if item.sig.inputs.len() != 1 {
        return parse::Error::new(item.span(), "`ap_main` should have exactly one argument.")
            .into_compile_error()
            .into();
    }

    let arg_ident = match item.sig.inputs.first().unwrap() {
        FnArg::Receiver(_) => {
            return parse::Error::new(
                item.span(),
                "`ap_main` function cannot take `self` as an argument",
            )
            .into_compile_error()
            .into();
        }
        FnArg::Typed(ty) => &ty.pat,
    };

    let ident = &item.sig.ident;
    let attrs = item.attrs;
    let unsafety = item.sig.unsafety;
    let block = &item.block;

    quote! {
        #(#attrs)*
        #[export_name = "_eonix_hal_ap_main"]
        pub #unsafety fn #ident(
            #arg_ident: eonix_mm::address::PRange,
        ) -> ! #block
    }
    .into()
}
