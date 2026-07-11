use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{quote, ToTokens};
use std::collections::HashMap;
use syn::{
    parse_macro_input, Data, DeriveInput, Error, Field, Fields, GenericArgument, GenericParam,
    Ident, Lifetime, PathArguments, Result, Type, TypeReference,
};

/// Defines a named typed-query item from a struct of component references.
///
/// The struct must have exactly one lifetime and named fields. Fields accept
/// `&T`, `&mut T`, `Option<&T>`, and `Option<&mut T>` with that lifetime.
///
/// ```ignore
/// #[derive(QueryData)]
/// struct Movement<'w> {
///     position: &'w mut Position,
///     velocity: &'w Velocity,
/// }
///
/// world.query_mut::<Movement>().for_each(|item| {
///     item.position.x += item.velocity.x;
/// });
/// ```
#[proc_macro_derive(QueryData)]
pub fn derive_query_data(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_query_data(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// Defines a zero-state typed schedule-stage label.
#[proc_macro_derive(StageLabel)]
pub fn derive_stage_label(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_stage_label(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn expand_stage_label(input: DeriveInput) -> Result<TokenStream2> {
    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        return Err(Error::new_spanned(
            input.generics,
            "StageLabel does not support generic parameters",
        ));
    }
    match &input.data {
        Data::Struct(data) if matches!(data.fields, Fields::Unit) => {}
        _ => {
            return Err(Error::new_spanned(
                input.ident,
                "StageLabel requires a unit struct",
            ));
        }
    }
    let name = input.ident;
    let stage_label = stage_label_path();
    Ok(quote! {
        impl #stage_label for #name {}
    })
}

struct ParsedField {
    ident: Ident,
    static_ty: Type,
    component_ty: Type,
    mutable: bool,
}

fn expand_query_data(input: DeriveInput) -> Result<TokenStream2> {
    let support = query_support_path();
    let lifetime = query_lifetime(&input)?;
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return Err(Error::new_spanned(
                    &data.fields,
                    "QueryData requires a struct with named fields",
                ));
            }
        },
        Data::Enum(_) | Data::Union(_) => {
            return Err(Error::new_spanned(
                &input.ident,
                "QueryData can only be derived for structs",
            ));
        }
    };

    if fields.is_empty() {
        return Err(Error::new_spanned(
            &input.ident,
            "QueryData requires at least one component field",
        ));
    }
    if fields.len() > 16 {
        return Err(Error::new_spanned(
            fields,
            "typed queries support at most 16 component fields",
        ));
    }

    let parsed = fields
        .iter()
        .map(|field| parse_field(field, lifetime))
        .collect::<Result<Vec<_>>>()?;
    reject_syntactic_duplicates(&parsed)?;

    let name = &input.ident;
    let field_names = parsed.iter().map(|field| &field.ident).collect::<Vec<_>>();
    let field_indices = (0..parsed.len()).collect::<Vec<_>>();
    let static_types = parsed
        .iter()
        .map(|field| &field.static_ty)
        .collect::<Vec<_>>();
    let read_only = parsed.iter().all(|field| !field.mutable);

    let (raw_query, item_pattern) = if parsed.len() == 1 {
        let ty = static_types[0];
        let field = field_names[0];
        (quote!(#ty), quote!(#field))
    } else {
        (quote!((#(#static_types,)*)), quote!((#(#field_names,)*)))
    };

    let read_only_impl = read_only.then(|| {
        quote! {
            unsafe impl #support::ReadOnlyQuerySpec for #name<'static> {}
        }
    });

    Ok(quote! {
        unsafe impl #support::QuerySpec for #name<'static> {
            type Chunk<'__sky_world> =
                <#raw_query as #support::QuerySpec>::Chunk<'__sky_world>;
            type Item<'__sky_world> = #name<'__sky_world>;

            #[inline(always)]
            fn descriptor() -> #support::QueryDescriptor {
                <#raw_query as #support::QuerySpec>::descriptor()
            }

            #[inline(always)]
            unsafe fn chunk_from_raw<'__sky_world>(
                chunk: &'__sky_world #support::Chunk,
                component_indices: &[u8],
            ) -> Self::Chunk<'__sky_world> {
                unsafe {
                    <#raw_query as #support::QuerySpec>::chunk_from_raw(
                        chunk,
                        component_indices,
                    )
                }
            }

            #[inline(always)]
            unsafe fn chunk_from_raw_parts<'__sky_world>(
                component_ptrs: &[*mut u8],
                start: usize,
                len: usize,
            ) -> Self::Chunk<'__sky_world> {
                unsafe {
                    <#raw_query as #support::QuerySpec>::chunk_from_raw_parts(
                        component_ptrs,
                        start,
                        len,
                    )
                }
            }

            #[inline(always)]
            unsafe fn for_each_entity<'__sky_world, __SkyFunc>(
                chunk: &'__sky_world #support::Chunk,
                component_indices: &[u8],
                f: &mut __SkyFunc,
            )
            where
                __SkyFunc: FnMut(Self::Item<'__sky_world>),
            {
                unsafe {
                    <#raw_query as #support::QuerySpec>::for_each_entity(
                        chunk,
                        component_indices,
                        &mut |#item_pattern| f(#name { #(#field_names,)* }),
                    )
                }
            }

            #[inline(always)]
            unsafe fn for_each_entity_raw_parts<'__sky_world, __SkyFunc>(
                component_ptrs: &[*mut u8],
                start: usize,
                len: usize,
                f: &mut __SkyFunc,
            )
            where
                __SkyFunc: FnMut(Self::Item<'__sky_world>),
            {
                for entity_index in start..start + len {
                    f(unsafe {
                        #name {
                            #(
                                #field_names:
                                    <#static_types as #support::QueryParam>::item_from_raw(
                                        component_ptrs[#field_indices],
                                        entity_index,
                                    ),
                            )*
                        }
                    });
                }
            }
        }

        #read_only_impl
    })
}

