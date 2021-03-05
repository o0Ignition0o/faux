extern crate proc_macro;

mod create;
mod methods;
mod self_type;

use darling::FromMeta;
use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_attribute]
pub fn create(args: TokenStream, original: TokenStream) -> TokenStream {
    let original = syn::parse_macro_input!(original as syn::ItemStruct);

    let args = syn::parse_macro_input!(args as syn::AttributeArgs);
    let args = match create::Args::from_list(&args) {
        Ok(v) => v,
        Err(e) => {
            return e.write_errors().into();
        }
    };

    let mockable = create::Mockable::new(original, args);

    TokenStream::from(mockable)
}

#[proc_macro_attribute]
pub fn methods(args: TokenStream, original: TokenStream) -> TokenStream {
    let original = syn::parse_macro_input!(original as syn::ItemImpl);

    let args = syn::parse_macro_input!(args as syn::AttributeArgs);
    let args = match methods::Args::from_list(&args) {
        Ok(v) => v,
        Err(e) => {
            return e.write_errors().into();
        }
    };

    match methods::Mockable::new(original, args) {
        Ok(mockable) => TokenStream::from(mockable),
        Err(e) => e.write_errors().into(),
    }
}

#[proc_macro]
pub fn when(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let expr = syn::parse_macro_input!(input as syn::Expr);
    match expr {
        syn::Expr::Field(syn::ExprField {
            base,
            member: syn::Member::Named(ident),
            ..
        }) => {
            let when = quote::format_ident!("_when_{}", ident);
            TokenStream::from(quote!( { #base.#when() }))
        }
        syn::Expr::MethodCall(syn::ExprMethodCall {
            receiver,
            method,
            args,
            ..
        }) => {
            let args = args
                .into_iter()
                .map(|arg| match arg {
                    syn::Expr::Verbatim(t) if t.to_string() == "_" => {
                        quote!(faux::any())
                    }
                    syn::Expr::Reference(syn::ExprReference { expr, .. }) => {
                        quote!(faux::eq(faux::Ref(#expr)))
                    }
                    arg => {
                        quote!(faux::eq(#arg))
                    }
                })
                .collect::<Vec<_>>();

            let when = quote::format_ident!("_when_{}", method);
            TokenStream::from(quote!({ #receiver.#when().with_args((#(#args),*)) }))
        }
        _ => darling::Error::custom("when! only accepts arguments in the format of: `when!(receiver.method)` or `receiver.method(args...)`").with_span(&expr).write_errors().into(),
    }
}
