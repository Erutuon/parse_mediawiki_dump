// Copyright 2018 Fredrik Portström <https://portstrom.com>
// This is free software distributed under the terms specified in
// the file LICENSE at the top-level directory of this distribution.

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

#[test]
fn main() {
    let mut parser = parse_mediawiki_dump::parse(std::io::BufReader::new(
        std::io::Cursor::new(DUMP),
    ));
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
}
