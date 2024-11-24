mod morphed;
mod receiver;

use crate::{create, self_type::SelfType};
use darling::FromMeta;
use morphed::Signature;
use proc_macro2::Span;
use quote::quote;
use syn::{ImplItemFn, Lifetime, PathArguments, ReturnType};

#[derive(Default, FromMeta)]
#[darling(default)]
pub struct Args {
    path: Option<syn::Path>,
    self_type: SelfType,
}

pub struct Mockable {
    // the real definitions inside the impl block
    real: syn::ItemImpl,
    // the morphed definitions
    morphed: syn::ItemImpl,
    // the _when_ methods in their own impl
    whens: syn::ItemImpl,
    // path to real struct
    real_ty: syn::TypePath,
    // path to morphed struct
    morphed_ty: syn::TypePath,
}

impl Mockable {
    pub fn new(real: syn::ItemImpl, args: Args) -> darling::Result<Self> {
        // check that we can support this impl
        let morphed_ty = match real.self_ty.as_ref() {
            syn::Type::Path(type_path) => type_path.clone(),
            node => {
                return Err(darling::Error::custom("#[faux::methods] does not support implementing types that are not a simple path").with_span(node));
            }
        };

        if let Some(segment) = morphed_ty
            .path
            .segments
            .iter()
            .find(|segment| segment.ident == "crate" || segment.ident == "super")
        {
            return Err(
                darling::Error::custom(
                    format!("#[faux::methods] does not support implementing types with '{segment}' in the path. Consider importing one level past '{segment}' and using #[faux::methods(path = \"{segment}\")]", segment = segment.ident)
                ).with_span(segment)
            );
        }

        // start transforming
        let real_ty = real_ty(&morphed_ty, args.path);

        let mut morphed = real.clone();

        let mut methods = morphed.items.iter_mut().filter_map(|item| match item {
            syn::ImplItem::Fn(m) => Some(m),
            _ => None,
        });

        let mut when_methods = vec![];
        for func in &mut methods {
            normalize_idents(&mut func.sig);
            let signature = Signature::morph(
                &func.sig,
                real.trait_.as_ref().map(|(_, path, _)| path),
                &func.vis,
            );
            func.block = signature.create_body(args.self_type, &real_ty, &morphed_ty)?;
            if let Some(methods) = signature.create_when() {
                when_methods.extend(methods.into_iter().map(syn::ImplItem::Fn));
            }
            desugar_async(func);
            func.attrs.push(syn::parse_quote! { #[track_caller] });
        }

        let generics = match &morphed_ty.path.segments.last().unwrap().arguments {
            syn::PathArguments::AngleBracketed(generics_in_struct) => {
                let generics_in_struct = &generics_in_struct.args;
                let params: syn::punctuated::Punctuated<_, _> = real
                    .generics
                    .params
                    .iter()
                    .filter(|generic| match generic {
                        syn::GenericParam::Type(ty) => generics_in_struct.iter().any(|g| match g {
                            syn::GenericArgument::Type(syn::Type::Path(type_path)) => {
                                type_path.path.is_ident(&ty.ident)
                            }
                            _ => false,
                        }),
                        syn::GenericParam::Lifetime(lt) => {
                            generics_in_struct.iter().any(|g| match g {
                                syn::GenericArgument::Lifetime(lifetime_in_struct) => {
                                    lt.lifetime == *lifetime_in_struct
                                }
                                _ => false,
                            })
                        }
                        syn::GenericParam::Const(_) => true,
                    })
                    .cloned()
                    .collect();
                syn::Generics {
                    params,
                    ..real.generics.clone()
                }
            }
            _ => syn::Generics::default(),
        };

        let (impl_generics, _, where_clause) = generics.split_for_impl();

        let whens = syn::parse_quote! {
            impl #impl_generics #morphed_ty #where_clause {
                #(#when_methods) *
            }
        };

        Ok(Mockable {
            real,
            morphed,
            whens,
            real_ty,
            morphed_ty,
        })
    }
}

impl From<Mockable> for proc_macro::TokenStream {
    fn from(mockable: Mockable) -> Self {
        let Mockable {
            real,
            morphed,
            whens,
            mut real_ty,
            morphed_ty,
        } = mockable;

        // create an identifier for the mod containing the real implementation
        // this is necessary until we are allowed to introduce type aliases within impl blocks
        let mod_ident = {
            let uuid = uuid::Uuid::new_v4();
            let ident = &real_ty.path.segments.last().unwrap().ident;
            syn::Ident::new(
                &match &real.trait_ {
                    None => format!("_faux_real_impl_{}_{}", ident, uuid.simple()),
                    Some((_, trait_, _)) => format!(
                        "_faux_real_impl_{}_{}_{}",
                        ident,
                        trait_.segments.last().unwrap().ident,
                        uuid.simple()
                    ),
                },
                proc_macro2::Span::call_site(),
            )
        };

        // make the original methods at least pub(super)
        // since they will be hidden in a nested mod
        let mut real = real;
        if real.trait_.is_none() {
            publicize_methods(&mut real);
        }
        let real = real;

        // creates an alias `type Foo = path::to::RealFoo` that may be wrapped inside some mods
        let alias = {
            let mut path_to_ty = morphed_ty.path.segments;
            let path_to_real_from_alias_mod = {
                // let mut real_ty = real_ty.clone();
                real_ty.path.segments.last_mut().unwrap().arguments = PathArguments::None;
                let first_path = &real_ty.path.segments.first().unwrap().ident;
                if *first_path == syn::Ident::new("crate", first_path.span()) {
                    // if it is an absolute position then no need to "super" up to find it
                    quote! { #real_ty }
                } else {
                    // otherwise do as many supers until you find the real struct definition
                    // one extra super for the nested mod
                    let supers = std::iter::repeat(quote! { super }).take(path_to_ty.len());
                    quote! { #(#supers::)*#real_ty }
                }
            };

            // type Foo = path::to::RealFoo
            let alias = {
                // the alias has to be public up to the mod wrapping it
                let pub_supers = if path_to_ty.len() < 2 {
                    quote! {}
                } else {
                    let supers = std::iter::repeat(quote! { super }).take(path_to_ty.len() - 1);
                    quote! { pub(#(#supers)::*) }
                };
                let pathless_type = path_to_ty.pop().unwrap();
                let ident = &pathless_type.value().ident;
                quote! {
                    //do not warn for things like Foo<i32> = RealFoo<i32>
                    #[allow(non_camel_case_types)]
                    #[allow(clippy::builtin_type_shadow)]
                    #pub_supers use #path_to_real_from_alias_mod as #ident;
                }
            };

            // nest the type alias in load-bearing mods
            path_to_ty.into_iter().fold(alias, |alias, segment| {
                quote! { mod #segment { #alias } }
            })
        };

        proc_macro::TokenStream::from(quote! {
            #morphed

            #whens

            mod #mod_ident {
                // make everything that was in-scope above also in-scope in this mod
                use super::*;

                #alias

                #real
            }
        })
    }
}

fn real_ty(morphed_ty: &syn::TypePath, path: Option<syn::Path>) -> syn::TypePath {
    let type_ident = &morphed_ty.path.segments.last().unwrap().ident;
    // combine a passed in path if given one
    // this will find the full path from the impl block to the morphed struct
    let mut path_to_morph_from_here = match path {
        None => morphed_ty.clone(),
        Some(mut path) => {
            let morphed_ty = morphed_ty.clone();
            path.segments.extend(morphed_ty.path.segments);
            syn::TypePath { path, ..morphed_ty }
        }
    };

    // now replace the last path segment with the original struct ident
    // this is now the path to the real struct from the impl block
    path_to_morph_from_here
        .path
        .segments
        .last_mut()
        .unwrap()
        .ident = create::real_struct_new_ident(type_ident);
    path_to_morph_from_here
}

// makes methods in this impl block be at least visible to super
fn publicize_methods(impl_block: &mut syn::ItemImpl) {
    impl_block
        .items
        .iter_mut()
        .filter_map(|item| match item {
            syn::ImplItem::Fn(m) => Some(m),
            _ => None,
        })
        .filter(|method| method.vis == syn::Visibility::Inherited)
        .for_each(|method| method.vis = syn::parse_quote! { pub(super) });
}

fn normalize_idents(signature: &mut syn::Signature) {
    signature
        .inputs
        .iter_mut()
        .filter_map(|a| match a {
            syn::FnArg::Receiver(_) => None,
            syn::FnArg::Typed(arg) => Some(arg.pat.as_mut()),
        })
        .enumerate()
        .for_each(|(i, arg_pat)| match arg_pat {
            syn::Pat::Ident(pat_ident) => {
                pat_ident.attrs = vec![];
                pat_ident.by_ref = None;
                pat_ident.mutability = None;
                pat_ident.subpat = None;
            }
            non_ident => {
                *non_ident = syn::Pat::Ident(syn::PatIdent {
                    attrs: vec![],
                    by_ref: None,
                    mutability: None,
                    subpat: None,
                    ident: syn::Ident::new(
                        &format!("_faux_arg_{i}"),
                        proc_macro2::Span::call_site(),
                    ),
                })
            }
        });
}

/// desugar_async turns async functions
/// into functions that return an async impl block
///
/// This allows us to track the relevant caller in case
/// mocks are used.
/// eg:
/// `async fn fetch(&self) -> u32`
///  becomes
/// `fn fetch(&self) -> impl Future<Output = i32> + use<'_>`
fn desugar_async(func: &mut ImplItemFn) {
    if func.sig.asyncness.take().is_none() {
        return;
    }

    let previous_return_type = &func.sig.output;
    let mut new_return_type = if let ReturnType::Type(_, ty) = previous_return_type {
        quote! { -> impl std::future::Future<Output = #ty> }
    } else {
        quote! { -> impl std::future::Future<Output = ()> }
    };

    if let Some(receiver) = func.sig.receiver() {
        if let Some((_, l)) = &receiver.reference {
            let lifetime = l
                .clone()
                .unwrap_or_else(|| Lifetime::new("'_", Span::call_site()));
            new_return_type.extend(quote! { + use< #lifetime >});
        }
    }

    func.sig.output = syn::parse_quote!(#new_return_type);
}
