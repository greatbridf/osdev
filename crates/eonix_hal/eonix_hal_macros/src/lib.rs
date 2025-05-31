extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

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
    if !attrs.is_empty() {
        panic!("`default_trap_handler` attribute does not take any arguments");
    }

    let item = parse_macro_input!(item as ItemFn);

    if item.sig.inputs.len() > 1 {
        panic!("`default_trap_handler` only takes one argument");
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
