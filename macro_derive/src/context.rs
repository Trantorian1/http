pub(crate) fn impl_context(mut ast: syn::Item) -> Result<proc_macro::TokenStream, syn::Error> {
    // Extract function data
    let syn::Item::Fn(fn_data) = &mut ast else {
        return Err(syn::Error::new_spanned(&ast, "Expected a function"));
    };

    let vis = &fn_data.vis;
    let generics = &fn_data.sig.generics;
    let inputs = fn_data.sig.inputs.clone();
    let block = fn_data.block.clone();

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let mut field_names = Vec::new();
    let mut field_types = Vec::new();
    let mut destructure = Vec::new();

    for input in &inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                return Err(syn::Error::new_spanned(
                    receiver,
                    "context can only be applied to functions which don't take `self` as a \
                     parameter",
                ));
            },
            syn::FnArg::Typed(syn::PatType { pat, ty, .. }) => {
                if let syn::Pat::Ident(ident) = pat.as_ref() {
                    field_names.push(ident.ident.clone());
                    field_types.push(ty);
                    destructure.push(ident);
                }
            },
        }
    }

    // Extract function arguments into their own `Context` struct.
    let context = quote::quote! {
        #vis struct Context #impl_generics {
            #(pub #field_names: #field_types),*
        } #where_clause

    };

    // Update function arguments and body to use the new `Context` struct instead.
    fn_data.sig.inputs = syn::parse_quote! { _context: Context #ty_generics };
    fn_data.block = syn::parse_quote! {
        {
            let Context {
                #(#destructure),*
            } = _context;

            #block
        }
    };

    // Combine `Context` struct and the new function definition.
    let token_stream = quote::quote! {
        #context

        #fn_data
    };

    Ok(token_stream.into())
}
