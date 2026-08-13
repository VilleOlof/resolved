use proc_macro::TokenStream;

use crate::script::Script;

mod script;
mod token;

#[rust_analyzer::macro_style(braces)]
#[proc_macro]
pub fn script(input: TokenStream) -> TokenStream {
    match Script::new(input) {
        Ok(chunk) => chunk.expand().into(),
        Err(err) => err.into(),
    }
}
