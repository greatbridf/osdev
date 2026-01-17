extern crate proc_macro;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{parse2, FnArg, Ident, ItemFn, LitStr};

fn define_syscall_impl(attrs: TokenStream, item: TokenStream) -> TokenStream {
    if attrs.is_empty() {
        panic!("`define_syscall` attribute should take one argument: `syscall_no`");
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

    let args_call = item
        .sig
        .inputs
        .iter()
        .enumerate()
        .map(|(idx, arg)| match arg {
            FnArg::Receiver(_) => panic!("&self is not permitted."),
            FnArg::Typed(_) => {
                let arg_ident = Ident::new(&format!("arg_{}", idx), Span::call_site());
                quote! { #arg_ident }
            }
        });

    let syscall_name = item.sig.ident;
    let syscall_name_str = LitStr::new(&syscall_name.to_string(), Span::call_site());
    let body = item.block;

    let helper_fn = Ident::new(&format!("_do_syscall_{}", syscall_name), Span::call_site());
    let helper_fn_pointer = Ident::new(
        &format!("_SYSCALL_ENTRY_{}", syscall_name.to_string().to_uppercase()),
        Span::call_site(),
    );

    let real_fn = Ident::new(&format!("sys_{}", syscall_name), Span::call_site());

    let raw_syscall_section = LitStr::new(
        &format!(".raw_syscalls.{}", syscall_name),
        Span::call_site(),
    );
    let syscall_fn_section =
        LitStr::new(&format!(".syscall_fns.{}", syscall_name), Span::call_site());

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
                let arg_ident = Ident::new(&format!("arg_{}", idx), Span::call_site());
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
    attrs: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    define_syscall_impl(attrs.into(), item.into()).into()
}
