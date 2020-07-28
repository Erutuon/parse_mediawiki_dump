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
extern crate bzip2;
extern crate parse_mediawiki_dump;

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
            Ok(page) => if page.namespace == 0 && match &page.format {
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

extern crate quick_xml;

use quick_xml::{events::Event, Reader};
use std::{convert::TryInto, io::BufRead, marker::PhantomData};

/// The default namespace type in the [`Page`] struct.
pub type RawNamespace = u32;

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
    Namespace(RawNamespace),
}

/**
Parsed page.

Parsed from the `page` element.

Generic over the type of the namespace, which must be convertible
from `RawNamespace` with `TryInto`. Use [`parse_with_namespace`] to select
a custom type for the namespace; [`parse`] uses the default, `RawNamespace>.

Although the `format` and `model` elements are defined as mandatory in the
[schema](https://www.mediawiki.org/xml/export-0.10.xsd), previous versions
of the schema don't contain them. Therefore the corresponding fields can
be `None`.

A namespace type should have at minimum namespaces 0 to 15, which are
present in all MediaWiki installations, and can look like this:

```rust,no_run
use std::convert::TryFrom;
// use parse_mediawiki_dump::RawNamespace;
type RawNamespace = u32;

pub enum Namespace {
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

impl TryFrom<RawNamespace> for Namespace {
    type Error = &'static str;

    fn try_from(id: RawNamespace) -> Result<Self, Self::Error> {
        use Namespace::*;
        let namespace = match id {
            0 => Main,
            1 => Talk,
            2 => User,
            3 => UserTalk,
            4 => Wiktionary,
            5 => WiktionaryTalk,
            6 => File,
            7 => FileTalk,
            8 => MediaWiki,
            9 => MediaWikiTalk,
            10 => Template,
            11 => TemplateTalk,
            12 => Help,
            13 => HelpTalk,
            14 => Category,
            15 => CategoryTalk,
            _ => return Err("invalid namespace"),
        };
        Ok(namespace)
    }
}
# fn main() {}
```
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
    of the page, which must be a type that can be converted
    from [`RawNamespace`] using [`TryInto`]. All namespaces in the dump are positive numbers,
    so an unsigned type can be used. The corresponding field in the database
    (the [`page_namespace`](https://www.mediawiki.org/wiki/Manual:Page_table#page_namespace)
    field in the `page` table) is a signed integer because there are two
    virtual namespaces with the values `-1` and `-2`, but all the pages that
    have entries in the `page` table have positive namespaces.

    Parsed from the text content of the `ns` element in the `page` element.

    For ordinary articles the namespace is `0`.
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
            Error::Namespace(namespace) => write!(
                formatter,
                "The namespace {} was not recognized",
                namespace
            ),
        }
    }
}

impl From<quick_xml::Error> for Error {
    fn from(value: quick_xml::Error) -> Self {
        Error::XmlReader(value)
    }
}

impl<R: BufRead, N> Iterator for Parser<R, N>
where
    RawNamespace: TryInto<N>,
{
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

fn next<R: BufRead, N>(
    parser: &mut Parser<R, N>,
) -> Result<Option<Page<N>>, Error>
where
    RawNamespace: TryInto<N>,
{
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
                    match parse_text(parser, &namespace)?
                        .parse::<RawNamespace>()
                    {
                        Err(_) => {
                            return Err(Error::Format(
                                parser.reader.buffer_position(),
                            ))
                        }
                        Ok(value) => {
                            namespace = Some(
                                value
                                    .try_into()
                                    .map_err(|_| Error::Namespace(value))?,
                            );
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
/// [`RawNamespace`]. Equivalent to `parse_with_namespace` with the second
/// generic argument set to `RawNamespace` (`parse_with_namespace::<_, RawNamespace>`).
///
/// The stream is parsed as an XML dump exported from MediaWiki. The parser is
/// an iterator over the pages in the dump.
pub fn parse<R: BufRead>(source: R) -> Parser<R, RawNamespace> {
    parse_with_namespace(source)
}

/// Creates a parser for a stream. Allows you to select a type for the namespace.
///
/// The stream is parsed as an XML dump exported from MediaWiki. The parser is
/// an iterator over the pages in the dump.
pub fn parse_with_namespace<R: BufRead, N>(source: R) -> Parser<R, N>
where
    RawNamespace: TryInto<N>,
{
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

fn parse_text<R: BufRead, N>(
    parser: &mut Parser<R, N>,
    output: &Option<impl Sized>,
) -> Result<String, Error>
where
    RawNamespace: TryInto<N>,
{
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

fn skip_element<R: BufRead, N>(
    parser: &mut Parser<R, N>,
) -> Result<(), quick_xml::Error>
where
    RawNamespace: TryInto<N>,
{
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
