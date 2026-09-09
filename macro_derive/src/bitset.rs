use syn::spanned::Spanned as _;

pub(crate) fn impl_bitset_derive(
    ast: &syn::DeriveInput,
) -> Result<proc_macro::TokenStream, syn::Error> {
    let enum_data = check_enum(ast)?;

    let enum_name = &ast.ident;
    let bitset_width = smallest_bitset_width(ast, enum_data, enum_name)?;

    check_repr(ast, &bitset_width)?;
    check_variants(enum_data)?;

    Ok(write_implementation(
        ast,
        enum_data,
        enum_name,
        &bitset_width,
    ))
}

/// We try and fit the bitset to the smallest possible integer width, based on the number of enum
/// variants.
///
/// # Errors
///
/// If the enum contains more than 64 variants. For ease of implementation we dot not allow for
/// bitsets whose bit width is greater than a single integer.
fn smallest_bitset_width(
    ast: &syn::DeriveInput,
    enum_data: &syn::DataEnum,
    enum_name: &syn::Ident,
) -> Result<syn::Ident, syn::Error> {
    match enum_data.variants.len() {
        0..=8 => Ok(syn::Ident::new("u8", proc_macro2::Span::call_site())),
        9..=16 => Ok(syn::Ident::new("u16", proc_macro2::Span::call_site())),
        17..=32 => Ok(syn::Ident::new("u32", proc_macro2::Span::call_site())),
        33..=64 => Ok(syn::Ident::new("u64", proc_macro2::Span::call_site())),
        n => Err(syn::Error::new_spanned(
            &ast.ident,
            format!("An enum bitset may only contain up to 64 variants, but {enum_name} has {n}"),
        )),
    }
}

/// `#[derive(Bitset)]` can only be applied to enums.
///
/// # Errors
///
/// If the derive macro is being applied to a `struct` or a `union` instead.
fn check_enum(ast: &syn::DeriveInput) -> Result<&syn::DataEnum, syn::Error> {
    match &ast.data {
        syn::Data::Enum(data_enum) => Ok(data_enum),
        syn::Data::Struct(_) => Err(syn::Error::new_spanned(
            &ast.ident,
            "expected an enum, found struct instead",
        )),
        syn::Data::Union(_) => Err(syn::Error::new_spanned(
            &ast.ident,
            "expected an enum, found a union instead",
        )),
    }
}

/// Enums with `#[derive(BitSet)]` must also be marked with `#[repr(bitset_width)]`, where
/// `bitset_width` is determined by [`smallest_bitset_width`].
///
/// # Errors
///
/// If the enum does not have the correct repr or is missing a repr altogether.
fn check_repr(ast: &syn::DeriveInput, bitset: &syn::Ident) -> Result<(), syn::Error> {
    let has_correct_repr = ast.attrs.iter().any(|attribute| {
        attribute.path().is_ident("repr")
            && attribute
                .parse_args::<syn::Ident>()
                .is_ok_and(|ident| &ident == bitset)
    });

    if has_correct_repr {
        Ok(())
    } else {
        Err(syn::Error::new_spanned(
            &ast.ident,
            format!("expected #[repr({bitset})]"),
        ))
    }
}

/// Bitsets can only be derived off enums with unit variants.
///
/// # Errors
///
/// If the enum contains any `struct` or `tupple` variants.
fn check_variants(enum_data: &syn::DataEnum) -> Result<(), syn::Error> {
    for variant in &enum_data.variants {
        let variant_name = &variant.ident;

        match variant.fields {
            syn::Fields::Named(_) => {
                return Err(syn::Error::new(
                    variant.span(),
                    format!(
                        "BitSet can only be constructed off enums with unit variants, but \
                         {variant_name} is a struct variant"
                    ),
                ));
            },
            syn::Fields::Unnamed(_) => {
                return Err(syn::Error::new(
                    variant.span(),
                    format!(
                        "BitSet can only be constructed off enums with unit variants, but \
                         {variant_name} is a tuple variant"
                    ),
                ));
            },
            syn::Fields::Unit => (),
        }
    }

    Ok(())
}

