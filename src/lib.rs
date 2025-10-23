use anyhow::Result;
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use std::io::Cursor;

pub fn starmath_to_mathml(starmath: &str) -> Result<String> {
    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);

    // XML declaration
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;
    writer.write_event(Event::Text(BytesText::new("\n")))?;

    // Root math element
    let mut math = BytesStart::new("math");
    math.push_attribute(("xmlns", "http://www.w3.org/1998/Math/MathML"));
    math.push_attribute(("display", "block"));
    writer.write_event(Event::Start(math))?;
    writer.write_event(Event::Text(BytesText::new("\n  ")))?;

    // Semantics element
    writer.write_event(Event::Start(BytesStart::new("semantics")))?;
    writer.write_event(Event::Text(BytesText::new("\n    ")))?;

    // mrow element
    writer.write_event(Event::Start(BytesStart::new("mrow")))?;
    writer.write_event(Event::Text(BytesText::new("\n      ")))?;

    // Parse StarMath and generate MathML
    parse_starmath(&mut writer, starmath)?;

    writer.write_event(Event::Text(BytesText::new("\n    ")))?;
    writer.write_event(Event::End(BytesEnd::new("mrow")))?;
    writer.write_event(Event::Text(BytesText::new("\n    ")))?;

    // Annotation with original StarMath
    // Decode the input for the annotation to match expected format
    let decoded = decode_html_entities(starmath);
    let mut annotation = BytesStart::new("annotation");
    annotation.push_attribute(("encoding", "StarMath 5.0"));
    writer.write_event(Event::Start(annotation))?;

    // If the input is long, use multiline formatting
    if decoded.len() > 70 {
        writer.write_event(Event::Text(BytesText::new("\n      ")))?;
        writer.write_event(Event::Text(BytesText::new(&decoded)))?;
    } else {
        writer.write_event(Event::Text(BytesText::new(&decoded)))?;
    }

    writer.write_event(Event::End(BytesEnd::new("annotation")))?;
    writer.write_event(Event::Text(BytesText::new("\n  ")))?;

    // Close semantics
    writer.write_event(Event::End(BytesEnd::new("semantics")))?;
    writer.write_event(Event::Text(BytesText::new("\n")))?;

    // Close math
    writer.write_event(Event::End(BytesEnd::new("math")))?;
    writer.write_event(Event::Text(BytesText::new("\n")))?;

    let result = writer.into_inner().into_inner();
    Ok(String::from_utf8(result)?)
}

fn parse_starmath(writer: &mut Writer<Cursor<Vec<u8>>>, input: &str) -> Result<()> {
    let tokens = tokenize(input);
    let mut parser = Parser::new(tokens);
    parser.parse_expression(writer, 0)?;
    Ok(())
}

#[derive(Debug, Clone)]
enum Token {
    Word(String),
    LBrace,
    RBrace,
    String(String),
}

