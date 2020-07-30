// Copyright 2018 Fredrik Portström <https://portstrom.com>
// This is free software distributed under the terms specified in
// the file LICENSE at the top-level directory of this distribution.

/*!
Parse XML dumps exported from MediaWiki.

This module parses [XML dumps](https://www.mediawiki.org/wiki/Help:Export)
exported from MediaWiki, providing each page from the dump through an iterator.
This is useful for parsing
the [dumps from Wikipedia and other Wikimedia projects](https://dumps.wikimedia.org).

# Caution

If you need to parse any wiki text extracted from a dump, please use the crate
Parse Wiki Text ([crates.io](https://crates.io/crates/parse_wiki_text),
[Github](https://github.com/portstrom/parse_wiki_text)).
Correctly parsing wiki text requires dealing with an astonishing amount
of difficult and counterintuitive cases. Parse Wiki Text automatically deals
with all these cases, giving you an unambiguous tree of parsed elements
that is easy to work with.

# Limitations

This module only parses dumps containing only one revision of each page.
This is what you get from the page `Special:Export` when enabling the option
“Include only the current revision, not the full history”, as well as what you
get from the Wikimedia dumps with file names ending with `-pages-articles.xml.bz2`.

This module ignores the `siteinfo` element, every child element of the `page`
element except `ns`, `revision` and `title`, and every element inside the
`revision` element except `format`, `model` and `text`.

Until there is a real use case that justifies going beyond these limitations,
they will remain in order to avoid premature design driven by imagined requirements.

# Examples

Parse a bzip2 compressed file and distinguish ordinary articles from other pages.
A running example with complete error handling is available in the
`examples` folder.

```rust,no_run
fn main() {
    let file = std::fs::File::open("example.xml.bz2").unwrap();
    let file = std::io::BufReader::new(file);
    let file = bzip2::bufread::BzDecoder::new(file);
    let file = std::io::BufReader::new(file);
    for result in parse_mediawiki_dump::parse(file) {
        match result {
            Err(error) => {
                eprintln!("Error: {}", error);
                break;
            }
            Ok(page) => if page.namespace.into_inner() == 0 && match &page.format {
                None => false,
                Some(format) => format == "text/x-wiki"
            } && match &page.model {
                None => false,
                Some(model) => model == "wikitext"
            } {
                println!(
                    "The page {title:?} is an ordinary article with byte length {length}.",
                    title = page.title,
                    length = page.text.len()
                );
            } else {
                println!("The page {:?} has something special to it.", page.title);
            }
        }
    }
}
```
*/

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use quick_xml::{events::Event, Reader};
use std::{convert::TryInto, io::BufRead, marker::PhantomData, str::FromStr};

/**
The default namespace type in the [`Page`] struct.

It wraps a signed integer because the corresponding field in the database
(the [`page_namespace`] field in the `page` table) is
a signed integer. However, all namespaces in the dump are positive numbers.
The two negative namespaces, -1 (Special) and -2 (Media)
never actually appear in the `page` table or in the XML dump.

The [`FromNamespaceId`] trait can be implemented to convert this type into
an enum that represents the namespaces of a particular MediaWiki installation.

[`page_namespace`]:
https://www.mediawiki.org/wiki/Manual:Page_table#page_namespace
*/
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash, Default)]
pub struct NamespaceId(pub i32);

impl NamespaceId {
    /// Creates a `NamespaceId` from an `i32`.
    pub const fn new(n: i32) -> Self {
        Self(n)
    }

    /// Returns the wrapped integer.
    pub const fn into_inner(self) -> i32 {
        self.0
    }
}

impl From<i32> for NamespaceId {
    fn from(n: i32) -> Self {
        Self::new(n)
    }
}

impl From<NamespaceId> for i32 {
    fn from(id: NamespaceId) -> Self {
        id.0
    }
}

impl FromStr for NamespaceId {
    type Err = <i32 as FromStr>::Err;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(FromStr::from_str(s)?))
    }
}

