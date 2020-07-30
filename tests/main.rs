// Copyright 2018 Fredrik Portström <https://portstrom.com>
// This is free software distributed under the terms specified in
// the file LICENSE at the top-level directory of this distribution.

use parse_mediawiki_dump::{impl_namespace, NamespaceId};
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

impl_namespace! {
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

#[test]
fn main() {
    let mut parser =
        parse_mediawiki_dump::parse(BufReader::new(Cursor::new(DUMP)));
    assert!(match parser.next() {
        Some(Ok(parse_mediawiki_dump::Page {
            format: Some(format),
            model: Some(model),
            namespace: NamespaceId(0),
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
            namespace: NamespaceId(1),
            redirect_title,
            text,
            title,
        })) =>
            redirect_title == Some("zeta".to_string())
                && text == "eta"
                && title == "epsilon",
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
