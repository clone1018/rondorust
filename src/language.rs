use crate::{Error, Result};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(crate) enum Expr {
    Num(f64),
    Ref(String),
    Word(String),
    Bin(char, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>, BTreeMap<String, Expr>),
    Map(Box<Expr>, Box<Expr>, Box<Expr>),
    Knob(f64, f64, f64),
    Curved(Box<Expr>, Box<Expr>),
}
impl Expr {
    fn bin(op: char, left: Self, right: Self) -> Self {
        Self::Bin(op, Box::new(left), Box::new(right))
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Line {
    pub line: usize,
    pub indent: usize,
    pub text: String,
}
impl Line {
    pub fn error(&self, message: impl Into<String>) -> Error {
        Error::at(self.line, self.indent + 1, message)
    }
    pub fn word(&self) -> &str {
        self.text.split_whitespace().next().unwrap_or("")
    }
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Chain {
    pub bindings: BTreeMap<String, Expr>,
    pub output: Option<Expr>,
}
#[derive(Clone, Debug)]
pub(crate) struct Synth {
    pub name: String,
    pub chain: Chain,
    pub post: Option<Chain>,
    pub options: BTreeMap<String, f64>,
}
#[derive(Clone, Debug)]
pub(crate) struct Play {
    pub header: Line,
    pub body: Vec<Line>,
}
#[derive(Clone, Debug)]
pub(crate) struct Bus {
    pub name: String,
    pub chain: Chain,
    pub sends: BTreeMap<String, f64>,
}
#[derive(Clone, Debug)]
pub(crate) struct Section {
    pub name: String,
    pub len: f64,
    pub layers: Vec<String>,
    pub plays: Vec<Play>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Program {
    pub synths: Vec<Synth>,
    pub plays: Vec<Play>,
    pub buses: Vec<Bus>,
    pub sections: Vec<Section>,
    pub directives: Vec<Line>,
}

pub(crate) fn parse(source: &str) -> Result<Program> {
    if source.len() > 1_048_576 {
        return Err(Error::invalid("source must be at most 1 MiB"));
    }
    let mut lines = Vec::new();
    for (i, raw) in source.lines().enumerate() {
        let mut quote = None;
        let mut previous = ' ';
        let mut end = raw.len();
        for (at, c) in raw.char_indices() {
            if matches!(c, '"' | '`') {
                if quote == Some(c) {
                    quote = None
                } else if quote.is_none() {
                    quote = Some(c)
                }
            }
            if c == '#' && previous.is_whitespace() && quote.is_none() {
                end = at;
                break;
            }
            previous = c;
        }
        let text = raw[..end].trim_end();
        if text.trim().is_empty() {
            continue;
        }
        let indent = text.len() - text.trim_start().len();
        if text[..indent].contains('\t') {
            return Err(Error::at(i + 1, 1, "use spaces for indentation"));
        }
        lines.push(Line {
            line: i + 1,
            indent,
            text: text[indent..].to_owned(),
        });
    }
    let mut program = Program::default();
    let mut at = 0;
    while at < lines.len() {
        let header = &lines[at];
        if header.indent != 0 {
            return Err(header.error("unexpected indentation at top level"));
        }
        let end = block_end(&lines, at);
        let body = &lines[at + 1..end];
        match header.word() {
            "synth" => program.synths.push(synth(header, body)?),
            "play" | "beat" => program.plays.push(play(header, body)?),
            "bus" => {
                let name = identifier(header, 1)?;
                if header.text.split_whitespace().count() != 2 {
                    return Err(header.error("bus expects a name and no extra arguments"));
                }
                let mut sends = BTreeMap::new();
                let mut fx = Vec::new();
                for line in body {
                    if line.word() == "send" {
                        let target = identifier(line, 1)?;
                        if line.text.split_whitespace().count() != 3 {
                            return Err(line.error("send expects a synth name and an amount"));
                        }
                        let amount = line
                            .text
                            .split_whitespace()
                            .nth(2)
                            .ok_or_else(|| line.error("send needs an amount"))?;
                        let amount = number(amount, line)?;
                        if !(0.0..=1.0).contains(&amount) {
                            return Err(line.error("send amount must be in 0..1"));
                        }
                        sends.insert(target, amount);
                    } else {
                        fx.push(line.clone());
                    }
                }
                program.buses.push(Bus {
                    name,
                    chain: chain(&fx, Some(Expr::Ref("input".into())))?,
                    sends,
                });
            }
            "section" => {
                let name = identifier(header, 1)?;
                let len = number(
                    header
                        .text
                        .split_whitespace()
                        .nth(2)
                        .ok_or_else(|| header.error("section needs a cycle length"))?,
                    header,
                )?;
                if !(0.001..=100_000.0).contains(&len) {
                    return Err(header.error("section length must be positive"));
                }
                let tail: Vec<_> = header.text.split_whitespace().skip(3).collect();
                let mut layers = Vec::new();
                for pair in tail.chunks(2) {
                    if pair.len() != 2 || pair[0] != "with" || !valid_name(pair[1]) {
                        return Err(header.error("expected `with SECTION` for each section layer"));
                    }
                    layers.push(pair[1].to_owned());
                }
                let mut plays = Vec::new();
                let mut n = 0;
                while n < body.len() {
                    let h = &body[n];
                    let e = block_end(body, n);
                    if !matches!(h.word(), "play" | "beat") {
                        return Err(h.error("sections contain play or beat blocks"));
                    }
                    plays.push(play(h, &body[n + 1..e])?);
                    n = e;
                }
                program.sections.push(Section {
                    name,
                    len,
                    layers,
                    plays,
                });
            }
            "js" | "sing" | "visual" | "mask" | "draw" | "out" | "zonedef" => {
                return Err(header.error(format!(
                    "`{}` is not supported by the native audio renderer",
                    header.word()
                )));
            }
            "cps" | "bpm" | "timesig" | "level" | "master" | "sidechain" | "stereo" | "macro"
            | "switch" | "patdef" | "scaledef" | "wavedef" | "curvedef" | "song" => {
                if !body.is_empty() {
                    return Err(body[0].error("this directive has no indented body"));
                }
                program.directives.push(header.clone());
            }
            _ => return Err(header.error(format!("unknown rondo directive `{}`", header.word()))),
        }
        at = end;
    }
    Ok(program)
}
fn block_end(lines: &[Line], at: usize) -> usize {
    let mut end = at + 1;
    while end < lines.len() && lines[end].indent > lines[at].indent {
        end += 1;
    }
    end
}
pub(crate) fn identifier(line: &Line, index: usize) -> Result<String> {
    let name = line
        .text
        .split_whitespace()
        .nth(index)
        .ok_or_else(|| line.error("expected a name"))?;
    if !valid_name(name) {
        return Err(line.error(format!("invalid name `{name}`")));
    }
    Ok(name.to_owned())
}
fn valid_name(name: &str) -> bool {
    let mut c = name.chars();
    c.next()
        .is_some_and(|x| x.is_ascii_alphabetic() || x == '_')
        && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
}
pub(crate) fn number(text: &str, line: &Line) -> Result<f64> {
    let n = text
        .parse::<f64>()
        .map_err(|_| line.error(format!("expected a number, got `{text}`")))?;
    if !n.is_finite() {
        return Err(line.error("number must be finite"));
    }
    Ok(n)
}
pub(crate) fn options(text: &str, line: &Line) -> Result<BTreeMap<String, f64>> {
    numeric_options(text, line, &[])
}
fn numeric_options(text: &str, line: &Line, flags: &[&str]) -> Result<BTreeMap<String, f64>> {
    let mut out = BTreeMap::new();
    let spaced = text.replace(':', " : ");
    let mut fields = spaced.split_whitespace();
    while let Some(key) = fields.next() {
        let value = if flags.contains(&key) {
            1.
        } else {
            if fields.next() != Some(":") {
                return Err(line.error("expected name:value"));
            }
            number(
                fields
                    .next()
                    .ok_or_else(|| line.error("option needs a value"))?,
                line,
            )?
        };
        if out.insert(key.to_owned(), value).is_some() {
            return Err(line.error(format!("duplicate option `{key}`")));
        }
    }
    Ok(out)
}
fn synth(header: &Line, body: &[Line]) -> Result<Synth> {
    let name = identifier(header, 1)?;
    let tail = header
        .text
        .split_whitespace()
        .skip(2)
        .collect::<Vec<_>>()
        .join(" ");
    let options = numeric_options(&tail, header, &["mono"])?;
    for key in options.keys() {
        if !matches!(
            key.as_str(),
            "mono"
                | "glide"
                | "unison"
                | "detune"
                | "spread"
                | "curve"
                | "blend"
                | "octaves"
                | "humanize"
                | "voices"
        ) {
            return Err(header.error(format!("unsupported voice option `{key}`")));
        }
    }
    let post_at = body.iter().position(|l| l.text == "post");
    let (voice, post) = if let Some(n) = post_at {
        if body[n + 1..].iter().any(|l| l.indent <= body[n].indent) {
            return Err(body[n].error("post must be last and its body indented"));
        }
        if body.len() == n + 1 {
            return Err(body[n].error("post needs an effect chain"));
        }
        (
            &body[..n],
            Some(chain(&body[n + 1..], Some(Expr::Ref("input".into())))?),
        )
    } else {
        (body, None)
    };
    let chain = chain(voice, None)?;
    if chain.output.is_none() {
        return Err(header.error("synth has no audio output"));
    }
    Ok(Synth {
        name,
        chain,
        post,
        options,
    })
}
fn play(header: &Line, body: &[Line]) -> Result<Play> {
    if body.is_empty() {
        return Err(header.error("play/beat needs notation"));
    }
    if header.word() == "play" {
        identifier(header, 1)?;
    }
    let mut joined: Vec<Line> = Vec::new();
    let mut depth = 0_i32;
    for line in body {
        if depth > 0 {
            let previous = joined.last_mut().expect("open notation line");
            previous.text.push(' ');
            previous.text.push_str(&line.text);
        } else {
            joined.push(line.clone());
        }
        for c in line.text.chars() {
            match c {
                '[' | '<' | '{' => depth += 1,
                ']' | '>' | '}' => depth -= 1,
                _ => {}
            }
        }
        if depth < 0 {
            return Err(line.error("unmatched notation group"));
        }
    }
    if depth != 0 {
        return Err(header.error("unclosed notation group"));
    }
    Ok(Play {
        header: header.clone(),
        body: joined,
    })
}

pub(crate) fn chain(lines: &[Line], initial: Option<Expr>) -> Result<Chain> {
    let mut result = Chain {
        output: initial,
        ..Default::default()
    };
    let mut at = 0;
    let indent = lines.first().map(|l| l.indent);
    while at < lines.len() {
        let line = &lines[at];
        if Some(line.indent) != indent {
            return Err(line.error("unexpected indentation in signal chain"));
        }
        if line.word() == "sum" {
            let index = identifier(line, 1)?;
            let range = line
                .text
                .split_whitespace()
                .nth(2)
                .ok_or_else(|| line.error("sum needs an integer range"))?;
            let (lo, hi) = range
                .split_once("..")
                .ok_or_else(|| line.error("sum range is LO..HI"))?;
            let lo = number(lo, line)?;
            let hi = number(hi, line)?;
            if lo.fract() != 0.0 || hi.fract() != 0.0 || (hi - lo).abs() > 63.0 {
                return Err(line.error("sum supports at most 64 integer steps"));
            }
            let end = block_end(lines, at);
            let sub = chain(&lines[at + 1..end], None)?;
            let body = sub
                .output
                .ok_or_else(|| line.error("sum needs an audio body"))?;
            let mut sum = Expr::Num(0.0);
            let step = if hi >= lo { 1.0 } else { -1.0 };
            for n in 0..=(hi - lo).abs() as usize {
                let mut bindings = sub.bindings.clone();
                bindings.insert(index.clone(), Expr::Num(lo + n as f64 * step));
                sum = Expr::bin('+', sum, substitute(&body, &bindings, &mut Vec::new())?);
            }
            if result.output.is_some() {
                return Err(
                    line.error("sum must be the source of a chain; use a binding to mix it")
                );
            }
            result.output = Some(sum);
            at = end;
            continue;
        }
        if let Some((name, text)) = line.text.split_once('=') {
            let name = name.trim();
            if !valid_name(name)
                || matches!(
                    name,
                    "note" | "gate" | "input" | "velocity" | "adsr" | "knob" | "switch" | "sum"
                )
            {
                return Err(line.error("invalid or reserved binding name"));
            }
            let expr = expression(text.trim(), line, None)?;
            if result.bindings.insert(name.to_owned(), expr).is_some() {
                return Err(line.error(format!("duplicate binding `{name}`")));
            }
        } else {
            let previous = result.output.take();
            result.output = Some(expression(&line.text, line, previous)?);
        }
        at += 1;
    }
    Ok(result)
}
fn substitute(
    expr: &Expr,
    bindings: &BTreeMap<String, Expr>,
    visiting: &mut Vec<String>,
) -> Result<Expr> {
    Ok(match expr {
        Expr::Ref(name) if bindings.contains_key(name) => {
            if visiting.contains(name) {
                return Err(Error::invalid("cyclic sum binding"));
            }
            visiting.push(name.clone());
            let e = substitute(&bindings[name], bindings, visiting)?;
            visiting.pop();
            e
        }
        Expr::Bin(op, a, b) => Expr::bin(
            *op,
            substitute(a, bindings, visiting)?,
            substitute(b, bindings, visiting)?,
        ),
        Expr::Call(name, args, named) => Expr::Call(
            name.clone(),
            args.iter()
                .map(|x| substitute(x, bindings, visiting))
                .collect::<Result<_>>()?,
            named
                .iter()
                .map(|(k, v)| Ok((k.clone(), substitute(v, bindings, visiting)?)))
                .collect::<Result<_>>()?,
        ),
        Expr::Map(x, a, b) => Expr::Map(
            Box::new(substitute(x, bindings, visiting)?),
            Box::new(substitute(a, bindings, visiting)?),
            Box::new(substitute(b, bindings, visiting)?),
        ),
        _ => expr.clone(),
    })
}

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Num(f64),
    Name(String),
    Op(char),
    Left,
    Right,
    Colon,
    Range,
    Arrow,
}
#[derive(Clone, Debug)]
struct Token {
    kind: Kind,
    space: bool,
    column: usize,
}
fn tokenize(text: &str, line: &Line) -> Result<Vec<Token>> {
    let mut tokens: Vec<Token> = Vec::new();
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut space = true;
    while at < bytes.len() {
        let c = bytes[at] as char;
        if c.is_ascii_whitespace() {
            at += 1;
            space = true;
            continue;
        }
        let column = at + line.indent + 1;
        let start = at;
        let kind = if text[at..].starts_with("..") {
            at += 2;
            Kind::Range
        } else if text[at..].starts_with("->") {
            at += 2;
            Kind::Arrow
        } else if c.is_ascii_digit()
            || c == '.'
            || (c == '-'
                && bytes
                    .get(at + 1)
                    .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
                && (space
                    || tokens.last().is_some_and(|t| {
                        matches!(t.kind, Kind::Colon | Kind::Range | Kind::Op(_))
                    })))
        {
            if c == '-' {
                at += 1;
            }
            while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                at += 1;
            }
            if bytes.get(at) == Some(&b'.') && !text[at..].starts_with("..") {
                at += 1;
                while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                    at += 1;
                }
            }
            Kind::Num(number(&text[start..at], line)?)
        } else if c.is_ascii_alphabetic() || c == '_' {
            at += 1;
            while bytes
                .get(at)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
            {
                at += 1;
            }
            Kind::Name(text[start..at].to_owned())
        } else {
            at += 1;
            match c {
                '+' | '-' | '*' | '/' | '^' => Kind::Op(c),
                '(' => Kind::Left,
                ')' => Kind::Right,
                ':' => Kind::Colon,
                _ => {
                    return Err(Error::at(
                        line.line,
                        column,
                        format!("unexpected character `{c}` in signal expression"),
                    ));
                }
            }
        };
        tokens.push(Token {
            kind,
            space,
            column,
        });
        space = false;
    }
    Ok(tokens)
}
pub(crate) fn expression(text: &str, line: &Line, pipe: Option<Expr>) -> Result<Expr> {
    let tokens = tokenize(text, line)?;
    let mut parser = ExpressionParser {
        tokens,
        at: 0,
        line,
        depth: 0,
    };
    let expr = if let Some(pipe) = pipe {
        match parser.peek() {
            Some(Kind::Op(op)) => {
                let op = *op;
                parser.at += 1;
                Expr::bin(op, pipe, parser.expr(1)?)
            }
            Some(Kind::Name(name)) if spec(name).is_some_and(|s| s.processor) => {
                let name = name.clone();
                parser.at += 1;
                parser.call(&name, vec![pipe])?
            }
            _ => {
                return Err(line
                    .error("a subsequent spine line needs a processor or an arithmetic operator"));
            }
        }
    } else {
        parser.expr(1)?
    };
    if parser.at != parser.tokens.len() {
        let token = &parser.tokens[parser.at];
        return Err(Error::at(
            line.line,
            token.column,
            "unexpected token or unsupported named argument",
        ));
    }
    Ok(expr)
}
#[derive(Clone, Copy)]
struct Spec {
    processor: bool,
    pos: &'static [bool],
    named: &'static [&'static str],
    enums: &'static [&'static str],
}
fn spec(name: &str) -> Option<Spec> {
    let (processor, pos, named, enums): (bool, &[bool], &[&str], &[&str]) = match name {
        "sine" | "saw" | "square" | "tri" => (false, &[false], &[], &[]),
        "pulse" | "syncsaw" => (false, &[false, false], &[], &[]),
        "fm" => (false, &[false, false], &["feedback", "wave"], &["wave"]),
        "supersaw" => (false, &[false], &["detune", "mix"], &[]),
        "wavetable" => (
            false,
            &[false, false],
            &["table", "warp", "warpamt"],
            &["table", "warp"],
        ),
        "lfsr" => (false, &[false], &["mode"], &["mode"]),
        "noise" => (false, &[true], &[], &[]),
        "lfo" => (false, &[false, true], &["sync"], &[]),
        "sample" => (
            false,
            &[true],
            &[
                "root", "speed", "loop", "variant", "start", "end", "reverse", "slices", "fade",
            ],
            &[],
        ),
        "pluck" => (false, &[false], &["decay", "damp", "seed"], &[]),
        "modal" => (
            false,
            &[false],
            &["model", "decay", "damp", "stretch", "keyScale"],
            &["model"],
        ),
        "tape" => (true, &[], &["wow", "flutter", "sat", "tone"], &[]),
        "transient" => (true, &[], &["attack", "sustain"], &[]),
        "exciter" => (true, &[], &["freq", "amount", "drive"], &[]),
        "deess" => (
            true,
            &[],
            &["freq", "threshold", "ratio", "attack", "release"],
            &[],
        ),
        "granular" => (
            false,
            &[true],
            &["pos", "root", "rate", "size", "density", "spray", "loop"],
            &[],
        ),
        "pitchshift" => (true, &[], &["semitones", "window", "mix"], &[]),
        "vocoder" => (
            true,
            &[false],
            &["bands", "low", "high", "q", "response"],
            &[],
        ),
        "ott" => (true, &[], &["depth", "low", "high", "makeup"], &[]),
        "looper" => (
            true,
            &[false],
            &["feedback", "mix", "clear", "maxtime", "name"],
            &["name"],
        ),
        "convolve" => (true, &[true], &["mix"], &[]),
        "env" => (false, &[], &["release", "curve", "loop"], &[]),
        "ladder" => (true, &[false], &["res"], &[]),
        "svf" => (true, &[false], &["res", "mode"], &["mode"]),
        "dualsvf" => (
            true,
            &[false, false],
            &["res", "mode", "a", "b"],
            &["mode", "a", "b"],
        ),
        "onepole" | "formant" | "pan" => (true, &[false], &[], &[]),
        "delay" => (true, &[false, false], &["maxtime", "sync", "mix"], &[]),
        "comb" => (true, &[false, false], &["damp"], &[]),
        "reverb" => (true, &[], &["room", "damp", "mix"], &[]),
        "shape" => (true, &[false], &["type"], &["type"]),
        "compress" => (
            true,
            &[],
            &[
                "threshold",
                "ratio",
                "attack",
                "release",
                "knee",
                "makeup",
                "key",
            ],
            &[],
        ),
        "limiter" => (true, &[], &["ceiling", "lookahead", "release"], &[]),
        "bitcrush" => (true, &[], &["bits", "downsample"], &[]),
        "chorus" => (true, &[], &["rate", "depth", "mix"], &[]),
        "flanger" => (true, &[], &["rate", "depth", "feedback", "mix"], &[]),
        "phaser" => (
            true,
            &[],
            &["rate", "depth", "feedback", "stages", "mix"],
            &[],
        ),
        "width" => (true, &[false], &["mode"], &["mode"]),
        "follow" => (true, &[], &["attack", "release", "mode"], &["mode"]),
        "noisegate" => (
            true,
            &[],
            &[
                "threshold",
                "range",
                "attack",
                "hold",
                "release",
                "hysteresis",
            ],
            &[],
        ),
        "eq" => (true, &[], &[], &[]),
        "tanh" | "fold" | "abs" | "floor" | "ceil" | "round" | "sign" | "sqrt" | "exp" | "log"
        | "sin" | "cos" => (true, &[], &[], &[]),
        "min" | "max" | "mod" => (true, &[false], &[], &[]),
        "clip" | "mix" => (true, &[false, false], &[], &[]),
        _ => return None,
    };
    Some(Spec {
        processor,
        pos,
        named,
        enums,
    })
}
struct ExpressionParser<'a> {
    tokens: Vec<Token>,
    at: usize,
    line: &'a Line,
    depth: usize,
}
impl ExpressionParser<'_> {
    fn peek(&self) -> Option<&Kind> {
        self.tokens.get(self.at).map(|t| &t.kind)
    }
    fn named(&self) -> Option<&str> {
        if let Some(Kind::Name(n)) = self.peek()
            && self
                .tokens
                .get(self.at + 1)
                .is_some_and(|t| t.kind == Kind::Colon)
        {
            return Some(n);
        }
        None
    }
    fn can_arg(&self) -> bool {
        self.tokens
            .get(self.at)
            .is_some_and(|t| t.space && matches!(t.kind, Kind::Num(_) | Kind::Name(_) | Kind::Left))
            && self.named().is_none()
    }
    fn expr(&mut self, min: u8) -> Result<Expr> {
        self.depth += 1;
        if self.depth > 64 {
            return Err(self.line.error("expression nesting limit exceeded"));
        }
        let mut left = self.primary()?;
        loop {
            match self.peek() {
                Some(Kind::Op(op)) => {
                    let op = *op;
                    let prec = match op {
                        '+' | '-' => 2,
                        '*' | '/' => 3,
                        '^' => 4,
                        _ => 0,
                    };
                    if prec < min {
                        break;
                    }
                    self.at += 1;
                    let right = self.expr(if op == '^' { prec } else { prec + 1 })?;
                    left = Expr::bin(op, left, right);
                }
                Some(Kind::Arrow) if min <= 1 => {
                    self.at += 1;
                    let lo = self.expr(3)?;
                    if self.peek() != Some(&Kind::Range) {
                        return Err(self.line.error("range map needs `..`"));
                    }
                    self.at += 1;
                    let hi = self.expr(3)?;
                    left = Expr::Map(Box::new(left), Box::new(lo), Box::new(hi));
                }
                _ => break,
            }
        }
        self.depth -= 1;
        Ok(left)
    }
    fn primary(&mut self) -> Result<Expr> {
        let token = self
            .tokens
            .get(self.at)
            .cloned()
            .ok_or_else(|| self.line.error("missing expression"))?;
        self.at += 1;
        match token.kind {
            Kind::Num(n) => Ok(Expr::Num(n)),
            Kind::Left => {
                let e = self.expr(0)?;
                if self.peek() != Some(&Kind::Right) {
                    return Err(self.line.error("unclosed parenthesis"));
                }
                self.at += 1;
                Ok(e)
            }
            Kind::Op('-') => Ok(Expr::bin('*', Expr::Num(-1.0), self.expr(5)?)),
            Kind::Name(name) => match name.as_str() {
                "js" | "mic" | "sing" | "ddsp" => Err(self
                    .line
                    .error(format!("`{name}` is not implemented in this native port"))),
                "adsr" => {
                    let mut args = Vec::new();
                    for _ in 0..4 {
                        args.push(self.expr(5)?);
                    }
                    Ok(Expr::Call(name, args, BTreeMap::new()))
                }
                "knob" => {
                    let def = self.literal()?;
                    let lo = self.literal()?;
                    if self.peek() != Some(&Kind::Range) {
                        return Err(self.line.error("knob needs LO..HI"));
                    }
                    self.at += 1;
                    let hi = self.literal()?;
                    if lo >= hi || def < lo || def > hi {
                        return Err(self.line.error("invalid knob range or default"));
                    }
                    if let Some(Kind::Name(curve)) = self.peek() {
                        if !matches!(curve.as_str(), "log" | "linear") {
                            return Err(self.line.error("knob curve must be log or linear"));
                        }
                        self.at += 1;
                    }
                    Ok(Expr::Knob(def, lo, hi))
                }
                "switch" => {
                    let a = self.literal()?;
                    let b = self.literal()?;
                    if a == b {
                        return Err(self.line.error("switch needs two different values"));
                    }
                    Ok(Expr::Knob(a, a.min(b), a.max(b)))
                }
                "env" => {
                    let mut args = Vec::new();
                    while self.can_arg() {
                        let e = self.expr(5)?;
                        if args.len() % 2 == 1 && self.peek() == Some(&Kind::Colon) {
                            self.at += 1;
                            args.push(Expr::Curved(Box::new(e), Box::new(self.expr(5)?)));
                        } else {
                            args.push(e);
                        }
                    }
                    if args.is_empty() && self.named().is_none() {
                        return Ok(Expr::Ref(name));
                    }
                    if args.is_empty() || args.len() % 2 != 0 {
                        return Err(self.line.error("env needs time/level pairs"));
                    }
                    let named = self.named_args(&spec("env").unwrap())?;
                    Ok(Expr::Call(name, args, named))
                }
                _ if spec(&name).is_some() => {
                    let spec = spec(&name).unwrap();
                    let mut args = Vec::new();
                    if spec.processor {
                        if !self.can_arg() {
                            return Ok(Expr::Ref(name));
                        }
                        args.push(self.expr(2)?);
                    }
                    self.call(&name, args)
                }
                _ => Ok(Expr::Ref(name)),
            },
            _ => Err(Error::at(
                self.line.line,
                token.column,
                "expected an expression",
            )),
        }
    }
    fn literal(&mut self) -> Result<f64> {
        if let Some(Kind::Num(n)) = self.peek() {
            let n = *n;
            self.at += 1;
            Ok(n)
        } else {
            Err(self.line.error("expected a number"))
        }
    }
    fn call(&mut self, name: &str, mut args: Vec<Expr>) -> Result<Expr> {
        let spec = spec(name).unwrap();
        if name == "eq" {
            while self.can_arg() {
                if let Some(Kind::Name(word)) = self.peek() {
                    if !matches!(
                        word.as_str(),
                        "hp" | "lp" | "peak" | "lowshelf" | "highshelf"
                    ) {
                        return Err(self.line.error("unknown EQ band"));
                    }
                    args.push(Expr::Word(word.clone()));
                    self.at += 1;
                } else {
                    args.push(self.expr(5)?);
                }
            }
        } else {
            for is_enum in spec.pos {
                if !self.can_arg() {
                    break;
                }
                if *is_enum {
                    if let Some(Kind::Name(word)) = self.peek() {
                        args.push(Expr::Word(word.clone()));
                        self.at += 1;
                    } else {
                        return Err(self.line.error("expected an enum word"));
                    }
                } else {
                    args.push(self.expr(2)?);
                }
            }
        }
        Ok(Expr::Call(name.into(), args, self.named_args(&spec)?))
    }
    fn named_args(&mut self, spec: &Spec) -> Result<BTreeMap<String, Expr>> {
        let mut out = BTreeMap::new();
        while let Some(key) = self.named() {
            if !spec.named.contains(&key) {
                break;
            }
            let key = key.to_owned();
            self.at += 2;
            let value = if spec.enums.contains(&key.as_str()) {
                if let Some(Kind::Name(n)) = self.peek() {
                    let x = Expr::Word(n.clone());
                    self.at += 1;
                    x
                } else {
                    return Err(self.line.error("expected an enum word"));
                }
            } else {
                self.expr(2)?
            };
            if out.insert(key.clone(), value).is_some() {
                return Err(self.line.error(format!("duplicate named argument `{key}`")));
            }
        }
        Ok(out)
    }
}