/**
Trait for a fallible conversion from [`NamespaceId`].
Required by the `namespace` field in the [`Page`] struct.

Automatically implemented for types that can be converted from `NamespaceId`
by [`TryInto::try_into`].

# Implementation
The trait can be implemented with the [`impl_namespace`] macro.

A type implementing `FromNamespaceId` should include values for namespace ids
-2 to 15, because they are present in all MediaWiki installations
according to the [MediaWiki documentation][built-in namespaces]
and all of 0 to 15 are likely to be found in a `pages-meta-current.xml` dump file.

[built-in namespaces]: https://www.mediawiki.org/wiki/Manual:Namespace#Built-in_namespaces

```rust
use std::convert::TryFrom;
use parse_mediawiki_dump::{FromNamespaceId, impl_namespace, NamespaceId};

impl_namespace! {
    /// A type containing the built-in MediaWiki namespaces.
    pub enum Namespace {
        Media = -2,
        Special = -1,
        Main = 0,
        Talk = 1,
        User = 2,
        UserTalk = 3,
        Wiktionary = 4,
        WiktionaryTalk = 5,
        File = 6,
        FileTalk = 7,
        MediaWiki = 8,
        MediaWikiTalk = 9,
        Template = 10,
        TemplateTalk = 11,
        Help = 12,
        HelpTalk = 13,
        Category = 14,
        CategoryTalk = 15,
    }
}

fn main() {
    assert_eq!(
        Namespace::from_namespace_id(NamespaceId(0)),
        Some(Namespace::Main)
    );
    assert_eq!(
        Namespace::from_namespace_id(NamespaceId(11)),
        Some(Namespace::TemplateTalk)
    );
}
```
*/
pub trait FromNamespaceId: Sized {
    /// Converts fallibly from `NamespaceId`.
    fn from_namespace_id(id: NamespaceId) -> Option<Self>;
}

impl<T> FromNamespaceId for T
where
    NamespaceId: TryInto<T>,
{
    fn from_namespace_id(id: NamespaceId) -> Option<Self> {
        id.try_into().ok()
    }
}

enum PageChildElement {
    Ns,
    Revision,
    Title,
    Redirect,
    Unknown,
}

enum RevisionChildElement {
    Format,
    Model,
    Text,
    Unknown,
}

#[derive(Debug)]
/// The error type for `Parser`.
pub enum Error {
    /// Format not matching expectations.
    ///
    /// Indicates the position in the stream.
    Format(usize),

    /// The source contains a feature not supported by the parser.
    ///
    /// In particular, this means a `page` element contains more than one `revision` element.
    NotSupported(usize),

    /// Error from the XML reader.
    XmlReader(quick_xml::Error),

    /// Namespace id could not be converted to selected namespace type.
    #[allow(missing_docs)]
    Namespace { id: NamespaceId, position: usize },
}

/**
Parsed page.

Parsed from the `page` element.

Generic over the type of the namespace, which must be convertible
from `NamespaceId` with `TryInto`. Use [`parse_with_namespace`] to select
a custom type for the namespace; [`parse`] uses the default, `NamespaceId>.

Although the `format` and `model` elements are defined as mandatory in the
[schema](https://www.mediawiki.org/xml/export-0.10.xsd), previous versions
of the schema don't contain them. Therefore the corresponding fields can
be `None`.
*/
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Page<N> {
    /// The format of the revision if any.
    ///
    /// Parsed from the text content of the `format` element in the `revision`
    /// element. `None` if the element is not present.
    ///
    /// For ordinary articles the format is `text/x-wiki`.
    pub format: Option<String>,

    /// The model of the revision if any.
    ///
    /// Parsed from the text content of the `model` element in the `revision`
    /// element. `None` if the element is not present.
    ///
    /// For ordinary articles the model is `wikitext`.
    pub model: Option<String>,

    /**
    The [namespace](https://www.mediawiki.org/wiki/Manual:Namespace)
    of the page. All parsing functions require that this field
    implement `FromNamespaceId`.

    Parsed from the text content of the `ns` element in the `page` element.
    */
    pub namespace: N,

    /// The text of the revision.
    ///
    /// Parsed from the text content of the `text` element in the `revision` element.
    pub text: String,

    /// The title of the page.
    ///
    /// Parsed from the text content of the `title` element in the `page` element.
    pub title: String,

    /// The redirect target if any.
    ///
    /// Parsed from the content of the `title` attribute of the `redirect`
    /// element in the `page` element.
    ///
    /// For pages that are not redirects, the `redirect` element is not present.
    pub redirect_title: Option<String>,
}

/// Parser working as an iterator over pages.
pub struct Parser<R: BufRead, Namespace> {
    buffer: Vec<u8>,
    namespace_buffer: Vec<u8>,
    reader: Reader<R>,
    started: bool,
    phantom: PhantomData<Namespace>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Format(position) => {
                write!(formatter, "Invalid format at position {}", position)
            }
            Error::NotSupported(position) => write!(
                formatter,
                "The element at position {} is not supported",
                position
            ),
            Error::XmlReader(error) => error.fmt(formatter),
            Error::Namespace { id, position } => write!(
                formatter,
                "The namespace {} at position {} was not recognized",
                id.into_inner(),
                position,
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::XmlReader(e) = self {
            Some(e)
        } else {
            None
        }
    }
}

