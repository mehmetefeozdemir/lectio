//! Formula parser: the LaTeX subset of ADR-023 → a `Node` tree. Errors say
//! what is wrong in plain words and where (a character position), so the
//! live preview can explain them.

use super::symbols::{bold, double_struck, function, italic, space, symbol};
use super::{Accent, Class, MathError, Node};

#[derive(Clone, Copy, PartialEq)]
enum Stop {
    End,
    Brace,
    Bracket,
    Right,
}

#[derive(Clone, Copy, PartialEq)]
enum Alphabet {
    Italic,
    Bold,
    DoubleStruck,
}

pub(super) struct Parser {
    chars: Vec<(usize, char)>,
    pos: usize,
    len: usize,
    alphabet: Alphabet,
}

impl Parser {
    pub(super) fn new(src: &str) -> Self {
        Self { chars: src.char_indices().collect(), pos: 0, len: src.len(), alphabet: Alphabet::Italic }
    }

    fn at(&self) -> usize {
        self.chars.get(self.pos).map_or(self.len, |(i, _)| *i)
    }

    fn error<T>(&self, message: impl Into<String>) -> Result<T, MathError> {
        Err(MathError { message: message.into(), at: self.at() })
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).map(|(_, c)| *c)
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        self.pos += usize::from(c.is_some());
        c
    }

    fn skip_spaces(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    pub(super) fn formula(&mut self) -> Result<Node, MathError> {
        self.skip_spaces();
        if self.peek().is_none() {
            return self.error(r"Type a formula, for example \frac{a}{b}.");
        }
        Ok(Node::Row(self.row(Stop::End)?))
    }

    fn row(&mut self, stop: Stop) -> Result<Vec<Node>, MathError> {
        let mut nodes = Vec::new();
        loop {
            self.skip_spaces();
            let Some(c) = self.peek() else {
                return match stop {
                    Stop::End => Ok(nodes),
                    Stop::Brace => self.error("A { has no matching }."),
                    Stop::Bracket => self.error("A [ has no matching ]."),
                    Stop::Right => self.error(r"\left has no matching \right."),
                };
            };
            match c {
                '}' if stop == Stop::Brace => {
                    self.pos += 1;
                    return Ok(nodes);
                }
                '}' => return self.error("A } has no matching {."),
                ']' if stop == Stop::Bracket => {
                    self.pos += 1;
                    return Ok(nodes);
                }
                '^' | '_' => {
                    self.pos += 1;
                    let script = self.argument(&c.to_string())?;
                    attach(&mut nodes, c == '^', script).or_else(|m| self.error(m))?;
                }
                '{' => {
                    self.pos += 1;
                    nodes.push(Node::Row(self.row(Stop::Brace)?));
                }
                '\\' => match self.command()? {
                    Command::Node(node) => nodes.push(node),
                    Command::Nothing => {}
                    Command::Right if stop == Stop::Right => return Ok(nodes),
                    Command::Right => return self.error(r"\right has no matching \left."),
                },
                '&' => return self.error("Alignment (&) is not supported in notes yet."),
                '$' => return self.error("No $ signs needed: the whole note is a formula."),
                _ => {
                    self.pos += 1;
                    nodes.push(self.atom(c));
                }
            }
        }
    }

    /// The thing after `^`, `_` or a command: a `{group}`, one command, or one character.
    fn argument(&mut self, what: &str) -> Result<Node, MathError> {
        self.skip_spaces();
        match self.peek() {
            None | Some('}') | Some('^') | Some('_') => self.error(format!("{what} needs something after it.")),
            Some('{') => {
                self.pos += 1;
                Ok(Node::Row(self.row(Stop::Brace)?))
            }
            Some('\\') => match self.command()? {
                Command::Node(node) => Ok(node),
                _ => self.error(format!("{what} needs something after it.")),
            },
            Some(c) => {
                self.pos += 1;
                Ok(self.atom(c))
            }
        }
    }

    fn command_name(&mut self) -> String {
        self.pos += 1; // the backslash
        let mut name = String::new();
        while let Some(c) = self.peek().filter(char::is_ascii_alphabetic) {
            name.push(c);
            self.pos += 1;
        }
        if name.is_empty()
            && let Some(c) = self.next()
        {
            name.push(c);
        }
        name
    }

    fn command(&mut self) -> Result<Command, MathError> {
        let start = self.pos;
        let name = self.command_name();
        let node = match name.as_str() {
            "" => return self.error(r"A \ at the end needs a command name after it."),
            "\\" => return self.error(r"Line breaks (\\) are not supported in notes yet."),
            "begin" | "end" => {
                return self.error(r"\begin{…} environments (matrices, aligned lines) are not supported in notes yet.");
            }
            "right" => return Ok(Command::Right),
            "limits" | "nolimits" | "displaystyle" | "textstyle" => return Ok(Command::Nothing),
            "frac" | "dfrac" | "tfrac" => {
                let num = self.argument(r"\frac")?;
                let den = self.argument(r"\frac{…}")?;
                Node::Frac { num: Box::new(num), den: Box::new(den) }
            }
            "binom" => {
                let top = self.argument(r"\binom")?;
                let bottom = self.argument(r"\binom{…}")?;
                Node::Binom { top: Box::new(top), bottom: Box::new(bottom) }
            }
            "sqrt" => {
                self.skip_spaces();
                let index = if self.peek() == Some('[') {
                    self.pos += 1;
                    Some(Box::new(Node::Row(self.row(Stop::Bracket)?)))
                } else {
                    None
                };
                Node::Sqrt { index, body: Box::new(self.argument(r"\sqrt")?) }
            }
            "text" | "textrm" | "mathrm" | "operatorname" | "textit" => Node::Text(self.raw_group(&name)?),
            "mathbf" | "boldsymbol" => self.with_alphabet(Alphabet::Bold, &name)?,
            "mathbb" => self.with_alphabet(Alphabet::DoubleStruck, &name)?,
            "mathit" => self.with_alphabet(Alphabet::Italic, &name)?,
            "left" => {
                let open = self.delimiter()?;
                let body = self.row(Stop::Right)?;
                let close = self.delimiter()?;
                Node::Delimited { open, close, body: Box::new(Node::Row(body)) }
            }
            _ => {
                if let Some(kind) = accent(&name) {
                    Node::Accent { kind, body: Box::new(self.argument(&format!("\\{name}"))?) }
                } else if let Some(em) = space(&name) {
                    Node::Space(em)
                } else if let Some(limits) = function(&name) {
                    Node::Function { name, limits }
                } else if let Some((ch, class)) = symbol(&name) {
                    Node::Symbol { ch, class }
                } else {
                    self.pos = start;
                    return self.error(format!("Unknown command \\{name}."));
                }
            }
        };
        Ok(Command::Node(node))
    }

    fn with_alphabet(&mut self, alphabet: Alphabet, name: &str) -> Result<Node, MathError> {
        let before = std::mem::replace(&mut self.alphabet, alphabet);
        let node = self.argument(&format!("\\{name}"));
        self.alphabet = before;
        node
    }

    /// `{raw text}` for `\text`: spaces kept, braces may nest.
    fn raw_group(&mut self, name: &str) -> Result<String, MathError> {
        self.skip_spaces();
        if self.next() != Some('{') {
            return self.error(format!("\\{name} needs its text in {{…}}."));
        }
        let mut depth = 0;
        let mut text = String::new();
        while let Some(c) = self.next() {
            match c {
                '{' => depth += 1,
                '}' if depth == 0 => return Ok(text),
                '}' => depth -= 1,
                '\\' => {
                    if let Some(escaped) = self.next() {
                        text.push(escaped);
                    }
                    continue;
                }
                _ => {}
            }
            text.push(c);
        }
        self.error(format!("\\{name}{{… has no matching }}."))
    }

    /// The bracket after `\left` or `\right`; `.` means none.
    fn delimiter(&mut self) -> Result<Option<char>, MathError> {
        self.skip_spaces();
        match self.peek() {
            Some('.') => {
                self.pos += 1;
                Ok(None)
            }
            Some(c @ ('(' | ')' | '[' | ']' | '|' | '/')) => {
                self.pos += 1;
                Ok(Some(c))
            }
            Some('\\') => {
                let name = self.command_name();
                match symbol(&name) {
                    Some((ch, Class::Open | Class::Close)) | Some((ch @ '‖', _)) => Ok(Some(ch)),
                    _ => self.error(format!(r"\{name} can't be used as a bracket after \left or \right.")),
                }
            }
            _ => self.error(r"\left and \right need a bracket after them, like \left( or \right]."),
        }
    }

    fn atom(&self, c: char) -> Node {
        let symbol = |ch, class| Node::Symbol { ch, class };
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => {
                let ch = match self.alphabet {
                    Alphabet::Italic if c.is_ascii_alphabetic() => italic(c),
                    Alphabet::Italic => c,
                    Alphabet::Bold => bold(c),
                    Alphabet::DoubleStruck => double_struck(c),
                };
                symbol(ch, Class::Ord)
            }
            '+' => symbol('+', Class::Bin),
            '-' => symbol('−', Class::Bin),
            '*' => symbol('∗', Class::Bin),
            '=' | '<' | '>' | ':' => symbol(c, Class::Rel),
            '(' | '[' => symbol(c, Class::Open),
            ')' | ']' | '!' => symbol(c, Class::Close),
            ',' | ';' => symbol(c, Class::Punct),
            '\'' => symbol('′', Class::Ord),
            '~' => Node::Space(1.0 / 3.0),
            _ => symbol(c, Class::Ord),
        }
    }
}

