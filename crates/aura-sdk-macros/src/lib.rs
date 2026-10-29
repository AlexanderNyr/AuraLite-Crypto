//! X4 — proc macros for AuraLite-Crypto contracts.
//!
//! Three attributes. Each one does exactly one thing: it **renames** the
//! annotated function and emits an `extern "C"` wrapper with the export name the
//! X3 lifecycle expects. No code generation beyond that — the body the author
//! wrote is the body that runs.
//!
//! | attribute | export | arity | called by |
//! |---|---|---|---|
//! | `#[entrypoint]` | `run` | 0 | `Lifecycle::call_stored` |
//! | `#[entrypoint(arg)]` | `run` | 1 | `Lifecycle::call_stored` with an argument |
//! | `#[constructor]` | `init` | 0 | `Lifecycle::deploy` |
//! | `#[constructor(arg)]` | `init` | 1 | `Lifecycle::deploy` with an argument |
//! | `#[migrate]` | `migrate` | 2 | `Lifecycle::upgrade` |
//!
//! # Why an attribute and not a trait
//!
//! The engine calls a WASM export by name and arity ([`Engine::entry_arity`]);
//! there is no type-level hook to implement. `#[entrypoint]` is the smallest
//! thing that turns a Rust function into that export, and it can only be used
//! once per module per name — a second `#[entrypoint]` is a duplicate-symbol
//! link error, which is the right failure.
//!
//! # Return convention
//!
//! The wrapped function returns `u32`. The wrapper returns it as `i32` without
//! interpretation: the ABI's negative codes are reserved for the *host*, so a
//! contract that wants to signal a refusal calls [`aura_sdk::abort`], which
//! traps and reverts the call.
//!
//! ```ignore
//! use aura_sdk::prelude::*;
//!
//! #[entrypoint]
//! pub fn run() -> u32 {
//!     0
//! }
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{ItemFn, LitStr, parse_macro_input};

/// Which export a wrapper produces.
enum Kind {
    Run,
    Init,
    Migrate,
}

impl Kind {
    fn export(&self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Init => "init",
            Self::Migrate => "migrate",
        }
    }
}

/// `#[entrypoint]` / `#[entrypoint(arg)]` — an empty or one-argument attribute.
struct Attribute {
    arg: bool,
}

impl Parse for Attribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Self { arg: false });
        }
        let ident: syn::Ident = input.parse()?;
        if ident != "arg" {
            return Err(syn::Error::new(
                ident.span(),
                "unknown argument: expected `#[entrypoint]`, `#[entrypoint(arg)]` \
                 or `#[constructor(arg)]`",
            ));
        }
        Ok(Self { arg: true })
    }
}

fn expand(kind: Kind, arg: bool, item: ItemFn) -> syn::Result<proc_macro2::TokenStream> {
    let export = kind.export();
    let wrapper_ident = syn::Ident::new(export, item.sig.ident.span());
    let body_ident = syn::Ident::new(&format!("__aura_{export}"), item.sig.ident.span());

    // The body must take exactly the parameters the export will pass, and it
    // must return `u32`. Both are reported here rather than at link time,
    // because the message is much better.
    let expected = match kind {
        Kind::Migrate => 2,
        Kind::Run | Kind::Init if arg => 1,
        Kind::Run | Kind::Init => 0,
    };
    let mut body = item.clone();
    body.sig.ident = body_ident.clone();
    if body.sig.inputs.len() != expected {
        return Err(syn::Error::new_spanned(
            &body.sig.inputs,
            format!(
                "`{export}` takes {expected} parameter(s); the function has {}",
                body.sig.inputs.len()
            ),
        ));
    }
    match &body.sig.output {
        syn::ReturnType::Type(_, ty) => match ty.as_ref() {
            syn::Type::Path(path) if path.path.is_ident("u32") => {}
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "an entry point must return u32 (call aura_sdk::abort to revert)",
                ));
            }
        },
        syn::ReturnType::Default => {
            return Err(syn::Error::new_spanned(
                &body.sig,
                "an entry point must return u32 (call aura_sdk::abort to revert)",
            ));
        }
    }

    let wrapper = match kind {
        Kind::Run | Kind::Init if arg => quote! {
            #[unsafe(no_mangle)]
            pub extern "C" fn #wrapper_ident(arg: i32) -> i32 {
                #body_ident(arg) as i32
            }
        },
        Kind::Run | Kind::Init => quote! {
            #[unsafe(no_mangle)]
            pub extern "C" fn #wrapper_ident() -> i32 {
                #body_ident() as i32
            }
        },
        Kind::Migrate => {
            quote! {
                #[unsafe(no_mangle)]
                pub extern "C" fn #wrapper_ident(from: i32, to: i32) -> i32 {
                    #body_ident(from, to) as i32
                }
            }
        }
    };

    // A short doc comment lands on the wrapper so `cargo doc` on a contract
    // shows which export the function became.
    let note = LitStr::new(
        &format!("WASM export `{export}` generated by `aura_sdk`."),
        proc_macro2::Span::call_site(),
    );

    Ok(quote! {
        #[doc = #note]
        #wrapper

        #body
    })
}

/// Turn a function into the `run` export the lifecycle calls.
#[proc_macro_attribute]
pub fn entrypoint(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as Attribute);
    let item = parse_macro_input!(item as ItemFn);
    expand(Kind::Run, attr.arg, item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Turn a function into the `init` export `Lifecycle::deploy` calls.
#[proc_macro_attribute]
pub fn constructor(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as Attribute);
    let item = parse_macro_input!(item as ItemFn);
    expand(Kind::Init, attr.arg, item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Turn a function into the `migrate(from, to)` export `Lifecycle::upgrade`
/// calls.
#[proc_macro_attribute]
pub fn migrate(attr: TokenStream, item: TokenStream) -> TokenStream {
    // `#[migrate]` takes no attribute argument, but `#[migrate(arg)]` is a
    // common typo for it — reject it loudly instead of ignoring it.
    let parsed = parse_macro_input!(attr as Attribute);
    let item = parse_macro_input!(item as ItemFn);
    expand(Kind::Migrate, parsed.arg, item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
