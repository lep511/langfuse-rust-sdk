//! The `#[observe]` attribute of `langfuse-rust-sdk`.
//!
//! Use it through `langfuse_rust_sdk::observe` (feature `macros`), whose
//! documentation describes it; the code it generates relies on
//! `langfuse_rust_sdk::otel::__observe`.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote, quote_spanned};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    FnArg, GenericArgument, Ident, ItemFn, LitBool, LitStr, Pat, Path, PathArguments, ReturnType,
    Token, Type, TypeParamBound, parse_quote,
};

/// Observation types `as_type` accepts, with their `ObservationType` variants.
const TYPES: &[(&str, &str)] = &[
    ("span", "Span"),
    ("generation", "Generation"),
    ("agent", "Agent"),
    ("tool", "Tool"),
    ("chain", "Chain"),
    ("retriever", "Retriever"),
    ("embedding", "Embedding"),
    ("evaluator", "Evaluator"),
    ("guardrail", "Guardrail"),
];

/// Trace a function as a Langfuse observation; see `langfuse_rust_sdk::observe`.
#[proc_macro_attribute]
pub fn observe(attr: TokenStream, item: TokenStream) -> TokenStream {
    match expand(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// Parsed attribute arguments.
struct Args {
    name: Option<LitStr>,
    kind: Ident,
    capture_input: Option<bool>,
    capture_output: Option<bool>,
    transform: Option<Path>,
    skip: Vec<Ident>,
    krate: Path,
}

fn parse_args(attr: TokenStream2) -> syn::Result<Args> {
    let mut args = Args {
        name: None,
        kind: Ident::new("Span", Span::call_site()),
        capture_input: None,
        capture_output: None,
        transform: None,
        skip: Vec::new(),
        krate: parse_quote!(::langfuse_rust_sdk),
    };
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("name") {
            args.name = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("as_type") {
            let value: LitStr = meta.value()?.parse()?;
            let requested = value.value();
            match TYPES.iter().find(|(name, _)| *name == requested) {
                Some((_, variant)) => args.kind = Ident::new(variant, value.span()),
                None => {
                    let valid: Vec<&str> = TYPES.iter().map(|(name, _)| *name).collect();
                    let hint = if requested == "event" {
                        " (events have no duration: create them with `ObservationType::Event`)"
                    } else {
                        ""
                    };
                    return Err(syn::Error::new(
                        value.span(),
                        format!(
                            "invalid as_type \"{requested}\"{hint}; expected one of: {}",
                            valid.join(", ")
                        ),
                    ));
                }
            }
        } else if meta.path.is_ident("capture_input") {
            args.capture_input = Some(meta.value()?.parse::<LitBool>()?.value);
        } else if meta.path.is_ident("capture_output") {
            args.capture_output = Some(meta.value()?.parse::<LitBool>()?.value);
        } else if meta.path.is_ident("transform_to_string") {
            args.transform = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("skip") {
            let content;
            syn::parenthesized!(content in meta.input);
            let names = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?;
            args.skip.extend(names);
        } else if meta.path.is_ident("crate") {
            let value: LitStr = meta.value()?.parse()?;
            args.krate = value.parse()?;
        } else {
            return Err(meta.error(
                "unknown #[observe] option; expected name, as_type, capture_input, \
                 capture_output, transform_to_string, skip or crate",
            ));
        }
        Ok(())
    });
    parser.parse2(attr)?;
    Ok(args)
}

/// What the function returns, for output handling.
enum Returns {
    /// A value (or `Result`), recorded when the call ends.
    Value,
    /// `impl Stream<Item = T>`: the observation follows the stream.
    Stream(Type),
    /// `impl Iterator<Item = T>`: the observation follows the iterator.
    Iterator(Type),
}

/// Recognize `impl Stream<Item = T>` / `impl Iterator<Item = T>`, with only
/// auto-trait and lifetime bounds besides.
fn classify(output: &ReturnType) -> Returns {
    let ReturnType::Type(_, ty) = output else {
        return Returns::Value;
    };
    let Type::ImplTrait(impl_trait) = &**ty else {
        return Returns::Value;
    };
    let mut found = None;
    for bound in &impl_trait.bounds {
        let TypeParamBound::Trait(trait_bound) = bound else {
            continue; // lifetimes
        };
        let Some(last) = trait_bound.path.segments.last() else {
            return Returns::Value;
        };
        match last.ident.to_string().as_str() {
            "Send" | "Sync" | "Unpin" => {}
            kind @ ("Stream" | "Iterator") if found.is_none() => {
                let PathArguments::AngleBracketed(arguments) = &last.arguments else {
                    return Returns::Value;
                };
                let item = arguments.args.iter().find_map(|argument| match argument {
                    GenericArgument::AssocType(assoc) if assoc.ident == "Item" => {
                        Some(assoc.ty.clone())
                    }
                    _ => None,
                });
                let Some(item) = item else {
                    return Returns::Value;
                };
                found = Some(if kind == "Stream" {
                    Returns::Stream(item)
                } else {
                    Returns::Iterator(item)
                });
            }
            _ => return Returns::Value,
        }
    }
    found.unwrap_or(Returns::Value)
}

/// Whether `ty` can be written as the type of a `let` (no `impl Trait`).
fn is_nameable(ty: &Type) -> bool {
    fn has_impl(tokens: TokenStream2) -> bool {
        tokens.into_iter().any(|tree| match tree {
            proc_macro2::TokenTree::Ident(ident) => ident == "impl",
            proc_macro2::TokenTree::Group(group) => has_impl(group.stream()),
            _ => false,
        })
    }
    !has_impl(quote!(#ty))
}

fn expand(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let args = parse_args(attr)?;
    let mut function: ItemFn = syn::parse2(item).map_err(|error| {
        syn::Error::new(
            error.span(),
            "#[observe] applies to functions and methods with a body",
        )
    })?;
    if let Some(constness) = &function.sig.constness {
        return Err(syn::Error::new(
            constness.span(),
            "#[observe] cannot trace a const fn",
        ));
    }
    let krate = &args.krate;
    let rt = quote!(#krate::otel::__observe);

    // Input: one entry per parameter, by name; `self` and `skip`ped ones
    // excluded. Non-identifier patterns are bound inside the body.
    let mut captured = Vec::new();
    let mut rebinds = Vec::new();
    let mut known = Vec::new();
    for (index, input) in function.sig.inputs.iter_mut().enumerate() {
        let FnArg::Typed(typed) = input else {
            continue; // the receiver
        };
        match &*typed.pat {
            Pat::Ident(pat) => {
                let ident = pat.ident.clone();
                known.push(ident.clone());
                if args.skip.contains(&ident) {
                    continue;
                }
                let key = ident.to_string();
                let key = key.strip_prefix("r#").unwrap_or(&key).to_owned();
                captured.push((key, quote!(#ident)));
            }
            pattern => {
                let key = quote!(#pattern).to_string();
                let ident = format_ident!("__lf_arg{}", index);
                let pattern = pattern.clone();
                rebinds.push(quote!(let #pattern = #ident;));
                captured.push((key, quote!(#ident)));
                *typed.pat = parse_quote!(#ident);
            }
        }
    }
    for skipped in &args.skip {
        if !known.contains(skipped) {
            return Err(syn::Error::new(
                skipped.span(),
                format!("#[observe] skip: no parameter named `{skipped}`"),
            ));
        }
    }

    let name = match &args.name {
        Some(name) => quote!(#name),
        None => {
            let name = function.sig.ident.to_string();
            let name = name.strip_prefix("r#").unwrap_or(&name).to_owned();
            quote!(#name)
        }
    };
    let kind = &args.kind;
    let option = |value: Option<bool>| match value {
        Some(value) => quote!(::core::option::Option::Some(#value)),
        None => quote!(::core::option::Option::None),
    };
    let capture_input = option(args.capture_input);
    let capture_output = option(args.capture_output);
    let input = if captured.is_empty() {
        quote!(::core::option::Option::None)
    } else {
        let entries = captured.iter().map(|(key, value)| {
            quote! {
                __lf_input.insert(
                    ::std::string::String::from(#key),
                    (&&#rt::Wrap(&#value)).__lf_capture(),
                );
            }
        });
        quote! {{
            #[allow(unused_imports)]
            use #rt::{CaptureAny as _, CaptureSerialize as _};
            let mut __lf_input = #rt::Map::new();
            #(#entries)*
            ::core::option::Option::Some(#rt::Value::Object(__lf_input))
        }}
    };

    let returns = classify(&function.sig.output);
    if args.transform.is_some() && matches!(returns, Returns::Value) {
        return Err(syn::Error::new(
            args.transform.span(),
            "#[observe] transform_to_string needs a function returning `impl Stream<Item = T>` \
             or `impl Iterator<Item = T>`",
        ));
    }
    let transform = match &args.transform {
        Some(path) => quote! {
            ::core::option::Option::Some(#path as fn(&[#rt::Value]) -> ::std::string::String)
        },
        None => quote!(::core::option::Option::None),
    };
    let outcome_traits = quote! {
        #[allow(unused_imports)]
        use #rt::{OutcomeAny as _, OutcomeResult as _, OutcomeResultSerialize as _, OutcomeSerialize as _};
    };
    let item_capture = |item: &Type| {
        quote! {
            |__lf_item: &#item, __lf_capture: bool| {
                #outcome_traits
                (&&&&#rt::Wrap(__lf_item)).__lf_outcome(__lf_capture)
            }
        }
    };
    let finish = match &returns {
        Returns::Value => quote! {{
            __lf_observer.finish(|__lf_capture| {
                #outcome_traits
                (&&&&#rt::Wrap(&__lf_output)).__lf_outcome(__lf_capture)
            });
            __lf_output
        }},
        Returns::Stream(item) => {
            let capture = item_capture(item);
            quote!(__lf_observer.wrap_stream(__lf_output, #capture, #transform))
        }
        Returns::Iterator(item) => {
            let capture = item_capture(item);
            quote!(__lf_observer.wrap_iter(__lf_output, #capture, #transform))
        }
    };

    let output_type = match &function.sig.output {
        ReturnType::Default => Some(quote!(())),
        ReturnType::Type(_, ty) if is_nameable(ty) => Some(quote!(#ty)),
        ReturnType::Type(..) => None,
    };
    let block = &function.block;
    let statements = &block.stmts;
    let start = quote! {
        let __lf_observer = #rt::Observer::start(
            #name,
            #krate::otel::ObservationType::#kind,
            #capture_input,
            #capture_output,
            || #input,
        );
    };
    let body = if function.sig.asyncness.is_some() {
        // Pins the type of the async block to the return type, so `?` and
        // `return` infer like in the original function.
        let fake_return = output_type.map(|ty| {
            quote! {
                #[allow(unreachable_code, clippy::diverging_sub_expression, clippy::empty_loop, clippy::let_unit_value)]
                if false {
                    let __lf_fake_return: #ty = loop {};
                    return __lf_fake_return;
                }
            }
        });
        quote_spanned! {block.span()=>
            #start
            #[allow(clippy::let_unit_value)]
            let __lf_output = __lf_observer
                .scope(async move {
                    #fake_return
                    #(#rebinds)*
                    #(#statements)*
                })
                .await;
            #finish
        }
    } else {
        let annotation = output_type.map(|ty| quote!(-> #ty));
        quote_spanned! {block.span()=>
            #start
            #[allow(clippy::let_unit_value)]
            let __lf_output = {
                let __lf_guard = __lf_observer.enter();
                #[allow(clippy::redundant_closure_call)]
                (|| #annotation {
                    #(#rebinds)*
                    #(#statements)*
                })()
            };
            #finish
        }
    };
    *function.block = parse_quote!({ #body });
    Ok(quote!(#function))
}
