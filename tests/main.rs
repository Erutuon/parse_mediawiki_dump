// Copyright 2018 Fredrik Portström <https://portstrom.com>
// This is free software distributed under the terms specified in
// the file LICENSE at the top-level directory of this distribution.

use parse_mediawiki_dump::NamespaceId;
use std::convert::TryFrom;
use std::io::{BufReader, Cursor};

const DUMP: &str = r#"
<mediawiki xmlns="http://www.mediawiki.org/xml/export-0.10/">,
    <page>
        <ns>0</ns>
        <title>alpha</title>
        <revision>
            <format>beta</format>
            <model>gamma</model>
            <text>delta</text>
        </revision>
    </page>
    <page>
        <title>epsilon</title>
        <ns>1</ns>
        <redirect title="zeta" />
        <revision>
            <text>eta</text>
        </revision>
    </page>
</mediawiki>"#;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy, Hash)]
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

impl TryFrom<NamespaceId> for Namespace {
    type Error = &'static str;

    fn try_from(id: NamespaceId) -> Result<Self, Self::Error> {
        use Namespace::*;
        let namespace = match i32::from(id) {
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

#[test]
fn main() {
    let mut parser =
        parse_mediawiki_dump::parse(BufReader::new(Cursor::new(DUMP)));
    assert!(match parser.next() {
        Some(Ok(parse_mediawiki_dump::Page {
            format: Some(format),
            model: Some(model),
            namespace,
            redirect_title,
            text,
            title,
        })) =>
            format == "beta"
                && model == "gamma"
                && redirect_title == None
                && text == "delta"
                && title == "alpha"
                && namespace == NamespaceId::from(0),
        _ => false,
    });
    assert!(match parser.next() {
        Some(Ok(parse_mediawiki_dump::Page {
            format: None,
            model: None,
            namespace,
            redirect_title,
            text,
            title,
        })) =>
            text == "eta"
                && title == "epsilon"
                && namespace == NamespaceId::from(1)
                && redirect_title == Some("zeta".to_string()),
        _ => false,
    });
    assert!(parser.next().is_none());

    let mut parser = parse_mediawiki_dump::parse_with_namespace(
        BufReader::new(Cursor::new(DUMP)),
    );
    assert!(match parser.next() {
        Some(Ok(parse_mediawiki_dump::Page {
            format: Some(format),
            model: Some(model),
            namespace: Namespace::Main,
            redirect_title,
            text,
            title,
        })) =>
            format == "beta"
                && model == "gamma"
                && redirect_title == None
                && text == "delta"
                && title == "alpha",
        _ => false,
    });
    assert!(match parser.next() {
        Some(Ok(parse_mediawiki_dump::Page {
            format: None,
            model: None,
            namespace: Namespace::Talk,
            redirect_title,
            text,
            title,
        })) =>
            text == "eta"
                && title == "epsilon"
                && redirect_title == Some("zeta".to_string()),
        _ => false,
    });
    assert!(parser.next().is_none());
}
