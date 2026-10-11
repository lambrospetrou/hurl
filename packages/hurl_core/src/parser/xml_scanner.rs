/*
 * Hurl (https://hurl.dev)
 * Copyright (C) 2026 Orange
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *          http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 */
//! XML body parser without libxml2 (feature `libxml` disabled), used in place of `xml.rs`.
//!
//! It finds the end of the XML body with a simple scanner, and does not check DTDs, entities or
//! encodings.
use crate::parser::{ParseError, ParseErrorKind, ParseResult};
use crate::reader::{Pos, Reader};

/// Parses a XML body from `reader`, see `xml.rs`.
pub fn parse(reader: &mut Reader) -> ParseResult<String> {
    // We test if our first character is a start of an XML text.
    // If not, we return immediately a recoverable error.
    // Otherwise, we start parsing the supposed XML buffer. Any subsequent error will be a
    // non-recoverable error.
    let c = reader.peek();
    match c {
        Some('<') => {}
        _ => {
            return Err(ParseError::new(
                reader.cursor().pos,
                true,
                ParseErrorKind::Xml,
            ));
        }
    }
    parse_root(reader)
}

struct Scanner<'a> {
    reader: &'a mut Reader,
    buf: String,
    prev_pos: Pos,
}

impl Scanner<'_> {
    fn next(&mut self) -> ParseResult<char> {
        let pos = self.reader.cursor().pos;
        match self.reader.read() {
            Some(c) => {
                self.prev_pos = pos;
                self.buf.push(c);
                Ok(c)
            }
            None => Err(self.error()),
        }
    }

    fn error(&self) -> ParseError {
        ParseError::new(self.prev_pos, false, ParseErrorKind::Xml)
    }

    fn expect(&mut self, s: &str) -> ParseResult<()> {
        for expected in s.chars() {
            if self.next()? != expected {
                return Err(self.error());
            }
        }
        Ok(())
    }

    /// Reads chars until `end` has been read.
    fn skip_until(&mut self, end: &str) -> ParseResult<()> {
        while !self.buf.ends_with(end) {
            self.next()?;
        }
        Ok(())
    }

    /// Reads a name whose first char is `first`, and returns the name and the next char.
    fn name(&mut self, first: char) -> ParseResult<(String, char)> {
        let mut name = first.to_string();
        loop {
            let c = self.next()?;
            if is_name_char(c) {
                name.push(c);
            } else {
                return Ok((name, c));
            }
        }
    }

    fn skip_whitespace(&mut self, mut c: char) -> ParseResult<char> {
        while c.is_whitespace() {
            c = self.next()?;
        }
        Ok(c)
    }

    /// Reads a `<!DOCTYPE` declaration until its closing `>`.
    fn doctype(&mut self) -> ParseResult<()> {
        let mut brackets = 0;
        let mut quote = None;
        loop {
            let c = self.next()?;
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '[') => brackets += 1,
                (None, ']') => brackets -= 1,
                (None, '>') if brackets == 0 => return Ok(()),
                _ => {}
            }
        }
    }

    /// Reads the attributes of a start tag, the first char after the tag name being `c`.
    /// Returns `true` if the element is empty (`<a/>`).
    fn attributes(&mut self, mut c: char) -> ParseResult<bool> {
        loop {
            if c == '>' {
                return Ok(false);
            }
            if c == '/' {
                if self.next()? != '>' {
                    return Err(self.error());
                }
                return Ok(true);
            }
            if !c.is_whitespace() {
                return Err(self.error());
            }
            c = self.skip_whitespace(c)?;
            if c == '>' || c == '/' {
                continue;
            }
            if !is_name_start_char(c) {
                return Err(self.error());
            }
            let (_, next) = self.name(c)?;
            c = self.skip_whitespace(next)?;
            if c != '=' {
                return Err(self.error());
            }
            let c2 = self.next()?;
            let quote = self.skip_whitespace(c2)?;
            if quote != '"' && quote != '\'' {
                return Err(self.error());
            }
            loop {
                let v = self.next()?;
                if v == quote {
                    break;
                }
                if v == '<' {
                    return Err(self.error());
                }
            }
            c = self.next()?;
        }
    }
}

fn is_name_start_char(c: char) -> bool {
    is_name_char(c) && !c.is_ascii_digit() && c != '-' && c != '.'
}

fn is_name_char(c: char) -> bool {
    !c.is_whitespace() && !matches!(c, '<' | '>' | '/' | '=' | '"' | '\'' | '?' | '!')
}

