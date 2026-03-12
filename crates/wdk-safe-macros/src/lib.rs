// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Procedural macros for `wdk-safe`.
//!
//! # `define_ioctl!`
//!
//! Declares a type-safe IOCTL constant with associated input/output buffer
//! types. See [`define_ioctl`] for full documentation.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, Expr, Ident, LitStr, Token, Type,
};

// ── DefineIoctlArgs parser ────────────────────────────────────────────────────

/// Parsed arguments for `define_ioctl!(...)`.
///
/// Full syntax:
/// ```text
/// define_ioctl!(
///     CONST_NAME,
///     device_type_expr,
///     function_expr,
///     InputType => OutputType,
///     method = Buffered | InDirect | OutDirect | Neither,   // optional
///     access = Any | Read | Write | ReadWrite,              // optional
/// )
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

        let mut method = None;
        let mut access = None;

        // Parse optional trailing `, method = X` and/or `, access = Y`.
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break; // Allow trailing comma.
            }
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let val: Ident = input.parse()?;
            match key.to_string().as_str() {
                "method" => {
                    if method.is_some() {
                        return Err(syn::Error::new(key.span(), "`method` specified twice"));
                    }
                    method = Some(val);
                }
                "access" => {
                    if access.is_some() {
                        return Err(syn::Error::new(key.span(), "`access` specified twice"));
                    }
                    access = Some(val);
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown key `{other}`; expected `method` or `access`\n\
                             hint: valid syntax is `define_ioctl!(NAME, device_type, function, \
                             Input => Output, method = Buffered, access = Any)`"
                        ),
                    ));
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

// ── define_ioctl! ─────────────────────────────────────────────────────────────