/// Actual bitset implementation, gets baked directly into the surrounding module where the derive
/// is called.
fn write_implementation(
    ast: &syn::DeriveInput,
    enum_data: &syn::DataEnum,
    enum_name: &syn::Ident,
    bitset_width: &syn::Ident,
) -> proc_macro::TokenStream {
    let bitset_name = syn::Ident::new(
        &format!("{enum_name}BitSet"),
        proc_macro2::Span::call_site(),
    );

    let bitset_doc = format!(
        "Compact set of all [`{enum_name}`] instances. \n\nTo save on allocations, the elements \
         of a bitset can be queried inidividually using [`contains`](Self::contains) or as an \
         iterator via [`iter`](Self::iter)"
    );

    let bitset_iter = syn::Ident::new(&format!("{enum_name}Iter"), proc_macro2::Span::call_site());

    let bitset_iter_doc =
        format!("Iterator over all [`{enum_name}`] variants recorded in a [`{bitset_name}`].");

    let variants = enum_data
        .variants
        .iter()
        .map(|variant| variant.ident.clone());

    let reprs = (0..enum_data.variants.len()).map(|i| {
        syn::LitInt::new(
            &format!("{i}{bitset_width}"),
            proc_macro2::Span::call_site(),
        )
    });

    let vis = &ast.vis;

    quote::quote! {
        #[doc = #bitset_doc]
        #[derive(Debug, Clone, PartialEq, Eq)]
        #vis struct #bitset_name(#bitset_width);

        impl #bitset_name {
            /// Initializes a new empty bitset. New error instances can be recorded by calling [`add`].
            ///
            /// [`add`]: Self::add
            #vis fn new() -> Self {
                Self(0)
            }

            /// Records an enum instance in the bitset. Enum occurrence can later be checked by
            /// calling [`contains`].
            ///
            /// [`contains`]: Self::contains
            #vis fn add(&mut self, err: #enum_name) {
                self.0 |= 1 << (err as #bitset_width);
            }

            /// Checks if an enum instance was previously recorded into the bitset.
            #vis fn contains(&self, err: #enum_name) -> bool {
                self.0 & 1 << (err as #bitset_width) > 0
            }

            /// Returns the number of enum instances recorded into the bitset.
            #vis fn len(&self) -> usize {
                self.0.count_ones() as usize
            }

            /// Returns true if the bitset contains no recorded enum instance.
            #vis fn is_empty(&self) -> bool {
                self.len() == 0
            }

            /// Iterates over all enum instances currently recorded in the bitset.
            #vis fn iter(&self) -> #bitset_iter {
                #bitset_iter {
                    bitset: self.0,
                    index: 0,
                }
            }
        }

        impl IntoIterator for #bitset_name {
            type Item = #enum_name;
            type IntoIter = #bitset_iter;

            fn into_iter(self) -> #bitset_iter {
                self.iter()
            }
        }

        #[doc = #bitset_iter_doc]
        #[derive(Debug, Clone, PartialEq, Eq)]
        #vis struct #bitset_iter {
            bitset: #bitset_width,
            index: u8,
        }

        impl Iterator for #bitset_iter {
            type Item = #enum_name;

            fn next(&mut self) -> Option<#enum_name> {
                while self.index < #bitset_width::BITS as u8 {
                    let check_occurrence = self.bitset & 1 << self.index;

                    self.index += 1;

                    if check_occurrence > 0 {
                        let index = check_occurrence.trailing_zeros() as #bitset_width;

                        match index {
                            #(
                                #reprs => return Some(#enum_name::#variants),
                            )*
                            _ => unreachable!()
                        }
                    }
                }

                None
            }
        }

        impl ExactSizeIterator for #bitset_iter {
            fn len(&self) -> usize {
                self.bitset.count_ones() as usize
            }
        }
    }
    .into()
}