enum Command {
    Node(Node),
    /// A command that changes nothing on its own (`\limits`).
    Nothing,
    /// `\right`: ends the row started by `\left`.
    Right,
}

fn accent(name: &str) -> Option<Accent> {
    Some(match name {
        "hat" | "widehat" => Accent::Hat,
        "bar" | "overline" => Accent::Bar,
        "vec" | "overrightarrow" => Accent::Vec,
        "dot" => Accent::Dot,
        "ddot" => Accent::Ddot,
        "tilde" | "widetilde" => Accent::Tilde,
        "underline" => Accent::Underline,
        _ => return None,
    })
}

/// Puts a superscript or subscript on the last node (or on nothing).
fn attach(nodes: &mut Vec<Node>, superscript: bool, script: Node) -> Result<(), String> {
    let last = nodes.pop().unwrap_or(Node::Row(Vec::new()));
    let (base, mut sub, mut sup) = match last {
        Node::Scripts { base, sub, sup } => (base, sub, sup),
        other => (Box::new(other), None, None),
    };
    let slot = if superscript { &mut sup } else { &mut sub };
    if slot.is_some() {
        let which = if superscript { "superscripts" } else { "subscripts" };
        return Err(format!("Two {which} in a row; use braces, like {{x^a}}^b."));
    }
    *slot = Some(Box::new(script));
    nodes.push(Node::Scripts { base, sub, sup });
    Ok(())
}
