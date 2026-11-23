use syn::visit::{self, Visit};
use syn::{Generics, Ident, ImplItem, ItemEnum, ItemImpl, ItemStruct, Type};

use crate::tokens::Source;
use crate::workspace::{Package, Workspace};
use crate::Unread;

/// The `src` of one member, read and parsed.
pub struct Member {
    pub name: String,
    pub files: Vec<(Source, syn::File)>,
}

pub enum Head<'a> {
    Enum(&'a ItemEnum),
    Struct(&'a ItemStruct),
}

struct Heads<'ast, 'name> {
    name: &'name str,
    plain_only: bool,
    found: Vec<Head<'ast>>,
}

impl Heads<'_, '_> {
    fn reads(&self, ident: &Ident, generics: &Generics) -> bool {
        *ident == self.name && !(self.plain_only && (!generics.params.is_empty() || generics.where_clause.is_some()))
    }
}

impl<'ast> Visit<'ast> for Heads<'ast, '_> {
    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        if self.reads(&node.ident, &node.generics) {
            self.found.push(Head::Enum(node));
        }
        visit::visit_item_enum(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        if self.reads(&node.ident, &node.generics) {
            self.found.push(Head::Struct(node));
        }
        visit::visit_item_struct(self, node);
    }
}

struct Capitals(Vec<String>);

impl<'ast> Visit<'ast> for Capitals {
    fn visit_ident(&mut self, node: &'ast Ident) {
        let name: String = node.to_string();
        if name.starts_with(|first: char| first.is_ascii_uppercase()) {
            self.0.push(name);
        }
    }
}

impl Head<'_> {
    /// Every capitalised name written in the variants or the fields.
    pub fn mentions(&self) -> Vec<String> {
        let mut capitals: Capitals = Capitals(Vec::new());
        match self {
            Head::Enum(item) => item.variants.iter().for_each(|variant| capitals.visit_variant(variant)),
            Head::Struct(item) => capitals.visit_fields(&item.fields),
        }
        capitals.0
    }
}

impl Member {
    pub fn read(workspace: &Workspace, package: &Package) -> Result<Member, Unread> {
        let mut sources: Vec<Source> = Vec::new();
        for path in workspace.rust_files(package, &["src"])? {
            sources.push(Source::read(&path, workspace.relative(&path))?);
        }
        Member::of(&package.name, sources)
    }

    pub fn of(name: &str, sources: Vec<Source>) -> Result<Member, Unread> {
        let mut files: Vec<(Source, syn::File)> = Vec::new();
        for source in sources {
            let parsed: syn::File = source.parsed()?;
            files.push((source, parsed));
        }
        Ok(Member { name: name.to_string(), files })
    }

    fn heads(&self, name: &str, plain_only: bool) -> Vec<Head<'_>> {
        let mut heads: Heads = Heads {
            name,
            plain_only,
            found: Vec::new(),
        };
        self.files.iter().for_each(|file| heads.visit_file(&file.1));
        heads.found
    }

    pub fn declares(&self, name: &str) -> bool {
        !self.heads(name, false).is_empty()
    }

    /// The one declaration of a type with no generics; a generic one is not read, so its user ends unread.
    pub fn declared(&self, name: &str) -> Result<Head<'_>, Unread> {
        let mut heads: Vec<Head> = self.heads(name, true);
        match (heads.pop(), heads.len()) {
            (Some(head), 0) => Ok(head),
            (Some(_), more) => Err(Unread(format!(
                "{name} is declared {} times as an enum or a struct in the crate's src; the scan needs exactly one",
                more + 1
            ))),
            (None, _) => Err(Unread(format!(
                "{name} is declared 0 times as an enum or a struct in the crate's src; the scan needs exactly one"
            ))),
        }
    }

    pub fn variants(&self, name: &str) -> Result<Vec<String>, Unread> {
        match self.declared(name)? {
            Head::Enum(item) => Ok(item.variants.iter().map(|variant| variant.ident.to_string()).collect()),
            Head::Struct(_) => Err(Unread(format!("{name} is a struct where the scan needs an enum"))),
        }
    }
}

pub struct Impl<'a> {
    pub kind: String,
    pub holder: String,
    pub item: &'a ItemImpl,
}

struct Impls<'ast, 'kinds> {
    kinds: &'kinds [&'kinds str],
    found: Vec<Impl<'ast>>,
}

pub fn plain(written: &Type) -> Option<String> {
    let Type::Path(path) = written else {
        return None;
    };
    path.qself.is_none().then(|| path.path.get_ident().map(ToString::to_string)).flatten()
}

impl<'ast> Visit<'ast> for Impls<'ast, '_> {
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let kind: Option<String> = node
            .trait_
            .as_ref()
            .and_then(|named| named.1.get_ident())
            .map(ToString::to_string)
            .filter(|kind| self.kinds.contains(&kind.as_str()));
        if let (Some(kind), Some(holder), true) = (kind, plain(&node.self_ty), node.generics.params.is_empty()) {
            self.found.push(Impl { kind, holder, item: node });
        }
        visit::visit_item_impl(self, node);
    }
}

/// The impls written `impl Kind for Type { … }`: the trait and the type each one bare name.
pub fn impls<'a>(file: &'a syn::File, kinds: &[&str]) -> Vec<Impl<'a>> {
    let mut impls: Impls = Impls { kinds, found: Vec::new() };
    impls.visit_file(file);
    impls.found
}

/// The type an impl gives an associated type, when it is written as one bare name.
pub fn slot(item: &ItemImpl, name: &str) -> Option<String> {
    item.items.iter().find_map(|inner| {
        if let ImplItem::Type(given) = inner {
            (given.ident == name).then(|| plain(&given.ty)).flatten()
        } else {
            None
        }
    })
}
