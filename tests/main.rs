// Copyright 2018 Fredrik Portström <https://portstrom.com>
// This is free software distributed under the terms specified in
// the file LICENSE at the top-level directory of this distribution.

use std::io::{BufReader, Cursor};
use parse_mediawiki_dump::RawNamespace;

extern crate parse_mediawiki_dump;

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
        <ns>42</ns>
        <redirect title="zeta" />
        <revision>
            <text>eta</text>
        </revision>
    </page>
</mediawiki>"#;

enum Namespace {
    Zero,
    FortyTwo,
}

impl Into<Namespace> for RawNamespace {
    fn into(self) -> Namespace {
        match self {
            0 => Namespace::Zero,
            42 => Namespace::FortyTwo,
            _ => panic!("invalid namespace"),
        }
    }
}

// impl From<i32> for Namespace {
//     fn from(n: i32) -> Self {
//         match n {
//             0 => Namespace::Zero,
//             42 => Namespace::FortyTwo,
//             _ => panic!("invalid namespace"),
//         }
//     }
// }

#[test]
fn main() {
    let mut parser =
        parse_mediawiki_dump::parse(BufReader::new(Cursor::new(DUMP)));
    assert!(match parser.next() {
        Some(Ok(parse_mediawiki_dump::Page {
            format: Some(format),
            model: Some(model),
            namespace: 0,
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
            namespace: 42,
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

    let mut parser =
        parse_mediawiki_dump::parse_with_namespace(BufReader::new(Cursor::new(DUMP)));
    assert!(match parser.next() {
        Some(Ok(parse_mediawiki_dump::Page {
            format: Some(format),
            model: Some(model),
            namespace: Namespace::Zero,
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
            namespace: Namespace::FortyTwo,
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
