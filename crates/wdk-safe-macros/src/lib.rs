// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Procedural macros for `wdk-safe`.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, Expr, Ident, LitStr, Token, Type,
};

// ── define_ioctl!
// ─────────────────────────────────────────────────────────────

/// Parsed arguments for the `define_ioctl!` macro.
///
/// Full syntax (all fields):
/// ```text
/// define_ioctl!(CONST_NAME, device_type, function, InputType => OutputType)
/// define_ioctl!(CONST_NAME, device_type, function, InputType => OutputType,
///               method = Buffered, access = Any)
/// ```
struct DefineIoctlArgs {
    const_name: Ident,
    device_type: Expr,
    function: Expr,
    input_type: Type,
    output_type: Type,
    method: Option<Ident>,
    access: Option<Ident>,
}

impl Parse for DefineIoctlArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let const_name: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
        let device_type: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let function: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let input_type: Type = input.parse()?;
        input.parse::<Token![=>]>()?;
        let output_type: Type = input.parse()?;

        // Optional trailing `, method = X, access = Y`
        let mut method = None;
        let mut access = None;

        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let val: Ident = input.parse()?;
            match key.to_string().as_str() {
                "method" => method = Some(val),
                "access" => access = Some(val),
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown key `{other}` — expected `method` or `access`"),
                    ))
                }
            }
        }

        Ok(Self {
            const_name,
            device_type,
            function,
            input_type,
            output_type,
            method,
            access,
        })
    }
}

/// Declares a type-safe IOCTL constant with associated input/output types.
///
/// # Syntax
///
/// ```rust,ignore
/// // Minimal — uses `TransferMethod::Buffered` and `RequiredAccess::Any`.
/// define_ioctl!(IOCTL_ECHO, 0x8000u16, 0x800u16, EchoRequest => EchoResponse);
///
/// // Full — explicit transfer method and required access.
/// define_ioctl!(
///     IOCTL_READ_DATA,
///     0x8000u16, 0x801u16,
///     ReadRequest => ReadResponse,
///     method = Buffered,
///     access = ReadWrite,
/// );
/// ```
///
/// # Expansion
///
/// Given `define_ioctl!(IOCTL_ECHO, 0x8000u16, 0x800u16, EchoReq => EchoRsp)`:
///
/// ```rust,ignore
/// pub const IOCTL_ECHO: wdk_safe::IoControlCode = /* ... */;
///
/// /// Input buffer type for [`IOCTL_ECHO`].
/// pub type IoctlEchoInput  = EchoReq;
///
/// /// Output buffer type for [`IOCTL_ECHO`].
/// pub type IoctlEchoOutput = EchoRsp;
/// ```
///
/// # Naming convention
///
/// Type alias names are derived by converting the constant name from
/// `SCREAMING_SNAKE_CASE` to `PascalCase` and appending `Input` / `Output`.
/// For example `IOCTL_MY_REQUEST` → `IoctlMyRequestInput` /
/// `IoctlMyRequestOutput`.
#[proc_macro]
pub fn define_ioctl(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as DefineIoctlArgs);

    let const_name = &args.const_name;
    let device_type = &args.device_type;
    let function = &args.function;
    let input_type = &args.input_type;
    let output_type = &args.output_type;

    // Resolve transfer method — default to Buffered.
    let method_variant = match args.method.as_ref().map(|id| id.to_string()).as_deref() {
        None | Some("Buffered") => quote! { ::wdk_safe::ioctl::TransferMethod::Buffered },
        Some("InDirect") => quote! { ::wdk_safe::ioctl::TransferMethod::InDirect },
        Some("OutDirect") => quote! { ::wdk_safe::ioctl::TransferMethod::OutDirect },
        Some("Neither") => quote! { ::wdk_safe::ioctl::TransferMethod::Neither },
        Some(other) => {
            return syn::Error::new(
                args.method.as_ref().unwrap().span(),
                format!(
                    "unknown transfer method `{other}`; \
                     expected one of: Buffered, InDirect, OutDirect, Neither"
                ),
            )
            .into_compile_error()
            .into();
        }
    };

    // Resolve required access — default to Any.
    let access_variant = match args.access.as_ref().map(|id| id.to_string()).as_deref() {
        None | Some("Any") => quote! { ::wdk_safe::ioctl::RequiredAccess::Any },
        Some("Read") => quote! { ::wdk_safe::ioctl::RequiredAccess::Read },
        Some("Write") => quote! { ::wdk_safe::ioctl::RequiredAccess::Write },
        Some("ReadWrite") => quote! { ::wdk_safe::ioctl::RequiredAccess::ReadWrite },
        Some(other) => {
            return syn::Error::new(
                args.access.as_ref().unwrap().span(),
                format!(
                    "unknown required access `{other}`; \
                     expected one of: Any, Read, Write, ReadWrite"
                ),
            )
            .into_compile_error()
            .into();
        }
    };

    // Derive type alias names: IOCTL_ECHO → IoctlEchoInput / IoctlEchoOutput.
    let name_str = const_name.to_string();
    let pascal: String = name_str
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect();

    let input_alias = Ident::new(&format!("{pascal}Input"), Span::call_site());
    let output_alias = Ident::new(&format!("{pascal}Output"), Span::call_site());

    // Doc strings reference the constant by name.
    let input_doc = LitStr::new(
        &format!("Input buffer type for [`{const_name}`]."),
        Span::call_site(),
    );
    let output_doc = LitStr::new(
        &format!("Output buffer type for [`{const_name}`]."),
        Span::call_site(),
    );

    quote! {
        pub const #const_name: ::wdk_safe::IoControlCode = ::wdk_safe::IoControlCode::new(
            #device_type,
            #function,
            #method_variant,
            #access_variant,
        );

        #[doc = #input_doc]
        pub type #input_alias = #input_type;

        #[doc = #output_doc]
        pub type #output_alias = #output_type;
    }
    .into()
}
