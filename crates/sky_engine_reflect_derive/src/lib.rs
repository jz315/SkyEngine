use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse_macro_input, parse_quote, Attribute, Data, DeriveInput, Error, Expr, Fields, Ident,
    LitStr, Result,
};

#[proc_macro_derive(Reflect, attributes(reflect))]
pub fn derive_reflect(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_reflect(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn expand_reflect(input: DeriveInput) -> Result<TokenStream2> {
    match &input.data {
        Data::Struct(data) => expand_struct(&input, data),
        Data::Enum(data) => expand_enum(&input, data),
        Data::Union(_) => Err(Error::new_spanned(
            input.ident,
            "Reflect derive does not support unions",
        )),
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

fn attrs_expr(attrs: &ReflectAttrs) -> TokenStream2 {
    let label = option_string(&attrs.label);
    let category = option_string(&attrs.category);
    let readonly = attrs.readonly;
    let min = option_f64(&attrs.min);
    let max = option_f64(&attrs.max);
    let step = option_f64(&attrs.step);

    quote! {
        ::sky_engine::reflect::ReflectAttrs {
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

fn expand_struct(input: &DeriveInput, data: &syn::DataStruct) -> Result<TokenStream2> {
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
                .push(parse_quote!(#ty: ::sky_engine::reflect::Reflect));
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let field_builders = reflected.iter().map(|field| {
        let field_ident = field.ident;
        let field_name = &field.name;
        let field_ty = field.ty;
        let attrs = attrs_expr(&field.attrs);
        quote! {
            ::sky_engine::reflect::ReflectField::new_raw::<Self, #field_ty>(
                #field_name,
                #attrs,
                |owner: &dyn ::std::any::Any| {
                    let owner = owner.downcast_ref::<Self>().ok_or_else(|| {
                        ::sky_engine::reflect::ReflectError::OwnerTypeMismatch {
                            expected: ::std::any::type_name::<Self>().to_string(),
                            actual: ::sky_engine::reflect::type_name_of_any(owner).to_string(),
                        }
                    })?;
                    <#field_ty as ::sky_engine::reflect::Reflect>::to_reflect_value(&owner.#field_ident)
                },
                |owner: &mut dyn ::std::any::Any, value: ::sky_engine::reflect::ReflectValue| {
                    let actual = ::sky_engine::reflect::type_name_of_any_mut(owner).to_string();
                    let owner = owner.downcast_mut::<Self>().ok_or_else(|| {
                        ::sky_engine::reflect::ReflectError::OwnerTypeMismatch {
                            expected: ::std::any::type_name::<Self>().to_string(),
                            actual,
                        }
                    })?;
                    <#field_ty as ::sky_engine::reflect::Reflect>::apply_reflect_value(
                        &mut owner.#field_ident,
                        value,
                    )
                },
                |owner: &dyn ::std::any::Any| {
                    let owner = owner.downcast_ref::<Self>().ok_or_else(|| {
                        ::sky_engine::reflect::ReflectError::OwnerTypeMismatch {
                            expected: ::std::any::type_name::<Self>().to_string(),
                            actual: ::sky_engine::reflect::type_name_of_any(owner).to_string(),
                        }
                    })?;
                    Ok(&owner.#field_ident as &dyn ::std::any::Any)
                },
                |owner: &mut dyn ::std::any::Any| {
                    let actual = ::sky_engine::reflect::type_name_of_any_mut(owner).to_string();
                    let owner = owner.downcast_mut::<Self>().ok_or_else(|| {
                        ::sky_engine::reflect::ReflectError::OwnerTypeMismatch {
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
                <#field_ty as ::sky_engine::reflect::Reflect>::to_reflect_value(&self.#field_ident)?,
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
                    return Err(::sky_engine::reflect::ReflectError::ReadonlyField {
                        field: #field_name.to_string(),
                    });
                }
                <#field_ty as ::sky_engine::reflect::Reflect>::apply_reflect_value(
                    &mut self.#field_ident,
                    field.value().clone(),
                )?;
            }
        }
    });

    Ok(quote! {
        impl #impl_generics ::sky_engine::reflect::Reflect for #ident #ty_generics #where_clause {
            fn reflect_type() -> ::sky_engine::reflect::ReflectType {
                ::sky_engine::reflect::ReflectType::new_struct::<Self>(
                    #type_path,
                    vec![#(#field_builders),*],
                )
            }

            fn reflect_dependencies(
                registry: &mut ::sky_engine::reflect::ReflectRegistry,
            ) -> ::std::result::Result<(), ::sky_engine::reflect::ReflectError> {
                #(#dependency_registers)*
                Ok(())
            }

            fn to_reflect_value(
                &self,
            ) -> ::std::result::Result<
                ::sky_engine::reflect::ReflectValue,
                ::sky_engine::reflect::ReflectError,
            > {
                let mut value = ::sky_engine::reflect::ReflectStructValue::new(#type_path);
                #(#value_fields)*
                Ok(::sky_engine::reflect::ReflectValue::Struct(value))
            }

            fn apply_reflect_value(
                &mut self,
                value: ::sky_engine::reflect::ReflectValue,
            ) -> ::std::result::Result<(), ::sky_engine::reflect::ReflectError> {
                let value = match value {
                    ::sky_engine::reflect::ReflectValue::Struct(value) => value,
                    other => {
                        return Err(::sky_engine::reflect::ReflectError::ValueTypeMismatch {
                            expected: "Struct",
                            actual: other.kind_name(),
                        });
                    }
                };

                let expected = #type_path;
                if !value.type_name().is_empty() && value.type_name() != expected {
                    return Err(::sky_engine::reflect::ReflectError::StructTypeMismatch {
                        expected: expected.to_string(),
                        actual: value.type_name().to_string(),
                    });
                }

                for field in value.fields() {
                    match field.name() {
                        #(#apply_fields)*
                        name => {
                            return Err(::sky_engine::reflect::ReflectError::UnknownField {
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

fn expand_enum(input: &DeriveInput, data: &syn::DataEnum) -> Result<TokenStream2> {
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
                .push(parse_quote!(#ty: ::sky_engine::reflect::Reflect));
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let variant_infos = data.variants.iter().map(|variant| {
        let name = variant.ident.to_string();
        let kind = match &variant.fields {
            Fields::Unit => quote!(::sky_engine::reflect::ReflectVariantKind::Unit),
            Fields::Unnamed(fields) => {
                let len = fields.unnamed.len();
                quote!(::sky_engine::reflect::ReflectVariantKind::Tuple { fields: #len })
            }
            Fields::Named(fields) => {
                let names = fields.named.iter().map(|field| {
                    field
                        .ident
                        .as_ref()
                        .map(|ident| ident.to_string())
                        .unwrap_or_default()
                });
                quote!(::sky_engine::reflect::ReflectVariantKind::Struct {
                    fields: vec![#(#names.to_string()),*],
                })
            }
        };
        quote!(::sky_engine::reflect::ReflectVariant::new(#name, #kind))
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
        impl #impl_generics ::sky_engine::reflect::Reflect for #ident #ty_generics #where_clause {
            fn reflect_type() -> ::sky_engine::reflect::ReflectType {
                ::sky_engine::reflect::ReflectType::new_enum::<Self>(
                    #type_path,
                    vec![#(#variant_infos),*],
                )
            }

            fn reflect_dependencies(
                registry: &mut ::sky_engine::reflect::ReflectRegistry,
            ) -> ::std::result::Result<(), ::sky_engine::reflect::ReflectError> {
                #(#dependency_registers)*
                Ok(())
            }

            fn to_reflect_value(
                &self,
            ) -> ::std::result::Result<
                ::sky_engine::reflect::ReflectValue,
                ::sky_engine::reflect::ReflectError,
            > {
                let variant = match self {
                    #(#read_arms,)*
                };
                Ok(::sky_engine::reflect::ReflectValue::Enum(
                    ::sky_engine::reflect::ReflectEnumValue::new(#type_path, variant)
                ))
            }

            fn apply_reflect_value(
                &mut self,
                value: ::sky_engine::reflect::ReflectValue,
            ) -> ::std::result::Result<(), ::sky_engine::reflect::ReflectError> {
                let value = match value {
                    ::sky_engine::reflect::ReflectValue::Enum(value) => value,
                    other => {
                        return Err(::sky_engine::reflect::ReflectError::ValueTypeMismatch {
                            expected: "Enum",
                            actual: other.kind_name(),
                        });
                    }
                };

                let expected = #type_path;
                if !value.type_name().is_empty() && value.type_name() != expected {
                    return Err(::sky_engine::reflect::ReflectError::EnumTypeMismatch {
                        expected: expected.to_string(),
                        actual: value.type_name().to_string(),
                    });
                }

                match value.variant() {
                    #(#unit_write_arms)*
                    variant => {
                        return Err(::sky_engine::reflect::ReflectError::Unsupported {
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