impl From<quick_xml::Error> for Error {
    fn from(value: quick_xml::Error) -> Self {
        Error::XmlReader(value)
    }
}

impl<R: BufRead, N: FromNamespaceId> Iterator for Parser<R, N> {
    type Item = Result<Page<N>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(match next(self) {
            Err(error) => Err(error),
            Ok(item) => Ok(item?),
        })
    }
}

fn match_namespace(namespace: Option<&[u8]>) -> bool {
    match namespace {
        None => false,
        Some(namespace) => {
            namespace == b"http://www.mediawiki.org/xml/export-0.10/" as &[u8]
        }
    }
}

fn next<R: BufRead, N: FromNamespaceId>(
    parser: &mut Parser<R, N>,
) -> Result<Option<Page<N>>, Error> {
    if !parser.started {
        loop {
            parser.buffer.clear();
            if let (namespace, Event::Start(event)) =
                parser.reader.read_namespaced_event(
                    &mut parser.buffer,
                    &mut parser.namespace_buffer,
                )?
            {
                if match_namespace(namespace)
                    && event.local_name() == b"mediawiki"
                {
                    break;
                }
                return Err(Error::Format(parser.reader.buffer_position()));
            }
        }
        parser.started = true;
    }
    loop {
        parser.buffer.clear();
        if !match parser.reader.read_namespaced_event(
            &mut parser.buffer,
            &mut parser.namespace_buffer,
        )? {
            (_, Event::End(_)) => return Ok(None),
            (namespace, Event::Start(event)) => {
                match_namespace(namespace) && event.local_name() == b"page"
            }
            _ => continue,
        } {
            skip_element(parser)?;
            continue;
        }
        let mut format = None;
        let mut model = None;
        let mut namespace = None;
        let mut redirect_title = None;
        let mut text = None;
        let mut title = None;
        loop {
            parser.buffer.clear();
            match match parser.reader.read_namespaced_event(
                &mut parser.buffer,
                &mut parser.namespace_buffer,
            )? {
                (_, Event::End(_)) => {
                    return match (namespace, text, title) {
                        (Some(namespace), Some(text), Some(title)) => {
                            Ok(Some(Page {
                                format,
                                model,
                                namespace,
                                redirect_title,
                                text,
                                title,
                            }))
                        }
                        _ => {
                            Err(Error::Format(parser.reader.buffer_position()))
                        }
                    }
                }
                (namespace, Event::Start(event)) => {
                    if match_namespace(namespace) {
                        match event.local_name() {
                            b"ns" => PageChildElement::Ns,
                            b"redirect" => {
                                let title_attribute = event
                                    .attributes()
                                    .filter_map(|r| r.ok())
                                    .find(|attr| attr.key == b"title");
                                redirect_title = match title_attribute {
                                    Some(attr) => {
                                        Some(attr.unescape_and_decode_value(
                                            &parser.reader,
                                        )?)
                                    }
                                    None => {
                                        return Err(Error::Format(
                                            parser.reader.buffer_position(),
                                        ))
                                    }
                                };
                                PageChildElement::Redirect
                            }
                            b"revision" => PageChildElement::Revision,
                            b"title" => PageChildElement::Title,
                            _ => PageChildElement::Unknown,
                        }
                    } else {
                        PageChildElement::Unknown
                    }
                }
                _ => continue,
            } {
                PageChildElement::Ns => {
                    match parse_text(parser, &namespace)?.parse::<NamespaceId>()
                    {
                        Err(_) => {
                            return Err(Error::Format(
                                parser.reader.buffer_position(),
                            ))
                        }
                        Ok(value) => {
                            namespace =
                                Some(N::from_namespace_id(value).ok_or_else(
                                    || Error::Namespace {
                                        id: value,
                                        position:
                                            parser.reader.buffer_position(),
                                    },
                                )?);
                            continue;
                        }
                    }
                }
                PageChildElement::Redirect => skip_element(parser)?,
                PageChildElement::Revision => {
                    if text.is_some() {
                        return Err(Error::NotSupported(
                            parser.reader.buffer_position(),
                        ));
                    }
                    loop {
                        parser.buffer.clear();
                        match match parser.reader.read_namespaced_event(
                            &mut parser.buffer,
                            &mut parser.namespace_buffer,
                        )? {
                            (_, Event::End(_)) => match text {
                                None => {
                                    return Err(Error::Format(
                                        parser.reader.buffer_position(),
                                    ))
                                }
                                Some(_) => break,
                            },
                            (namespace, Event::Start(event)) => {
                                if match_namespace(namespace) {
                                    match event.local_name() {
                                        b"format" => {
                                            RevisionChildElement::Format
                                        }
                                        b"model" => RevisionChildElement::Model,
                                        b"text" => RevisionChildElement::Text,
                                        _ => RevisionChildElement::Unknown,
                                    }
                                } else {
                                    RevisionChildElement::Unknown
                                }
                            }
                            _ => continue,
                        } {
                            RevisionChildElement::Format => {
                                format = Some(parse_text(parser, &format)?)
                            }
                            RevisionChildElement::Model => {
                                model = Some(parse_text(parser, &model)?)
                            }
                            RevisionChildElement::Text => {
                                text = Some(parse_text(parser, &text)?)
                            }
                            RevisionChildElement::Unknown => {
                                skip_element(parser)?
                            }
                        }
                    }
                    continue;
                }
                PageChildElement::Title => {
                    title = Some(parse_text(parser, &title)?);
                    continue;
                }
                PageChildElement::Unknown => skip_element(parser)?,
            }
        }
    }
}

