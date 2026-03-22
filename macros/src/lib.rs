extern crate proc_macro;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    parse2, FnArg, Ident, ItemFn, ItemStruct, LitStr, ReturnType, Signature,
};

fn define_syscall_impl(attrs: TokenStream, item: TokenStream) -> TokenStream {
    if attrs.is_empty() {
        panic!(
            "`define_syscall` attribute should take one argument: `syscall_no`"
        );
    }

    let syscall_no = parse2::<Ident>(attrs).expect("Invalid syscall number");
    let item = parse2::<ItemFn>(item).unwrap();

    let attrs = item.attrs;
    let vis = item.vis;

    let args = item.sig.inputs.iter();
    let ty_ret = item.sig.output;

    assert!(
        item.sig.asyncness.is_some(),
        "Syscall must be async function"
    );

    let args_mapped = item
        .sig
        .inputs
        .iter()
        .enumerate()
        .map(|(idx, arg)| match arg {
            FnArg::Receiver(_) => panic!("&self is not permitted."),
            FnArg::Typed(arg) => {
                let arg_ident = Ident::new(&format!("arg_{}", idx), Span::call_site());
                let arg_ty = &arg.ty;
                quote! { let #arg_ident: #arg_ty = <#arg_ty>::from_arg(args[#idx]); }
            }
        });

    let args_call =
        item.sig
            .inputs
            .iter()
            .enumerate()
            .map(|(idx, arg)| match arg {
                FnArg::Receiver(_) => panic!("&self is not permitted."),
                FnArg::Typed(_) => {
                    let arg_ident =
                        Ident::new(&format!("arg_{}", idx), Span::call_site());
                    quote! { #arg_ident }
                }
            });

    let syscall_name = item.sig.ident;
    let syscall_name_str =
        LitStr::new(&syscall_name.to_string(), Span::call_site());
    let body = item.block;

    let helper_fn =
        Ident::new(&format!("_do_syscall_{}", syscall_name), Span::call_site());
    let helper_fn_pointer = Ident::new(
        &format!("_SYSCALL_ENTRY_{}", syscall_name.to_string().to_uppercase()),
        Span::call_site(),
    );

    let real_fn =
        Ident::new(&format!("sys_{}", syscall_name), Span::call_site());

    let raw_syscall_section = LitStr::new(
        &format!(".raw_syscalls.{}", syscall_name),
        Span::call_site(),
    );
    let syscall_fn_section = LitStr::new(
        &format!(".syscall_fns.{}", syscall_name),
        Span::call_site(),
    );

    let trace_format_string = {
        let arg_count = item.sig.inputs.len();
        let brackets = (0..arg_count)
            .map(|_| String::from("{:x?}"))
            .collect::<Vec<_>>()
            .join(", ");

        LitStr::new(&brackets, Span::call_site())
    };

    let trace_format_args = {
        let args = item.sig.inputs.iter();
        let args = args.enumerate().map(|(idx, arg)| match arg {
            FnArg::Receiver(_) => panic!("&self is not permitted."),
            FnArg::Typed(_) => {
                let arg_ident =
                    Ident::new(&format!("arg_{}", idx), Span::call_site());
                quote! { #arg_ident }
            }
        });

        quote! { #(#args,)* }
    };

    quote! {
        #[used]
        #[doc(hidden)]
        #[no_mangle]
        #[link_section = #raw_syscall_section]
        static #helper_fn_pointer: crate::kernel::syscall::RawSyscallHandler =
            crate::kernel::syscall::RawSyscallHandler {
                no: #syscall_no,
                handler: #helper_fn,
                name: #syscall_name_str,
            };

        #[link_section = #syscall_fn_section]
        fn #helper_fn <'thd, 'alloc>(
            thd: &'thd crate::kernel::task::Thread,
            thd_alloc: crate::kernel::task::ThreadAlloc<'alloc>,
            args: [usize; 6]
        ) -> core::pin::Pin<Box<
            dyn core::future::Future<Output = Option<usize>> + Send + 'thd,
            crate::kernel::task::ThreadAlloc<'alloc>
        >> {
            use crate::kernel::syscall::{FromSyscallArg, SyscallRetVal};
            use alloc::boxed::Box;

            #(#args_mapped)*

            unsafe {
                core::pin::Pin::new_unchecked(
                    Box::new_in(
                        async move {
                            eonix_log::println_trace!(
                                feat: "trace_syscall",
                                "tid{}: {}({}) => {{",
                                thd.tid,
                                #syscall_name_str,
                                format_args!(#trace_format_string, #trace_format_args),
                            );

                            let retval = #real_fn(thd, #(#args_call),*).await.into_retval();

                            eonix_log::println_trace!(
                                feat: "trace_syscall",
                                "}} => {:x?}",
                                retval,
                            );

                            retval
                        },
                        thd_alloc
                    )
                )
            }
        }

        #(#attrs)*
        #[link_section = #syscall_fn_section]
        #vis async fn #real_fn(
            thread: &crate::kernel::task::Thread,
            #(#args),*
        ) #ty_ret #body
    }
}

/// Define a syscall used by the kernel. The syscall handler will be generated in the
/// `.syscalls` section and then linked into the kernel binary.
///
/// One hidden parameter will be passed to the syscall handler:
/// - `thread: &Thread`
///
/// The arguments of the syscall MUST implement `FromSyscallArg` trait and the return value
/// types MUST implement `SyscallRetVal` trait.
///
/// # Usage
/// ```no_run
/// # use eonix_macros::define_syscall;
/// #[define_syscall]
/// fn read(fd: u32, buf: *mut u8, count: u32) -> u32
/// {
///     /* ... */
/// }
/// ```
#[proc_macro_attribute]
pub fn define_syscall(
    attrs: proc_macro::TokenStream, item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    define_syscall_impl(attrs.into(), item.into()).into()
}

fn check_late_init_sig(sig: &Signature) {
    assert!(sig.constness.is_none(), "no const function allowed");
    assert!(sig.inputs.is_empty(), "no arguments allowed");

    if let Some(abi) = &sig.abi {
        if let Some(name) = &abi.name {
            assert_eq!(name.value(), "Rust", "expected Rust ABI");
        }
    }

    assert!(
        matches!(&sig.output, ReturnType::Default),
        "no return types allowed"
    );

    let generics = &sig.generics;

    assert_eq!(generics.const_params().count(), 0, "no generics allowed");
    assert_eq!(generics.type_params().count(), 0, "no generics allowed")
}

fn define_late_init_normal(
    func: &TokenStream, func_parsed: &ItemFn,
) -> TokenStream {
    let func_ident = &func_parsed.sig.ident;
    let func_name = func_parsed.sig.ident.to_string();

    let static_ident = Ident::new(
        &format!("__LATE_INIT_{}", func_name.to_uppercase()),
        Span::call_site(),
    );

    quote! {
        #func

        #[used]
        #[doc(hidden)]
        #[link_section = ".late_init"]
        static #static_ident : fn() = #func_ident;
    }
}

fn define_late_init_async(
    func: &TokenStream, func_parsed: &ItemFn,
) -> TokenStream {
    let func_ident = &func_parsed.sig.ident;
    let func_name = func_parsed.sig.ident.to_string();

    let helper_ident = Ident::new(
        &format!("__late_init_async_wrapper_{}", func_name.to_uppercase()),
        Span::call_site(),
    );

    let static_ident = Ident::new(
        &format!("__LATE_INIT_ASYNC_{}", func_name.to_uppercase()),
        Span::call_site(),
    );

    let pin_t = quote!(core::pin::Pin);
    let box_t = quote!(alloc::boxed::Box);
    let future_t = quote!(core::future::Future);

    quote! {
        #func

        fn #helper_ident() ->
            #pin_t<#box_t<dyn #future_t<Output = ()> + Send>> {
            #box_t::pin(#func_ident())
        }

        #[used]
        #[doc(hidden)]
        #[link_section = ".late_init_async"]
        static #static_ident: fn() ->
            #pin_t<#box_t<dyn #future_t<Output = ()> + Send>> = #helper_ident;
    }
}

fn define_late_init_impl(attrs: TokenStream, func: TokenStream) -> TokenStream {
    if !attrs.is_empty() {
        panic!("attributes not allowed here");
    }

    let func_parsed =
        parse2::<ItemFn>(func.clone()).expect("expected function definition");

    check_late_init_sig(&func_parsed.sig);

    if func_parsed.sig.asyncness.is_some() {
        define_late_init_async(&func, &func_parsed)
    } else {
        define_late_init_normal(&func, &func_parsed)
    }
}

/// Define a function to run after kernel initialization.
///
/// # Note
/// Keep in mind that all the functions defined with this macro may be run in
/// **ANY** order.
///
/// So if order matters, try another way other than this!
#[proc_macro_attribute]
pub fn define_late_init(
    attrs: proc_macro::TokenStream, func: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    define_late_init_impl(attrs.into(), func.into()).into()
}

fn define_transparent_deref_impl(items: TokenStream) -> TokenStream {
    let def = parse2::<ItemStruct>(items).expect("expected struct definition");

    let ident = &def.ident;

    assert!(def.fields.len() == 1, "expected only 1 field");

    let field = def.fields.iter().next().unwrap();

    assert!(field.ident.is_none(), "expected tuple structs");

    let inner = &field.ty;

    quote! {
        impl core::ops::Deref for #ident {
            type Target = #inner;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    }
}

#[proc_macro_derive(TransparentDeref)]
pub fn define_transparent_deref(
    items: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    define_transparent_deref_impl(items.into()).into()
}