/// Declares a type-safe IOCTL constant with associated input/output types.
///
/// # Syntax
///
/// ```rust,ignore
/// // Minimal — defaults to METHOD_BUFFERED and FILE_ANY_ACCESS.
/// define_ioctl!(IOCTL_ECHO, 0x8000u16, 0x800u16, EchoRequest => EchoResponse);
///
/// // Full — explicit transfer method and required access.
/// define_ioctl!(
///     IOCTL_READ_DATA,
///     0x8000u16,
///     0x801u16,
///     ReadRequest => ReadResponse,
///     method = Buffered,
///     access = ReadWrite,
/// );
/// ```
///
/// # Parameters
///
/// | Position | Description |
/// |----------|-------------|
/// | 1 | Constant name (`SCREAMING_SNAKE_CASE`) |
/// | 2 | Device type (`u16`) — use `0x8000`–`0xFFFF` for custom |
/// | 3 | Function code (`u16`) — use `0x800`–`0xFFF` for custom |
/// | 4 | `InputType => OutputType` — buffer types |
/// | `method =` | Transfer method: `Buffered` (default), `InDirect`, `OutDirect`, `Neither` |
/// | `access =` | Required access: `Any` (default), `Read`, `Write`, `ReadWrite` |
///
/// # Expansion
///
/// Given `define_ioctl!(IOCTL_ECHO, 0x8000u16, 0x800u16, EchoReq => EchoRsp)`:
///
/// ```rust,ignore
/// /// IOCTL_ECHO — raw value 0x8000_2000.
/// pub const IOCTL_ECHO: wdk_safe::IoControlCode = /* METHOD_BUFFERED | ANY */;
///
/// /// Input buffer type for `IOCTL_ECHO`.
/// pub type IoctlEchoInput  = EchoReq;
///
/// /// Output buffer type for `IOCTL_ECHO`.
/// pub type IoctlEchoOutput = EchoRsp;
/// ```
///
/// # Naming convention
///
/// Type alias names are derived by converting the constant name from
/// `SCREAMING_SNAKE_CASE` to `PascalCase` and appending `Input`/`Output`.
///
/// | Constant | Input alias | Output alias |
/// |----------|-------------|--------------|
/// | `IOCTL_ECHO` | `IoctlEchoInput` | `IoctlEchoOutput` |
/// | `IOCTL_MY_REQUEST` | `IoctlMyRequestInput` | `IoctlMyRequestOutput` |
#[proc_macro]
pub fn define_ioctl(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as DefineIoctlArgs);

    let const_name = &args.const_name;
    let device_type = &args.device_type;
    let function = &args.function;
    let input_type = &args.input_type;
    let output_type = &args.output_type;

    // Resolve transfer method (default: Buffered).
    let method_tokens = match resolve_method(args.method.as_ref()) {
        Ok(t) => t,
        Err(e) => return e.into_compile_error().into(),
    };

    // Resolve required access (default: Any).
    let access_tokens = match resolve_access(args.access.as_ref()) {
        Ok(t) => t,
        Err(e) => return e.into_compile_error().into(),
    };

    // Derive PascalCase type alias names.
    let (input_alias, output_alias) = derive_alias_names(const_name);

    // Doc strings that reference the constant by name.
    let const_doc = LitStr::new(
        &format!("`{const_name}` — type-safe IOCTL constant."),
        Span::call_site(),
    );
    let input_doc = LitStr::new(
        &format!("Input buffer type for [`{const_name}`]."),
        Span::call_site(),
    );
    let output_doc = LitStr::new(
        &format!("Output buffer type for [`{const_name}`]."),
        Span::call_site(),
    );

    quote! {
        #[doc = #const_doc]
        pub const #const_name: ::wdk_safe::IoControlCode = ::wdk_safe::IoControlCode::new(
            #device_type,
            #function,
            #method_tokens,
            #access_tokens,
        );

        #[doc = #input_doc]
        pub type #input_alias = #input_type;

        #[doc = #output_doc]
        pub type #output_alias = #output_type;
    }
    .into()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn resolve_method(ident: Option<&Ident>) -> syn::Result<proc_macro2::TokenStream> {
    Ok(
        match ident.map(|id| id.to_string()).as_deref() {
            None | Some("Buffered") => {
                quote! { ::wdk_safe::ioctl::TransferMethod::Buffered }
            }
            Some("InDirect") => quote! { ::wdk_safe::ioctl::TransferMethod::InDirect },
            Some("OutDirect") => quote! { ::wdk_safe::ioctl::TransferMethod::OutDirect },
            Some("Neither") => quote! { ::wdk_safe::ioctl::TransferMethod::Neither },
            Some(other) => {
                return Err(syn::Error::new(
                    ident.unwrap().span(),
                    format!(
                        "unknown transfer method `{other}`; \
                         expected one of: Buffered (default), InDirect, OutDirect, Neither"
                    ),
                ));
            }
        },
    )
}

fn resolve_access(ident: Option<&Ident>) -> syn::Result<proc_macro2::TokenStream> {
    Ok(
        match ident.map(|id| id.to_string()).as_deref() {
            None | Some("Any") => quote! { ::wdk_safe::ioctl::RequiredAccess::Any },
            Some("Read") => quote! { ::wdk_safe::ioctl::RequiredAccess::Read },
            Some("Write") => quote! { ::wdk_safe::ioctl::RequiredAccess::Write },
            Some("ReadWrite") => quote! { ::wdk_safe::ioctl::RequiredAccess::ReadWrite },
            Some(other) => {
                return Err(syn::Error::new(
                    ident.unwrap().span(),
                    format!(
                        "unknown required access `{other}`; \
                         expected one of: Any (default), Read, Write, ReadWrite"
                    ),
                ));
            }
        },
    )
}

/// Converts `IOCTL_MY_REQUEST` → (`IoctlMyRequestInput`, `IoctlMyRequestOutput`).
fn derive_alias_names(const_name: &Ident) -> (Ident, Ident) {
    let pascal: String = const_name
        .to_string()
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
    (input_alias, output_alias)
}