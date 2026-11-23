use std::fs;
use std::path::Path;

use proc_macro2::{Spacing, Span, TokenStream, TokenTree};

use crate::Unread;

pub struct Source {
    pub shown: String,
    pub text: String,
    pub tokens: TokenStream,
}

impl Source {
    pub fn read(path: &Path, shown: String) -> Result<Source, Unread> {
        let text: String = fs::read_to_string(path).map_err(|error| Unread(format!("{shown}: {error}")))?;
        Source::of(shown, text)
    }

    pub fn of(shown: String, text: String) -> Result<Source, Unread> {
        let tokens: TokenStream = text
            .parse()
            .map_err(|error| Unread(format!("{shown}: {error}; the scan cannot read this file")))?;
        Ok(Source { shown, text, tokens })
    }

    pub fn parsed(&self) -> Result<syn::File, Unread> {
        syn::parse2(self.tokens.clone())
            .map_err(|error| Unread(format!("{}:{}: {error}; the scan cannot read this file", self.shown, error.span().start().line)))
    }

    pub fn slice(&self, span: Span) -> &str {
        self.text.get(span.byte_range()).unwrap_or("")
    }
}

pub fn squeezed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

/// Calls `visit` with the trees of the stream and of every group inside it, a macro's included.
pub fn levels(stream: &TokenStream, visit: &mut dyn FnMut(&[TokenTree])) {
    let trees: Vec<TokenTree> = stream.clone().into_iter().collect();
    visit(&trees);
    for tree in &trees {
        if let TokenTree::Group(group) = tree {
            levels(&group.stream(), visit);
        }
    }
}

pub fn is_ident(tree: &TokenTree, name: &str) -> bool {
    matches!(tree, TokenTree::Ident(ident) if ident == name)
}

pub fn is_punct(tree: &TokenTree, mark: char) -> bool {
    matches!(tree, TokenTree::Punct(punct) if punct.as_char() == mark)
}

fn is_joint(tree: &TokenTree, mark: char) -> bool {
    matches!(tree, TokenTree::Punct(punct) if punct.as_char() == mark && punct.spacing() == Spacing::Joint)
}

pub fn fat_arrows(stream: &TokenStream) -> usize {
    let mut count: usize = 0;
    levels(stream, &mut |trees| {
        count += trees.windows(2).filter(|pair| is_joint(&pair[0], '=') && is_punct(&pair[1], '>')).count()
    });
    count
}

pub fn followed(stream: &TokenStream, first: &str, second: &str) -> usize {
    let mut count: usize = 0;
    levels(stream, &mut |trees| {
        count += trees.windows(2).filter(|pair| is_ident(&pair[0], first) && is_ident(&pair[1], second)).count()
    });
    count
}

pub fn idents(stream: &TokenStream, name: &str) -> usize {
    let mut count: usize = 0;
    levels(stream, &mut |trees| count += trees.iter().filter(|tree| is_ident(tree, name)).count());
    count
}