/// Parses `reader` until the root element is closed. The first char is `<`.
fn parse_root(reader: &mut Reader) -> ParseResult<String> {
    let prev_pos = reader.cursor().pos;
    let mut s = Scanner {
        reader,
        buf: String::new(),
        prev_pos,
    };
    let mut stack: Vec<String> = vec![];
    loop {
        let c = s.next()?;
        if c != '<' {
            if stack.is_empty() && !c.is_whitespace() {
                return Err(s.error());
            }
            continue;
        }
        match s.next()? {
            '?' => s.skip_until("?>")?,
            '!' => match s.next()? {
                '-' => {
                    s.expect("-")?;
                    s.skip_until("-->")?;
                }
                '[' if !stack.is_empty() => {
                    s.expect("CDATA[")?;
                    s.skip_until("]]>")?;
                }
                'D' if stack.is_empty() => {
                    s.expect("OCTYPE")?;
                    s.doctype()?;
                }
                _ => return Err(s.error()),
            },
            '/' => {
                let first = s.next()?;
                if !is_name_start_char(first) {
                    return Err(s.error());
                }
                let (name, next) = s.name(first)?;
                if s.skip_whitespace(next)? != '>' {
                    return Err(s.error());
                }
                if stack.pop().as_ref() != Some(&name) {
                    return Err(s.error());
                }
                if stack.is_empty() {
                    return Ok(s.buf);
                }
            }
            c if is_name_start_char(c) => {
                let (name, next) = s.name(c)?;
                let empty = s.attributes(next)?;
                if !empty {
                    stack.push(name);
                } else if stack.is_empty() {
                    return Ok(s.buf);
                }
            }
            _ => return Err(s.error()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::{CharPos, Pos};

    #[test]
    fn parse_xml_brute_force_errors() {
        let mut reader = Reader::new("");
        let error = parse(&mut reader).err().unwrap();
        assert_eq!(error.pos, Pos { line: 1, column: 1 });
        assert_eq!(error.kind, ParseErrorKind::Xml);
        assert!(error.recoverable);

        let mut reader = Reader::new("x");
        let error = parse(&mut reader).err().unwrap();
        assert_eq!(error.pos, Pos { line: 1, column: 1 });
        assert_eq!(error.kind, ParseErrorKind::Xml);
        assert!(error.recoverable);

        let mut reader = Reader::new("<<");
        let error = parse(&mut reader).err().unwrap();
        assert_eq!(error.pos, Pos { line: 1, column: 2 });
        assert_eq!(error.kind, ParseErrorKind::Xml);
        assert!(!error.recoverable);

        let mut reader = Reader::new("<users><user /></users");
        let error = parse(&mut reader).err().unwrap();
        assert_eq!(
            error.pos,
            Pos {
                line: 1,
                column: 22
            }
        );
        assert_eq!(error.kind, ParseErrorKind::Xml);

        let mut reader = Reader::new("<users aa><user /></users");
        let error = parse(&mut reader).err().unwrap();
        assert_eq!(
            error.pos,
            Pos {
                line: 1,
                column: 10
            }
        );
        assert_eq!(error.kind, ParseErrorKind::Xml);
    }

    #[test]
    fn parse_xml_brute_force_ok() {
        let mut reader = Reader::new("<users><user /></users>");
        assert_eq!(
            parse(&mut reader).unwrap(),
            String::from("<users><user /></users>")
        );
        assert_eq!(reader.cursor().index, CharPos(23));

        let mut reader = Reader::new("<users><user /></users>xx");
        assert_eq!(
            parse(&mut reader).unwrap(),
            String::from("<users><user /></users>")
        );
        assert_eq!(reader.cursor().index, CharPos(23));
        assert_eq!(reader.peek_n(2), String::from("xx"));

        let mut reader = Reader::new("<?xml version=\"1.0\"?><users/>xxx");
        assert_eq!(
            parse(&mut reader).unwrap(),
            String::from("<?xml version=\"1.0\"?><users/>")
        );
        assert_eq!(reader.cursor().index, CharPos(29));
    }

    #[test]
    fn parse_xml_soap_utf8() {
        let xml = r#"<?xml version='1.0' encoding='UTF-8'?>
<soapenv:Envelope xmlns:soapenv="http://schemas.xmlsoap.org/soap/envelope/">
    <soapenv:Body>
        <ns31:UploadInboundResponseElement xmlns:ns31="http://www.example.com/schema/xyzWS">
            <ns31:UploadInboundResult>&lt;?xml version="1.0" encoding="UTF-8" ?>&lt;ATKCST>&lt;Head>&lt;FunCode>9000&lt;/FunCode>&lt;Remark>接收数据成功&lt;/Remark>&lt;/Head>&lt;/ATKCST></ns31:UploadInboundResult>
        </ns31:UploadInboundResponseElement>
    </soapenv:Body>
</soapenv:Envelope>"#;

        // A valid complete XML
        let input = xml;
        let output = xml;
        let mut reader = Reader::new(input);
        assert_eq!(parse(&mut reader).unwrap(), String::from(output),);
        assert_eq!(reader.cursor().index, CharPos(520));

        // A XML with data padding
        let input = format!("{xml} xx xx xx xx");
        let output = xml;
        let mut reader = Reader::new(&input);
        assert_eq!(parse(&mut reader).unwrap(), String::from(output),);
        assert_eq!(reader.cursor().index, CharPos(520));

        // Two consecutive XML
        let input = format!("{xml}{xml}");
        let output = xml;
        let mut reader = Reader::new(&input);
        assert_eq!(parse(&mut reader).unwrap(), String::from(output),);
        assert_eq!(reader.cursor().index, CharPos(520));

        let mut reader = Reader::new(&input);
        assert_eq!(parse(&mut reader).unwrap(), String::from(output),);
        assert_eq!(reader.cursor().index, CharPos(520));
    }

    #[test]
    fn parse_xml_books_with_entry_response_start() {
        let xml = r#"<?xml version="1.0"?>
<catalog>
   <book id="bk101">
      <author>Gambardella, Matthew</author>
      <title>XML Developer's Guide</title>
      <genre>Computer</genre>
      <price>44.95</price>
      <publish_date>2000-10-01</publish_date>
      <description>An in-depth look at creating applications
      with XML.</description>
   </book>
   <book id="bk102">
      <author>Ralls, Kim</author>
      <title>Midnight Rain</title>
      <genre>Fantasy</genre>
      <price>5.95</price>
      <publish_date>2000-12-16</publish_date>
      <description>A former architect battles corporate zombies,
      an evil sorceress, and her own childhood to become queen
      of the world.</description>
   </book>
   <book id="bk103">
      <author>Corets, Eva</author>
      <title>Maeve Ascendant</title>
      <genre>Fantasy</genre>
      <price>5.95</price>
      <publish_date>2000-11-17</publish_date>
      <description>After the collapse of a nanotechnology
      society in England, the young survivors lay the
      foundation for a new society.</description>
   </book>
   <book id="bk104">
      <author>Corets, Eva</author>
      <title>Oberon's Legacy</title>
      <genre>Fantasy</genre>
      <price>5.95</price>
      <publish_date>2001-03-10</publish_date>
      <description>In post-apocalypse England, the mysterious
      agent known only as Oberon helps to create a new life
      for the inhabitants of London. Sequel to Maeve
      Ascendant.</description>
   </book>
   <book id="bk105">
      <author>Corets, Eva</author>
      <title>The Sundered Grail</title>
      <genre>Fantasy</genre>
      <price>5.95</price>
      <publish_date>2001-09-10</publish_date>
      <description>The two daughters of Maeve, half-sisters,
      battle one another for control of England. Sequel to
      Oberon's Legacy.</description>
   </book>
   <book id="bk106">
      <author>Randall, Cynthia</author>
      <title>Lover Birds</title>
      <genre>Romance</genre>
      <price>4.95</price>
      <publish_date>2000-09-02</publish_date>
      <description>When Carla meets Paul at an ornithology
      conference, tempers fly as feathers get ruffled.</description>
   </book>
   <book id="bk107">
      <author>Thurman, Paula</author>
      <title>Splish Splash</title>
      <genre>Romance</genre>
      <price>4.95</price>
      <publish_date>2000-11-02</publish_date>
      <description>A deep sea diver finds true love twenty
      thousand leagues beneath the sea.</description>
   </book>
   <book id="bk108">
      <author>Knorr, Stefan</author>
      <title>Creepy Crawlies</title>
      <genre>Horror</genre>
      <price>4.95</price>
      <publish_date>2000-12-06</publish_date>
      <description>An anthology of horror stories about roaches,
      centipedes, scorpions  and other insects.</description>
   </book>
   <book id="bk109">
      <author>Kress, Peter</author>
      <title>Paradox Lost</title>
      <genre>Science Fiction</genre>
      <price>6.95</price>
      <publish_date>2000-11-02</publish_date>
      <description>After an inadvertent trip through a Heisenberg
      Uncertainty Device, James Salway discovers the problems
      of being quantum.</description>
   </book>
   <book id="bk110">
      <author>O'Brien, Tim</author>
      <title>Microsoft .NET: The Programming Bible</title>
      <genre>Computer</genre>
      <price>36.95</price>
      <publish_date>2000-12-09</publish_date>
      <description>Microsoft's .NET initiative is explored in
      detail in this deep programmer's reference.</description>
   </book>
   <book id="bk111">
      <author>O'Brien, Tim</author>
      <title>MSXML3: A Comprehensive Guide</title>
      <genre>Computer</genre>
      <price>36.95</price>
      <publish_date>2000-12-01</publish_date>
      <description>The Microsoft MSXML3 parser is covered in
      detail, with attention to XML DOM interfaces, XSLT processing,
      SAX and more.</description>
   </book>
   <book id="bk112">
      <author>Galos, Mike</author>
      <title>Visual Studio 7: A Comprehensive Guide</title>
      <genre>Computer</genre>
      <price>49.95</price>
      <publish_date>2001-04-16</publish_date>
      <description>Microsoft Visual Studio 7 is explored in depth,
      looking at how Visual Basic, Visual C++, C#, and ASP+ are
      integrated into a comprehensive development
      environment.</description>
   </book>
</catalog>"#;

        let chunk = format!("{xml}\nHTTP 200");
        let mut reader = Reader::new(&chunk);
        assert_eq!(parse(&mut reader).unwrap(), String::from(xml),);
        assert_eq!(reader.cursor().index, CharPos(4411));
    }
}
