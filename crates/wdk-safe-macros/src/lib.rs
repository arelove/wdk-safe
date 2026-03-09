// Copyright (c) 2025 arelove
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Procedural macros for `wdk-safe`.

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    Expr,
    Ident,
    Token,
    Type,
};

/// Parsed arguments for the `define_ioctl!` macro.
///
/// Syntax:
/// ```text
/// define_ioctl!(CONST_NAME, device_type, function, Input => Output)
/// ```
struct DefineIoctlArgs {
    const_name:  Ident,
    device_type: Expr,
    function:    Expr,
    input_type:  Type,
    output_type: Type,
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
        Ok(Self {
            const_name,
            device_type,
            function,
            input_type,
            output_type,
        })
    }
}

/// Declares a type-safe IOCTL constant with associated input/output types.
///
/// # Syntax
///
/// ```rust,ignore
/// define_ioctl!(CONST_NAME, device_type, function, InputType => OutputType);
/// ```
///
/// # Example
///
/// ```rust,ignore
/// use wdk_safe::define_ioctl;
///
/// #[repr(C)]
/// pub struct EchoRequest  { pub value: u32 }
///
/// #[repr(C)]
/// pub struct EchoResponse { pub value: u32 }
///
/// define_ioctl!(IOCTL_ECHO, 0x8000u16, 0x800u16, EchoRequest => EchoResponse);
///
/// // Expands to:
/// //   pub const IOCTL_ECHO: wdk_safe::IoControlCode = ...;
/// //   pub type IoctlEchoInput  = EchoRequest;
/// //   pub type IoctlEchoOutput = EchoResponse;
/// ```
#[proc_macro]
pub fn define_ioctl(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as DefineIoctlArgs);

    let const_name  = &args.const_name;
    let device_type = &args.device_type;
    let function    = &args.function;
    let input_type  = &args.input_type;
    let output_type = &args.output_type;

    // Derive type alias names from the const name, e.g.
    // IOCTL_ECHO  →  IoctlEchoInput  /  IoctlEchoOutput
    let name_str = const_name.to_string();
    let pascal: String = name_str
        .split('_')
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + chars.as_str().to_lowercase().as_str()
                }
            }
        })
        .collect();

    let input_alias  = Ident::new(&format!("{pascal}Input"),  proc_macro2::Span::call_site());
    let output_alias = Ident::new(&format!("{pascal}Output"), proc_macro2::Span::call_site());

    quote! {
        pub const #const_name: ::wdk_safe::IoControlCode = ::wdk_safe::IoControlCode::new(
            #device_type,
            #function,
            ::wdk_safe::ioctl::TransferMethod::Buffered,
            ::wdk_safe::ioctl::RequiredAccess::Any,
        );

        /// Input buffer type for [`
        #[doc = stringify!(#const_name)]
        /// `].
        pub type #input_alias = #input_type;

        /// Output buffer type for [`
        #[doc = stringify!(#const_name)]
        /// `].
        pub type #output_alias = #output_type;
    }
    .into()
}