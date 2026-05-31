use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;
use syn::{
    parse_macro_input, parse_quote, Attribute, Data, DeriveInput, Error, Expr, Fields, Ident,
    LitStr, Meta, Result, Token,
};

use syn::punctuated::Punctuated;

#[proc_macro_derive(Reflect, attributes(reflect))]
pub fn derive_reflect(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_reflect(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

#[proc_macro_attribute]
pub fn persist(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let input = parse_macro_input!(input as DeriveInput);
    expand_persist(args, input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

#[derive(Default)]
struct PersistContainerAttrs {
    component: bool,
    name: Option<LitStr>,
}

fn expand_persist(
    args: Punctuated<Meta, Token![,]>,
    mut input: DeriveInput,
) -> Result<TokenStream2> {
    let attrs = parse_persist_container_attrs(args)?;
    if !attrs.component {
        return Err(Error::new_spanned(
            input.ident,
            "#[persist] currently requires the component mode: #[persist(component)]",
        ));
    }
    if !input.generics.params.is_empty() {
        return Err(Error::new_spanned(
            input.generics,
            "#[persist(component)] does not support generic component types yet",
        ));
    }

    match &mut input.data {
        Data::Struct(data) => rewrite_persist_field_attrs(&mut data.fields)?,
        Data::Enum(_) => {}
        Data::Union(_) => {
            return Err(Error::new_spanned(
                input.ident,
                "#[persist(component)] does not support unions",
            ));
        }
    }

    input
        .attrs
        .push(parse_quote!(#[derive(::serde::Serialize, ::serde::Deserialize)]));

    let ident = &input.ident;
    let short_name = ident.to_string();
    let name = if let Some(name) = attrs.name {
        quote!(::std::option::Option::Some(#name))
    } else {
        quote!(::std::option::Option::None)
    };

    Ok(quote! {
        #input

        impl ::sky_engine::scene::Persist for #ident {
            const SHORT_NAME: &'static str = #short_name;
            const NAME: ::std::option::Option<&'static str> = #name;
        }

        ::sky_engine::scene::__private::inventory::submit! {
            ::sky_engine::scene::PersistRegistration::component::<#ident>()
        }
    })
}

fn parse_persist_container_attrs(
    args: Punctuated<Meta, Token![,]>,
) -> Result<PersistContainerAttrs> {
    let mut out = PersistContainerAttrs::default();
    for meta in args {
        match meta {
            Meta::Path(path) if path.is_ident("component") => {
                out.component = true;
            }
            Meta::NameValue(name_value) if name_value.path.is_ident("name") => {
                let Expr::Lit(expr_lit) = name_value.value else {
                    return Err(Error::new_spanned(
                        name_value.value,
                        "expected string literal",
                    ));
                };
                let syn::Lit::Str(value) = expr_lit.lit else {
                    return Err(Error::new_spanned(expr_lit, "expected string literal"));
                };
                out.name = Some(value);
            }
            other => {
                return Err(Error::new_spanned(
                    other,
                    "unsupported #[persist(...)] attribute",
                ));
            }
        }
    }
    Ok(out)
}

#[derive(Default)]
struct PersistFieldAttrs {
    skip: bool,
    default: bool,
}

fn rewrite_persist_field_attrs(fields: &mut Fields) -> Result<()> {
    for field in fields.iter_mut() {
        let mut persist_attrs = PersistFieldAttrs::default();
        let mut retained = Vec::with_capacity(field.attrs.len());

        for attr in std::mem::take(&mut field.attrs) {
            if attr.path().is_ident("persist") {
                parse_persist_field_attr(&attr, &mut persist_attrs)?;
            } else {
                retained.push(attr);
            }
        }

        if persist_attrs.skip {
            retained.push(parse_quote!(#[serde(skip)]));
        }
        if persist_attrs.default {
            retained.push(parse_quote!(#[serde(default)]));
        }

        field.attrs = retained;
    }

    Ok(())
}

fn parse_persist_field_attr(attr: &Attribute, out: &mut PersistFieldAttrs) -> Result<()> {
    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("skip") {
            out.skip = true;
            return Ok(());
        }
        if meta.path.is_ident("default") {
            out.default = true;
            return Ok(());
        }
        Err(meta.error("unsupported #[persist(...)] field attribute"))
    })
}

fn expand_reflect(input: DeriveInput) -> Result<TokenStream2> {
    let reflect_path = reflect_crate_path();
    match &input.data {
        Data::Struct(data) => expand_struct(&input, data, &reflect_path),
        Data::Enum(data) => expand_enum(&input, data, &reflect_path),
        Data::Union(_) => Err(Error::new_spanned(
            input.ident,
            "Reflect derive does not support unions",
        )),
    }
}

fn reflect_crate_path() -> TokenStream2 {
    match crate_name("sky_reflect") {
        Ok(found) => crate_root_path(found),
        Err(_) => match crate_name("sky_engine") {
            Ok(found) => {
                let root = crate_root_path(found);
                quote!(#root::reflect)
            }
            Err(_) => quote!(::sky_engine::reflect),
        },
    }
}

fn crate_root_path(found: FoundCrate) -> TokenStream2 {
    match found {
        FoundCrate::Itself => quote!(crate),
        FoundCrate::Name(name) => {
            let ident = Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#ident)
        }
    }
}

#[derive(Default)]
struct ReflectAttrs {
    name: Option<LitStr>,
    skip: bool,
    readonly: bool,
    label: Option<LitStr>,
    category: Option<LitStr>,
    min: Option<Expr>,
    max: Option<Expr>,
    step: Option<Expr>,
}

fn parse_reflect_attrs(attrs: &[Attribute]) -> Result<ReflectAttrs> {
    let mut out = ReflectAttrs::default();

    for attr in attrs.iter().filter(|attr| attr.path().is_ident("reflect")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                out.name = Some(meta.value()?.parse()?);
                return Ok(());
            }
            if meta.path.is_ident("skip") {
                out.skip = true;
                return Ok(());
            }
            if meta.path.is_ident("readonly") {
                out.readonly = true;
                return Ok(());
            }
            if meta.path.is_ident("label") {
                out.label = Some(meta.value()?.parse()?);
                return Ok(());
            }
            if meta.path.is_ident("category") {
                out.category = Some(meta.value()?.parse()?);
                return Ok(());
            }
            if meta.path.is_ident("min") {
                out.min = Some(meta.value()?.parse()?);
                return Ok(());
            }
            if meta.path.is_ident("max") {
                out.max = Some(meta.value()?.parse()?);
                return Ok(());
            }
            if meta.path.is_ident("step") {
                out.step = Some(meta.value()?.parse()?);
                return Ok(());
            }

            Err(meta.error("unsupported #[reflect(...)] attribute"))
        })?;
    }

    Ok(out)
}

fn type_path_expr(attrs: &ReflectAttrs) -> TokenStream2 {
    if let Some(name) = &attrs.name {
        quote!(#name)
    } else {
        quote!(::std::any::type_name::<Self>())
    }
}

fn attrs_expr(attrs: &ReflectAttrs, reflect_path: &TokenStream2) -> TokenStream2 {
    let label = option_string(&attrs.label);
    let category = option_string(&attrs.category);
    let readonly = attrs.readonly;
    let min = option_f64(&attrs.min);
    let max = option_f64(&attrs.max);
    let step = option_f64(&attrs.step);

    quote! {
        #reflect_path::ReflectAttrs {
            label: #label,
            readonly: #readonly,
            min: #min,
            max: #max,
            step: #step,
            category: #category,
        }
    }
}

fn option_string(value: &Option<LitStr>) -> TokenStream2 {
    if let Some(value) = value {
        quote!(::std::option::Option::Some(#value.to_string()))
    } else {
        quote!(::std::option::Option::None)
    }
}

fn option_f64(value: &Option<Expr>) -> TokenStream2 {
    if let Some(value) = value {
        quote!(::std::option::Option::Some((#value) as f64))
    } else {
        quote!(::std::option::Option::None)
    }
}

struct ReflectedField<'a> {
    ident: &'a Ident,
    name: String,
    ty: &'a syn::Type,
    attrs: ReflectAttrs,
}

fn expand_struct(
    input: &DeriveInput,
    data: &syn::DataStruct,
    reflect_path: &TokenStream2,
) -> Result<TokenStream2> {
    let ident = &input.ident;
    let container_attrs = parse_reflect_attrs(&input.attrs)?;
    let type_path = type_path_expr(&container_attrs);

    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new_spanned(
            ident,
            "Reflect derive currently supports named structs",
        ));
    };

    let mut reflected = Vec::new();
    for field in &fields.named {
        let attrs = parse_reflect_attrs(&field.attrs)?;
        if attrs.skip {
            continue;
        }
        let Some(field_ident) = &field.ident else {
            continue;
        };
        reflected.push(ReflectedField {
            ident: field_ident,
            name: field_ident.to_string().trim_start_matches("r#").to_string(),
            ty: &field.ty,
            attrs,
        });
    }

    let mut generics = input.generics.clone();
    {
        let where_clause = generics.make_where_clause();
        for field in &reflected {
            let ty = field.ty;
            where_clause
                .predicates
                .push(parse_quote!(#ty: #reflect_path::Reflect));
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let field_builders = reflected.iter().map(|field| {
        let field_ident = field.ident;
        let field_name = &field.name;
        let field_ty = field.ty;
        let attrs = attrs_expr(&field.attrs, reflect_path);
        quote! {
            #reflect_path::ReflectField::new_raw::<Self, #field_ty>(
                #field_name,
                #attrs,
                |owner: &dyn ::std::any::Any| {
                    let owner = owner.downcast_ref::<Self>().ok_or_else(|| {
                        #reflect_path::ReflectError::OwnerTypeMismatch {
                            expected: ::std::any::type_name::<Self>().to_string(),
                            actual: #reflect_path::type_name_of_any(owner).to_string(),
                        }
                    })?;
                    <#field_ty as #reflect_path::Reflect>::to_reflect_value(&owner.#field_ident)
                },
                |owner: &mut dyn ::std::any::Any, value: #reflect_path::ReflectValue| {
                    let actual = #reflect_path::type_name_of_any_mut(owner).to_string();
                    let owner = owner.downcast_mut::<Self>().ok_or_else(|| {
                        #reflect_path::ReflectError::OwnerTypeMismatch {
                            expected: ::std::any::type_name::<Self>().to_string(),
                            actual,
                        }
                    })?;
                    <#field_ty as #reflect_path::Reflect>::apply_reflect_value(
                        &mut owner.#field_ident,
                        value,
                    )
                },
                |owner: &dyn ::std::any::Any| {
                    let owner = owner.downcast_ref::<Self>().ok_or_else(|| {
                        #reflect_path::ReflectError::OwnerTypeMismatch {
                            expected: ::std::any::type_name::<Self>().to_string(),
                            actual: #reflect_path::type_name_of_any(owner).to_string(),
                        }
                    })?;
                    Ok(&owner.#field_ident as &dyn ::std::any::Any)
                },
                |owner: &mut dyn ::std::any::Any| {
                    let actual = #reflect_path::type_name_of_any_mut(owner).to_string();
                    let owner = owner.downcast_mut::<Self>().ok_or_else(|| {
                        #reflect_path::ReflectError::OwnerTypeMismatch {
                            expected: ::std::any::type_name::<Self>().to_string(),
                            actual,
                        }
                    })?;
                    Ok(&mut owner.#field_ident as &mut dyn ::std::any::Any)
                },
            )
        }
    });

    let dependency_registers = reflected.iter().map(|field| {
        let field_ty = field.ty;
        quote!(registry.register::<#field_ty>()?;)
    });

    let value_fields = reflected.iter().map(|field| {
        let field_ident = field.ident;
        let field_name = &field.name;
        let field_ty = field.ty;
        quote! {
            value.insert(
                #field_name,
                <#field_ty as #reflect_path::Reflect>::to_reflect_value(&self.#field_ident)?,
            );
        }
    });

    let apply_fields = reflected.iter().map(|field| {
        let field_ident = field.ident;
        let field_name = &field.name;
        let field_ty = field.ty;
        let readonly = field.attrs.readonly;
        quote! {
            #field_name => {
                if #readonly {
                    return Err(#reflect_path::ReflectError::ReadonlyField {
                        field: #field_name.to_string(),
                    });
                }
                <#field_ty as #reflect_path::Reflect>::apply_reflect_value(
                    &mut self.#field_ident,
                    field.value().clone(),
                )?;
            }
        }
    });

    Ok(quote! {
        impl #impl_generics #reflect_path::Reflect for #ident #ty_generics #where_clause {
            fn reflect_type() -> #reflect_path::ReflectType {
                #reflect_path::ReflectType::new_struct::<Self>(
                    #type_path,
                    vec![#(#field_builders),*],
                )
            }

            fn reflect_dependencies(
                registry: &mut #reflect_path::ReflectRegistry,
            ) -> ::std::result::Result<(), #reflect_path::ReflectError> {
                #(#dependency_registers)*
                Ok(())
            }

            fn to_reflect_value(
                &self,
            ) -> ::std::result::Result<
                #reflect_path::ReflectValue,
                #reflect_path::ReflectError,
            > {
                let mut value = #reflect_path::ReflectStructValue::new(#type_path);
                #(#value_fields)*
                Ok(#reflect_path::ReflectValue::Struct(value))
            }

            fn apply_reflect_value(
                &mut self,
                value: #reflect_path::ReflectValue,
            ) -> ::std::result::Result<(), #reflect_path::ReflectError> {
                let value = match value {
                    #reflect_path::ReflectValue::Struct(value) => value,
                    other => {
                        return Err(#reflect_path::ReflectError::ValueTypeMismatch {
                            expected: "Struct",
                            actual: other.kind_name(),
                        });
                    }
                };

                let expected = #type_path;
                if !value.type_name().is_empty() && value.type_name() != expected {
                    return Err(#reflect_path::ReflectError::StructTypeMismatch {
                        expected: expected.to_string(),
                        actual: value.type_name().to_string(),
                    });
                }

                for field in value.fields() {
                    match field.name() {
                        #(#apply_fields)*
                        name => {
                            return Err(#reflect_path::ReflectError::UnknownField {
                                type_name: expected.to_string(),
                                field: name.to_string(),
                            });
                        }
                    }
                }

                Ok(())
            }
        }
    })
}