fn tokenize(input: &str) -> Vec<Token> {
    // Decode HTML entities first
    let decoded = decode_html_entities(input);

    let mut tokens = Vec::new();
    let mut chars = decoded.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' => {
                chars.next();
            }
            '{' => {
                chars.next();
                tokens.push(Token::LBrace);
            }
            '}' => {
                chars.next();
                tokens.push(Token::RBrace);
            }
            '"' => {
                chars.next();
                let mut string = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch == '"' {
                        chars.next();
                        break;
                    }
                    string.push(chars.next().unwrap());
                }
                tokens.push(Token::String(string));
            }
            _ => {
                let mut word = String::new();
                while let Some(&ch) = chars.peek() {
                    if [' ', '{', '}', '"', '\t', '\n'].contains(&ch) {
                        break;
                    }
                    word.push(chars.next().unwrap());
                }
                if !word.is_empty() {
                    tokens.push(Token::Word(word));
                }
            }
        }
    }

    tokens
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.pos);
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn parse_expression(
        &mut self,
        writer: &mut Writer<Cursor<Vec<u8>>>,
        depth: usize,
    ) -> Result<()> {
        let mut first = true;
        let indent = "  ".repeat(3 + depth);

        while let Some(token) = self.peek() {
            if matches!(token, Token::RBrace) {
                return Ok(());
            }

            if !first {
                writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;
            }
            first = false;

            self.parse_element(writer, depth)?;
        }
        Ok(())
    }

    fn parse_element(&mut self, writer: &mut Writer<Cursor<Vec<u8>>>, depth: usize) -> Result<()> {
        let token = match self.peek() {
            Some(t) => t.clone(),
            None => return Ok(()),
        };

        match token {
            Token::Word(ref word) => match word.as_str() {
                "acute" => self.parse_accent(writer, "´", depth)?,
                "sqrt" => self.parse_sqrt(writer, depth)?,
                "sum" => self.parse_sum(writer, depth)?,
                "left" => self.parse_left_fence(writer, depth)?,
                "right" => {
                    self.advance();
                    // Skip the closing parenthesis
                    self.advance();
                }
                "±" | "+-" | "−" | "-" | "×" | "*" | "times" => {
                    self.advance();
                    writer.write_event(Event::Start(BytesStart::new("mo")))?;
                    writer.write_event(Event::Text(BytesText::new(word)))?;
                    writer.write_event(Event::End(BytesEnd::new("mo")))?;
                }
                _ => {
                    self.advance();
                    // Determine if this is a number or identifier
                    let is_number = word.chars().all(|c| c.is_ascii_digit() || c == ',');
                    if is_number {
                        writer.write_event(Event::Start(BytesStart::new("mn")))?;
                        writer.write_event(Event::Text(BytesText::new(word)))?;
                        writer.write_event(Event::End(BytesEnd::new("mn")))?;
                    } else {
                        // Check if this is a standard mathematical function (should be upright)
                        let is_function = matches!(
                            word.as_str(),
                            "sin"
                                | "cos"
                                | "tan"
                                | "sec"
                                | "csc"
                                | "cot"
                                | "sinh"
                                | "cosh"
                                | "tanh"
                                | "sech"
                                | "csch"
                                | "coth"
                                | "arcsin"
                                | "arccos"
                                | "arctan"
                                | "arcsec"
                                | "arccsc"
                                | "arccot"
                                | "log"
                                | "ln"
                                | "lg"
                                | "exp"
                                | "lim"
                                | "sup"
                                | "inf"
                                | "max"
                                | "min"
                                | "det"
                                | "dim"
                                | "ker"
                                | "deg"
                                | "gcd"
                                | "lcm"
                                | "Pr"
                                | "hom"
                                | "arg"
                                | "mod"
                        );

                        // All multi-character identifiers (except functions) get mathvariant="italic"
                        // Single-character variables don't need it (already italic by default in MathML)
                        let mut mi = BytesStart::new("mi");
                        if word.len() > 1 && !is_function {
                            mi.push_attribute(("mathvariant", "italic"));
                        }
                        writer.write_event(Event::Start(mi))?;
                        writer.write_event(Event::Text(BytesText::new(word)))?;
                        writer.write_event(Event::End(BytesEnd::new("mi")))?;
                    }
                }
            },
            Token::String(ref s) => {
                self.advance();
                writer.write_event(Event::Start(BytesStart::new("mtext")))?;
                writer.write_event(Event::Text(BytesText::new(s)))?;
                writer.write_event(Event::End(BytesEnd::new("mtext")))?;
            }
            Token::LBrace => {
                self.advance();
                self.parse_group(writer, depth)?;
            }
            Token::RBrace => {
                return Ok(());
            }
        }
        Ok(())
    }

    fn parse_group(&mut self, writer: &mut Writer<Cursor<Vec<u8>>>, depth: usize) -> Result<()> {
        // Look ahead to see what follows this group
        let group_start = self.pos;
        let mut brace_count = 1;
        let mut temp_pos = self.pos;

        while temp_pos < self.tokens.len() && brace_count > 0 {
            match &self.tokens[temp_pos] {
                Token::LBrace => brace_count += 1,
                Token::RBrace => brace_count -= 1,
                _ => {}
            }
            temp_pos += 1;
        }

        let group_end = temp_pos;

        // Check what follows
        if let Some(Token::Word(op)) = self.tokens.get(group_end) {
            match op.as_str() {
                "rsub" => {
                    let indent = "  ".repeat(4 + depth);
                    writer.write_event(Event::Start(BytesStart::new("msub")))?;
                    writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

                    let mut sub_parser = Parser {
                        tokens: self.tokens[group_start..group_end - 1].to_vec(),
                        pos: 0,
                    };
                    sub_parser.parse_expression(writer, depth + 1)?;

                    writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

                    self.pos = group_end + 1; // Skip past rsub
                    self.parse_element(writer, depth + 1)?;

                    let parent_indent = "  ".repeat(3 + depth);
                    writer.write_event(Event::Text(BytesText::new(&format!(
                        "\n{}",
                        parent_indent
                    ))))?;
                    writer.write_event(Event::End(BytesEnd::new("msub")))?;

                    // Skip the closing brace of the subscript if it exists
                    if matches!(self.peek(), Some(Token::RBrace)) {
                        self.advance();
                    }

                    return Ok(());
                }
                "^" => {
                    let indent = "  ".repeat(4 + depth);
                    writer.write_event(Event::Start(BytesStart::new("msup")))?;
                    writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

                    let mut sub_parser = Parser {
                        tokens: self.tokens[group_start..group_end - 1].to_vec(),
                        pos: 0,
                    };
                    sub_parser.parse_expression(writer, depth + 1)?;

                    writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

                    self.pos = group_end + 1; // Skip past ^
                    self.parse_element(writer, depth + 1)?;

                    let parent_indent = "  ".repeat(3 + depth);
                    writer.write_event(Event::Text(BytesText::new(&format!(
                        "\n{}",
                        parent_indent
                    ))))?;
                    writer.write_event(Event::End(BytesEnd::new("msup")))?;

                    // Skip the closing brace of the superscript if it exists
                    if matches!(self.peek(), Some(Token::RBrace)) {
                        self.advance();
                    }

                    return Ok(());
                }
                "over" => {
                    let indent = "  ".repeat(4 + depth);
                    writer.write_event(Event::Start(BytesStart::new("mfrac")))?;
                    writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

                    let mut sub_parser = Parser {
                        tokens: self.tokens[group_start..group_end - 1].to_vec(),
                        pos: 0,
                    };
                    sub_parser.parse_expression(writer, depth + 1)?;

                    writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

                    self.pos = group_end + 1; // Skip past over
                    self.parse_element(writer, depth + 1)?;

                    let parent_indent = "  ".repeat(3 + depth);
                    writer.write_event(Event::Text(BytesText::new(&format!(
                        "\n{}",
                        parent_indent
                    ))))?;
                    writer.write_event(Event::End(BytesEnd::new("mfrac")))?;

                    // Skip the closing brace of the denominator if it exists
                    if matches!(self.peek(), Some(Token::RBrace)) {
                        self.advance();
                    }

                    return Ok(());
                }
                _ => {}
            }
        }

        // Regular group - parse its contents
        let mut sub_parser = Parser {
            tokens: self.tokens[group_start..group_end - 1].to_vec(),
            pos: 0,
        };
        sub_parser.parse_expression(writer, depth)?;

        self.pos = group_end;

        Ok(())
    }

    fn parse_accent(
        &mut self,
        writer: &mut Writer<Cursor<Vec<u8>>>,
        accent: &str,
        depth: usize,
    ) -> Result<()> {
        self.advance(); // skip "acute"

        let indent = "  ".repeat(4 + depth);

        let mut attr = BytesStart::new("mover");
        attr.push_attribute(("accent", "true"));
        writer.write_event(Event::Start(attr))?;
        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

        self.parse_element(writer, depth + 1)?;

        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

        let mut mo = BytesStart::new("mo");
        mo.push_attribute(("stretchy", "false"));
        writer.write_event(Event::Start(mo))?;
        writer.write_event(Event::Text(BytesText::new(accent)))?;
        writer.write_event(Event::End(BytesEnd::new("mo")))?;

        let parent_indent = "  ".repeat(3 + depth);
        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", parent_indent))))?;
        writer.write_event(Event::End(BytesEnd::new("mover")))?;

        Ok(())
    }

    fn parse_sqrt(&mut self, writer: &mut Writer<Cursor<Vec<u8>>>, depth: usize) -> Result<()> {
        self.advance(); // skip "sqrt"

        let indent = "  ".repeat(4 + depth);

        writer.write_event(Event::Start(BytesStart::new("msqrt")))?;
        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

        self.parse_element(writer, depth + 1)?;

        let parent_indent = "  ".repeat(3 + depth);
        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", parent_indent))))?;
        writer.write_event(Event::End(BytesEnd::new("msqrt")))?;

        Ok(())
    }

    fn parse_sum(&mut self, writer: &mut Writer<Cursor<Vec<u8>>>, depth: usize) -> Result<()> {
        self.advance(); // skip "sum"

        let indent = "  ".repeat(4 + depth);

        writer.write_event(Event::Start(BytesStart::new("mrow")))?;
        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

        let mut mo = BytesStart::new("mo");
        mo.push_attribute(("stretchy", "false"));
        writer.write_event(Event::Start(mo))?;
        writer.write_event(Event::Text(BytesText::new("∑")))?;
        writer.write_event(Event::End(BytesEnd::new("mo")))?;

        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

        writer.write_event(Event::Start(BytesStart::new("mrow")))?;
        writer.write_event(Event::Text(BytesText::new(&format!(
            "\n{}",
            "  ".repeat(5 + depth)
        ))))?;

        self.parse_element(writer, depth + 2)?;

        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;
        writer.write_event(Event::End(BytesEnd::new("mrow")))?;

        let parent_indent = "  ".repeat(3 + depth);
        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", parent_indent))))?;
        writer.write_event(Event::End(BytesEnd::new("mrow")))?;

        Ok(())
    }

    fn parse_left_fence(
        &mut self,
        writer: &mut Writer<Cursor<Vec<u8>>>,
        depth: usize,
    ) -> Result<()> {
        self.advance(); // skip "left"

        let indent = "  ".repeat(4 + depth);

        // Get the opening fence
        let fence = if let Some(Token::Word(f)) = self.peek() {
            f.clone()
        } else {
            return Ok(());
        };
        self.advance();

        writer.write_event(Event::Start(BytesStart::new("mrow")))?;
        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

        // Opening fence
        let mut mo = BytesStart::new("mo");
        mo.push_attribute(("fence", "true"));
        mo.push_attribute(("form", "prefix"));
        mo.push_attribute(("stretchy", "true"));
        writer.write_event(Event::Start(mo))?;
        writer.write_event(Event::Text(BytesText::new(&fence)))?;
        writer.write_event(Event::End(BytesEnd::new("mo")))?;

        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

        // Content inside fence
        writer.write_event(Event::Start(BytesStart::new("mrow")))?;
        writer.write_event(Event::Text(BytesText::new(&format!(
            "\n{}",
            "  ".repeat(5 + depth)
        ))))?;

        writer.write_event(Event::Start(BytesStart::new("mrow")))?;
        writer.write_event(Event::Text(BytesText::new(&format!(
            "\n{}",
            "  ".repeat(6 + depth)
        ))))?;

        // Parse until we hit "right"
        while let Some(token) = self.peek() {
            if let Token::Word(w) = token {
                if w == "right" {
                    break;
                }
            }
            self.parse_element(writer, depth + 3)?;

            if let Some(next_token) = self.peek() {
                if let Token::Word(w) = next_token {
                    if w != "right" {
                        writer.write_event(Event::Text(BytesText::new(&format!(
                            "\n{}",
                            "  ".repeat(6 + depth)
                        ))))?;
                    }
                }
            }
        }

        writer.write_event(Event::Text(BytesText::new(&format!(
            "\n{}",
            "  ".repeat(5 + depth)
        ))))?;
        writer.write_event(Event::End(BytesEnd::new("mrow")))?;

        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;
        writer.write_event(Event::End(BytesEnd::new("mrow")))?;

        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", indent))))?;

        // Closing fence
        self.advance(); // skip "right"
        let closing_fence = if let Some(Token::Word(f)) = self.peek() {
            f.clone()
        } else {
            return Ok(());
        };
        self.advance();

        let mut mo = BytesStart::new("mo");
        mo.push_attribute(("fence", "true"));
        mo.push_attribute(("form", "postfix"));
        mo.push_attribute(("stretchy", "true"));
        writer.write_event(Event::Start(mo))?;
        writer.write_event(Event::Text(BytesText::new(&closing_fence)))?;
        writer.write_event(Event::End(BytesEnd::new("mo")))?;

        let parent_indent = "  ".repeat(3 + depth);
        writer.write_event(Event::Text(BytesText::new(&format!("\n{}", parent_indent))))?;
        writer.write_event(Event::End(BytesEnd::new("mrow")))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_starmath_conversion() {
        let test_cases = vec![
            (
                r#"D &quot;=&quot; C − M × log 10 sum {cuatro plieges}"#,
                "test/test1.expected.mml",
            ),
            (
                r#"&quot;%&quot; grasacorporal &quot;=&quot; left ({4,95} over {densidad} &quot;-&quot; 4,5 right ) × 100"#,
                "test/test2.expected.mml",
            ),
            (
                r#"acute {X} "=" ± {Z} rsub {alfa} sqrt {{{s} ^ {2}} over {n}}"#,
                "test/test3.expected.mml",
            ),
            (
                r#"P &quot;=&quot; ± {Z} rsub {alfa} sqrt {{pq} over {n}}"#,
                "test/test4.expected.mml",
            ),
        ];

        for (input, expected_file) in test_cases {
            let expected = fs::read_to_string(expected_file)
                .unwrap_or_else(|_| panic!("Failed to read {}", expected_file));
            let actual = starmath_to_mathml(input).expect("Failed to convert StarMath to MathML");

            assert_eq!(
                actual.trim(),
                expected.trim(),
                "Mismatch for test file: {}",
                expected_file
            );
        }
    }
}
