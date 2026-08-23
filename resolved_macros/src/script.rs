use std::ops::Deref;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};

use crate::token::{Pos, Token, Tokens};

/// > `__c0`, `__c1` etc
const CAP_PREFIX: &str = "__c";
/// > `__r0`, `__r1` etc
const REF_PREFIX: &str = "__r";

#[derive(Debug, Clone)]
pub(crate) struct Capture(Token);

impl Deref for Capture {
    type Target = Token;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Capture {
    fn new(token: &Token) -> Self {
        Self(token.clone())
    }
}

impl ToTokens for Capture {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let ts: TokenStream = self.0.tree().clone().into();
        tokens.extend(TokenStream2::from(ts));
    }
}

#[derive(Debug)]
pub(crate) struct Captures(Vec<Capture>);

impl Captures {
    pub(crate) fn new() -> Self {
        Self(Vec::new())
    }

    pub(crate) fn add(&mut self, token: &Token) {
        if self.0.iter().any(|arg| &**arg == token) {
            return;
        }
        self.0.push(Capture::new(token));
    }

    pub(crate) fn captures(&self) -> &[Capture] {
        &self.0
    }
}

#[derive(Debug)]
pub(crate) struct Script {
    source: String,
    captures: Captures,
    references: Captures,
}

impl Script {
    pub(crate) fn new(tokens: TokenStream) -> Result<Self, TokenStream2> {
        let tokens = Tokens::retokenize(tokens)?;

        let mut source = String::new();
        let mut captures = Captures::new();
        let mut references = Captures::new();

        let mut curr_cap = 0;
        let mut curr_ref = 0;
        let mut cap_map = std::collections::HashMap::new();
        let mut ref_map = std::collections::HashMap::new();

        let mut prev_end: Option<Pos> = None;
        for t in tokens.0 {
            let mut is_token = 0;
            if t.is_capture() {
                captures.add(&t);
                is_token = 1;
            } else if t.is_itemref() {
                references.add(&t);
                is_token = 1;
            }

            let (line, col) = (t.start().line, t.start().column);
            if let Some(prev) = prev_end {
                if line > prev.line {
                    source.push('\n');
                } else if line == prev.line {
                    source.push_str(&" ".repeat(col.saturating_sub(prev.column + is_token)));
                }
            }

            if t.is_capture() {
                source.push_str(CAP_PREFIX);

                let t_name = t.to_string();
                // if the same var is referenced multiple times we need to track which id it had
                // and only increment if we encounter a totally actual new variable
                match cap_map.get(&t_name) {
                    None => {
                        source.push_str(&curr_cap.to_string());
                        cap_map.insert(t.to_string(), curr_cap.to_string());
                        curr_cap += 1;
                    }
                    Some(count) => {
                        source.push_str(&count);
                    }
                }
            } else if t.is_itemref() {
                source.push_str(REF_PREFIX);

                let t_name = t.to_string();
                match ref_map.get(&t_name) {
                    None => {
                        source.push_str(&curr_ref.to_string());
                        ref_map.insert(t.to_string(), curr_ref.to_string());
                        curr_ref += 1;
                    }
                    Some(count) => {
                        source.push_str(&count);
                    }
                }
            } else {
                source.push_str(&t.to_string());
            }

            prev_end = Some(t.end());
        }

        let source = source.trim_end().to_string();
        Ok(Self {
            source,
            captures,
            references,
        })
    }

    pub(crate) fn captures(&self) -> &[Capture] {
        self.captures.captures()
    }

    pub(crate) fn references(&self) -> &[Capture] {
        self.references.captures()
    }

    pub(crate) fn expand(&self) -> TokenStream2 {
        let source = &self.source;

        let mut curr_cap = 0;
        let mut curr_ref = 0;

        let caps_len = self.captures().len() + self.references().len();
        let caps = self.captures().iter().map(|cap| {
            let cap_name = format!("{CAP_PREFIX}{curr_cap}");
            curr_cap += 1;
            quote! { .and_then(|x| x.named_arg(#cap_name, &#cap)) }
        });
        let refs = self.references().iter().map(|item| {
            let ref_name = format!("{REF_PREFIX}{curr_ref}");
            curr_ref += 1;
            quote! { .and_then(|x| x.named_arg_ref(#ref_name, &#item)) }
        });

        quote! {{
            std::result::Result
                ::<resolved::Script<'_>, resolved::Error>::
                Ok(resolved::Script::new_with_capacity(#source, #caps_len))

            #( #caps )*
            #( #refs )*
        }}
    }
}