fn expand_enum(
    input: &DeriveInput,
    data: &syn::DataEnum,
    reflect_path: &TokenStream2,
) -> Result<TokenStream2> {
    let ident = &input.ident;
    let container_attrs = parse_reflect_attrs(&input.attrs)?;
    let type_path = type_path_expr(&container_attrs);

    let mut generics = input.generics.clone();
    let where_clause = generics.make_where_clause();
    for variant in &data.variants {
        for field in &variant.fields {
            let ty = &field.ty;
            where_clause
                .predicates
                .push(parse_quote!(#ty: #reflect_path::Reflect));
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let variant_infos = data.variants.iter().map(|variant| {
        let name = variant.ident.to_string();
        let kind = match &variant.fields {
            Fields::Unit => quote!(#reflect_path::ReflectVariantKind::Unit),
            Fields::Unnamed(fields) => {
                let len = fields.unnamed.len();
                quote!(#reflect_path::ReflectVariantKind::Tuple { fields: #len })
            }
            Fields::Named(fields) => {
                let names = fields.named.iter().map(|field| {
                    field
                        .ident
                        .as_ref()
                        .map(|ident| ident.to_string())
                        .unwrap_or_default()
                });
                quote!(#reflect_path::ReflectVariantKind::Struct {
                    fields: vec![#(#names.to_string()),*],
                })
            }
        };
        quote!(#reflect_path::ReflectVariant::new(#name, #kind))
    });

    let read_arms = data.variants.iter().map(|variant| {
        let variant_ident = &variant.ident;
        let variant_name = variant_ident.to_string();
        match &variant.fields {
            Fields::Unit => quote!(Self::#variant_ident => #variant_name),
            Fields::Unnamed(_) => quote!(Self::#variant_ident(..) => #variant_name),
            Fields::Named(_) => quote!(Self::#variant_ident { .. } => #variant_name),
        }
    });

    let unit_write_arms = data.variants.iter().filter_map(|variant| {
        let variant_ident = &variant.ident;
        let variant_name = variant_ident.to_string();
        matches!(variant.fields, Fields::Unit)
            .then(|| quote!(#variant_name => *self = Self::#variant_ident,))
    });

    let dependency_registers = data.variants.iter().flat_map(|variant| {
        variant.fields.iter().map(|field| {
            let ty = &field.ty;
            quote!(registry.register::<#ty>()?;)
        })
    });

    Ok(quote! {
        impl #impl_generics #reflect_path::Reflect for #ident #ty_generics #where_clause {
            fn reflect_type() -> #reflect_path::ReflectType {
                #reflect_path::ReflectType::new_enum::<Self>(
                    #type_path,
                    vec![#(#variant_infos),*],
                )
            }

            fn reflect_dependencies(
                registry: &mut #reflect_path::ReflectRegistry,
            ) -> ::std::result::Result<(), #reflect_path::ReflectError> {
                #(#dependency_registers)*
                Ok(())
            }

            fn to_reflect_value(
                &self,
            ) -> ::std::result::Result<
                #reflect_path::ReflectValue,
                #reflect_path::ReflectError,
            > {
                let variant = match self {
                    #(#read_arms,)*
                };
                Ok(#reflect_path::ReflectValue::Enum(
                    #reflect_path::ReflectEnumValue::new(#type_path, variant)
                ))
            }

            fn apply_reflect_value(
                &mut self,
                value: #reflect_path::ReflectValue,
            ) -> ::std::result::Result<(), #reflect_path::ReflectError> {
                let value = match value {
                    #reflect_path::ReflectValue::Enum(value) => value,
                    other => {
                        return Err(#reflect_path::ReflectError::ValueTypeMismatch {
                            expected: "Enum",
                            actual: other.kind_name(),
                        });
                    }
                };

                let expected = #type_path;
                if !value.type_name().is_empty() && value.type_name() != expected {
                    return Err(#reflect_path::ReflectError::EnumTypeMismatch {
                        expected: expected.to_string(),
                        actual: value.type_name().to_string(),
                    });
                }

                match value.variant() {
                    #(#unit_write_arms)*
                    variant => {
                        return Err(#reflect_path::ReflectError::Unsupported {
                            message: format!(
                                "enum variant '{}::{}' cannot be written by Reflect v1",
                                expected,
                                variant,
                            ),
                        });
                    }
                }
                Ok(())
            }
        }
    })
}
