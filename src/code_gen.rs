use convert_case::{Case, Casing};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::code_analysis::PrepareStatement;

pub(crate) async fn gen_file(
    client: &impl tokio_postgres::GenericClient,
    stmts_raw: String,
) -> eyre::Result<String> {
    crate::code_analysis::prepare_stmts(client, &stmts_raw)
        .await?
        .into_iter()
        .map(gen_fn)
        .collect::<eyre::Result<Vec<String>>>()
        .map(|s| s.join("\n"))
}

fn gen_fn(ps: PrepareStatement) -> eyre::Result<String> {
    fn quote_type(ty: &tokio_postgres::types::Type) -> eyre::Result<TokenStream> {
        use tokio_postgres::types::Type;
        Ok(match ty {
            &Type::BOOL => quote! { bool },
            &Type::INT2 => quote! { i16 },
            &Type::INT4 => quote! { i32 },
            &Type::INT8 => quote! { i64 },
            &Type::FLOAT4 => quote! { f32 },
            &Type::FLOAT8 => quote! { f64 },
            &Type::CHAR | &Type::VARCHAR | &Type::TEXT | &Type::NAME => quote! { String },
            &Type::BYTEA => quote! { Vec<u8> },
            &Type::OID => quote! { tokio_postgres::types::Oid },
            &Type::OID_ARRAY => quote! { Vec<tokio_postgres::types::Oid> },
            &Type::BOOL_ARRAY => quote! { Vec<bool> },
            &Type::NAME_ARRAY => quote! { Vec<String> },
            _ => eyre::bail!("type {ty} not supported yet"),
        })
    }

    let pascal_name = ps.name.to_case(Case::Pascal);
    let rows_struct_ident = format_ident!("{}Rows", pascal_name);
    let params_struct_ident = format_ident!("{}Params", pascal_name);

    let fn_name = format_ident!("{}", ps.name);
    let sql_statement = ps.statement.to_string();

    let has_params = !ps.parameter_types.is_empty();
    let param_params = if has_params {
        quote! {
            p: #params_struct_ident
        }
    } else {
        quote! {}
    };
    let params_struct = if has_params {
        let param_types = ps
            .parameter_types
            .iter()
            .map(|p| {
                let field_type = quote_type(&p.type_)?;
                let field_ident = format_ident!("{}", p.name);

                Ok(quote! {
                    pub #field_ident: #field_type
                })
            })
            .collect::<eyre::Result<Vec<_>>>()?;

        quote! {
            pub struct #params_struct_ident{
                #(#param_types,)*
            }
        }
    } else {
        quote! {}
    };

    let rows_struct = {
        let result_fields = ps
            .result_types
            .iter()
            .map(|c| {
                let field_type = quote_type(&c.type_)?;
                let field_ident = format_ident!("{}", c.name);

                Ok(quote! {
                    pub #field_ident: Option<#field_type>
                })
            })
            .collect::<eyre::Result<Vec<_>>>()?;
        quote! {
            pub struct #rows_struct_ident{
                #(#result_fields,)*
            }
        }
    };

    // Generate param binding for the query
    let param_binding = if has_params {
        // Create parameter references for binding
        let param_refs = ps
            .parameter_types
            .iter()
            .map(|p| {
                let field_ident = format_ident!("{}", p.name);

                Ok(quote! { &p.#field_ident })
            })
            .collect::<eyre::Result<Vec<_>>>()?;

        quote! { &[#(#param_refs),*] }
    } else {
        quote! { &[] }
    };

    let get_expressions = {
        let get_exprs = ps
            .result_types
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let field_ident = format_ident!("{}", c.name);
                let i = proc_macro2::Literal::usize_unsuffixed(i);

                quote! { #field_ident: r.get(#i) }
            })
            .collect::<Vec<_>>();

        quote! { #(#get_exprs),* }
    };

    // Generate the function body with the appropriate try_get expressions
    let paragraph = match ps.client_method {
        crate::code_analysis::ClientMethod::Query => {
            quote! {
                #params_struct
                #rows_struct

                pub async fn #fn_name(
                    c: &impl tokio_postgres::GenericClient,
                    #param_params
                ) -> Result<Vec<#rows_struct_ident>, tokio_postgres::Error> {
                    c.query(#sql_statement, #param_binding).await.map(|rs| {
                        rs.into_iter()
                            .map(|r| #rows_struct_ident{
                                #get_expressions
                            })
                            .collect()
                    })
                }
            }
        }
        crate::code_analysis::ClientMethod::Execute => {
            quote! {
                #params_struct

                pub async fn #fn_name(
                    c: &impl tokio_postgres::GenericClient,
                    #param_params
                ) -> Result<u64, tokio_postgres::Error> {
                    c.execute(#sql_statement, #param_binding).await
                }
            }
        }
    };

    Ok(prettyplease::unparse(&syn::parse2(paragraph)?))
}