/// Creates a parser for a stream in which namespaces are represented as
/// [`NamespaceId`]. Equivalent to `parse_with_namespace` with the second
/// generic argument set to `NamespaceId` (`parse_with_namespace::<_, NamespaceId>`).
///
/// The stream is parsed as an XML dump exported from MediaWiki. The parser is
/// an iterator over the pages in the dump.
pub fn parse<R: BufRead>(source: R) -> Parser<R, NamespaceId> {
    parse_with_namespace(source)
}

/// Creates a parser for a stream. Allows you to select a type for the namespace.
///
/// The stream is parsed as an XML dump exported from MediaWiki. The parser is
/// an iterator over the pages in the dump.
pub fn parse_with_namespace<R: BufRead, N: FromNamespaceId>(
    source: R,
) -> Parser<R, N> {
    let mut reader = Reader::from_reader(source);
    reader.expand_empty_elements(true);
    Parser {
        buffer: vec![],
        namespace_buffer: vec![],
        reader,
        started: false,
        phantom: PhantomData,
    }
}

fn parse_text<R: BufRead, N: FromNamespaceId>(
    parser: &mut Parser<R, N>,
    output: &Option<impl Sized>,
) -> Result<String, Error> {
    if output.is_some() {
        return Err(Error::Format(parser.reader.buffer_position()));
    }
    parser.buffer.clear();
    let text = match parser
        .reader
        .read_namespaced_event(
            &mut parser.buffer,
            &mut parser.namespace_buffer,
        )?
        .1
    {
        Event::Text(text) => text.unescape_and_decode(&parser.reader)?,
        Event::End { .. } => return Ok(String::new()),
        _ => return Err(Error::Format(parser.reader.buffer_position())),
    };
    parser.buffer.clear();
    if let Event::End(_) = parser
        .reader
        .read_namespaced_event(
            &mut parser.buffer,
            &mut parser.namespace_buffer,
        )?
        .1
    {
        Ok(text)
    } else {
        Err(Error::Format(parser.reader.buffer_position()))
    }
}

fn skip_element<R: BufRead, N: FromNamespaceId>(
    parser: &mut Parser<R, N>,
) -> Result<(), quick_xml::Error> {
    let mut level = 0;
    loop {
        parser.buffer.clear();
        match parser
            .reader
            .read_namespaced_event(
                &mut parser.buffer,
                &mut parser.namespace_buffer,
            )?
            .1
        {
            Event::End(_) => {
                if level == 0 {
                    return Ok(());
                }
                level -= 1;
            }
            Event::Start(_) => level += 1,
            _ => {}
        }
    }
}

/**
Enclose a namespace enum definition to derive the [`FromNamespaceId`] trait
as well as other [common traits] ([`Debug`], [`Eq`], [`PartialEq`], [`Ord`],
[`PartialOrd`], [`Clone`], [`Copy`], [`Hash`]) for it.

[common traits]:
https://rust-lang.github.io/api-guidelines/interoperability.html#c-common-traits
*/
#[macro_export]
macro_rules! impl_namespace {
    (
        $(#[$attribute:meta])*
        $visibility:vis enum $namespace:ident {
            $($variant:ident = $id:literal),* $(,)?
        }
    ) => {
        $(#[$attribute])*
        #[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy, Hash)]
        #[repr(i32)]
        $visibility enum $namespace {
            $($variant = $id,)*
        }

        impl ::std::convert::TryFrom<::parse_mediawiki_dump::NamespaceId> for $namespace {
            type Error = &'static str;

            fn try_from(id: ::parse_mediawiki_dump::NamespaceId) -> Result<Self, Self::Error> {
                match i32::from(id) {
                    $($id => Ok($namespace::$variant),)*
                    _ => Err("invalid namespace id"),
                }
            }
        }
    };
}