fn query_lifetime(input: &DeriveInput) -> Result<&Lifetime> {
    if input.generics.where_clause.is_some() {
        return Err(Error::new_spanned(
            &input.generics,
            "QueryData does not support where clauses",
        ));
    }

    let mut lifetime = None;
    for parameter in &input.generics.params {
        match parameter {
            GenericParam::Lifetime(value) if lifetime.is_none() && value.bounds.is_empty() => {
                lifetime = Some(&value.lifetime);
            }
            GenericParam::Lifetime(value) if !value.bounds.is_empty() => {
                return Err(Error::new_spanned(
                    value,
                    "the QueryData lifetime cannot have bounds",
                ));
            }
            _ => {
                return Err(Error::new_spanned(
                    parameter,
                    "QueryData supports exactly one lifetime and no type or const parameters",
                ));
            }
        }
    }

    lifetime.ok_or_else(|| {
        Error::new_spanned(
            &input.generics,
            "QueryData requires exactly one lifetime parameter",
        )
    })
}

fn parse_field(field: &Field, query_lifetime: &Lifetime) -> Result<ParsedField> {
    let ident = field.ident.clone().expect("named fields checked above");
    let mut static_ty = field.ty.clone();
    let reference = query_reference(&field.ty).ok_or_else(|| {
        Error::new_spanned(
            &field.ty,
            "query fields must be &T, &mut T, Option<&T>, or Option<&mut T>",
        )
    })?;

    let Some(field_lifetime) = &reference.lifetime else {
        return Err(Error::new_spanned(
            reference,
            "query field references must use the struct lifetime",
        ));
    };
    if field_lifetime.ident != query_lifetime.ident {
        return Err(Error::new_spanned(
            field_lifetime,
            format!(
                "query field references must use the struct lifetime `{}`",
                query_lifetime.to_token_stream()
            ),
        ));
    }

    let component_ty = (*reference.elem).clone();
    let mutable = reference.mutability.is_some();
    query_reference_mut(&mut static_ty)
        .expect("the cloned field has the same supported shape")
        .lifetime = Some(Lifetime::new("'static", Span::call_site()));

    Ok(ParsedField {
        ident,
        static_ty,
        component_ty,
        mutable,
    })
}

fn query_reference(ty: &Type) -> Option<&TypeReference> {
    match ty {
        Type::Reference(reference) => Some(reference),
        Type::Path(path) if path.qself.is_none() => {
            let segment = path.path.segments.last()?;
            if segment.ident != "Option" {
                return None;
            }
            let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                return None;
            };
            if arguments.args.len() != 1 {
                return None;
            }
            match arguments.args.first()? {
                GenericArgument::Type(Type::Reference(reference)) => Some(reference),
                _ => None,
            }
        }
        _ => None,
    }
}

fn query_reference_mut(ty: &mut Type) -> Option<&mut TypeReference> {
    match ty {
        Type::Reference(reference) => Some(reference),
        Type::Path(path) if path.qself.is_none() => {
            let segment = path.path.segments.last_mut()?;
            if segment.ident != "Option" {
                return None;
            }
            let PathArguments::AngleBracketed(arguments) = &mut segment.arguments else {
                return None;
            };
            if arguments.args.len() != 1 {
                return None;
            }
            match arguments.args.first_mut()? {
                GenericArgument::Type(Type::Reference(reference)) => Some(reference),
                _ => None,
            }
        }
        _ => None,
    }
}

fn reject_syntactic_duplicates(fields: &[ParsedField]) -> Result<()> {
    let mut seen = HashMap::new();
    for field in fields {
        let key = field.component_ty.to_token_stream().to_string();
        if seen.insert(key.clone(), &field.ident).is_some() {
            return Err(Error::new_spanned(
                &field.component_ty,
                format!("duplicate query component type `{key}`"),
            ));
        }
    }
    Ok(())
}

fn query_support_path() -> TokenStream2 {
    if let Ok(found) = crate_name("sky_ecs") {
        let root = found_crate_path(found);
        return quote!(#root::ecs::__private);
    }
    if let Ok(found) = crate_name("sky_engine") {
        let root = found_crate_path(found);
        return quote!(#root::ecs::__private);
    }
    quote!(::sky_ecs::ecs::__private)
}

fn stage_label_path() -> TokenStream2 {
    if let Ok(found) = crate_name("sky_ecs") {
        let root = found_crate_path(found);
        return quote!(#root::StageLabel);
    }
    if let Ok(found) = crate_name("sky_engine") {
        let root = found_crate_path(found);
        return quote!(#root::ecs::StageLabel);
    }
    quote!(::sky_ecs::StageLabel)
}

fn found_crate_path(found: FoundCrate) -> TokenStream2 {
    match found {
        FoundCrate::Itself => quote!(::sky_ecs),
        FoundCrate::Name(name) => {
            let ident = Ident::new(&name, Span::call_site());
            quote!(::#ident)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn duplicate_component_fields_fail_during_derivation() {
        let input: DeriveInput = parse_quote! {
            struct Invalid<'w> {
                first: &'w Position,
                second: &'w Position,
            }
        };

        let error = expand_query_data(input).expect_err("duplicate fields must be rejected");
        assert!(error.to_string().contains("duplicate query component type"));
    }

    #[test]
    fn non_reference_fields_are_rejected() {
        let input: DeriveInput = parse_quote! {
            struct Invalid<'w> {
                value: Position,
                marker: &'w Marker,
            }
        };

        let error = expand_query_data(input).expect_err("owned fields must be rejected");
        assert!(error.to_string().contains("query fields must be"));
    }
}
