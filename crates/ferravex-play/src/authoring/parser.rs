use super::*;
#[derive(Debug)]
struct Token {
    text: String,
    span: SourceSpan,
    symbol: Option<char>,
}
struct Lexer<'a> {
    source: &'a str,
    at: usize,
    line: usize,
    column: usize,
    tokens: usize,
}
impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.source[self.at..].chars().next()
    }
    fn take(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }
    fn next(&mut self) -> Result<Option<Token>, ContentError> {
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.take();
            }
            if self.peek() == Some('#') {
                while self.peek().is_some_and(|c| c != '\n') {
                    self.take();
                }
            } else {
                break;
            }
        }
        let span = SourceSpan {
            line: self.line,
            column: self.column,
            offset: self.at,
        };
        let Some(c) = self.take() else {
            return Ok(None);
        };
        self.tokens += 1;
        if self.tokens > 524288 {
            return Err(ContentError::new(ErrorKind::Limit, span, "too many tokens"));
        }
        if "{};".contains(c) {
            return Ok(Some(Token {
                text: c.to_string(),
                span,
                symbol: Some(c),
            }));
        }
        let mut text = String::new();
        if c == '"' {
            loop {
                let c = self.take().ok_or_else(|| {
                    ContentError::new(ErrorKind::Syntax, span, "unterminated string")
                })?;
                if c == '"' {
                    break;
                }
                let c = if c == '\\' {
                    match self.take() {
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        _ => {
                            return Err(ContentError::new(
                                ErrorKind::Syntax,
                                span,
                                "invalid escape",
                            ));
                        }
                    }
                } else {
                    if c.is_control() {
                        return Err(ContentError::new(
                            ErrorKind::Syntax,
                            span,
                            "unescaped control in string",
                        ));
                    }
                    c
                };
                text.push(c);
                if text.len() > MAX_STRING {
                    return Err(ContentError::new(ErrorKind::Limit, span, "string too long"));
                }
            }
        } else {
            if c.is_control() {
                return Err(ContentError::new(
                    ErrorKind::Syntax,
                    span,
                    "control character",
                ));
            }
            text.push(c);
            while self
                .peek()
                .is_some_and(|v| !v.is_whitespace() && !"{};#\"".contains(v))
            {
                text.push(self.take().unwrap());
                if text.len() > MAX_STRING {
                    return Err(ContentError::new(ErrorKind::Limit, span, "token too long"));
                }
            }
        }
        Ok(Some(Token {
            text,
            span,
            symbol: None,
        }))
    }
}
struct Parser<'a> {
    lexer: Lexer<'a>,
    peek: Option<Token>,
    nodes: usize,
    values: usize,
}
impl Parser<'_> {
    fn span(&self) -> SourceSpan {
        self.peek.as_ref().map_or(
            SourceSpan {
                line: self.lexer.line,
                column: self.lexer.column,
                offset: self.lexer.at,
            },
            |t| t.span,
        )
    }
    fn fail(&self, context: impl Into<String>) -> ContentError {
        ContentError::new(ErrorKind::Syntax, self.span(), context)
    }
    fn bump(&mut self) -> Result<Token, ContentError> {
        let token = self
            .peek
            .take()
            .ok_or_else(|| self.fail("unexpected end"))?;
        self.peek = self.lexer.next()?;
        Ok(token)
    }
    fn word(&mut self) -> Result<Token, ContentError> {
        if self.peek.as_ref().is_none_or(|t| t.symbol.is_some()) {
            return Err(self.fail("expected word or quoted string"));
        }
        self.bump()
    }
    fn literal(&mut self, expected: &str) -> Result<(), ContentError> {
        if self.peek.as_ref().is_none_or(|t| t.text != expected) {
            return Err(self.fail(format!("expected {expected}")));
        }
        self.bump()?;
        Ok(())
    }
    fn symbol(&mut self, c: char) -> Result<(), ContentError> {
        if self.peek.as_ref().and_then(|t| t.symbol) != Some(c) {
            return Err(self.fail(format!("expected {c}")));
        }
        self.bump()?;
        Ok(())
    }
    fn end(&mut self) -> Result<(), ContentError> {
        self.symbol('}')?;
        if self.peek.as_ref().and_then(|t| t.symbol) == Some(';') {
            self.bump()?;
        }
        Ok(())
    }
    fn identifier(&mut self) -> Result<Token, ContentError> {
        let t = self.word()?;
        if !identifier(&t.text) {
            return Err(ContentError::new(
                ErrorKind::Syntax,
                t.span,
                "identifier must be 1..64 ASCII letters, digits, _ or -",
            ));
        }
        Ok(t)
    }
    fn float(&mut self) -> Result<f64, ContentError> {
        let token = self.word()?;
        token
            .text
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| {
                ContentError::new(ErrorKind::Range, token.span, "expected finite number")
            })
    }
    fn floats<const N: usize>(&mut self) -> Result<[f32; N], ContentError> {
        let mut a = [0.; N];
        for v in &mut a {
            let span = self.span();
            *v = self.float()? as f32;
            if !v.is_finite() {
                return Err(ContentError::new(
                    ErrorKind::Range,
                    span,
                    "number exceeds f32",
                ));
            }
        }
        Ok(a)
    }
    fn transform(&mut self) -> Result<Transform, ContentError> {
        let span = self.span();
        let translation = self.floats()?;
        let rotation = self.floats()?;
        let scale = self.floats()?;
        let t = Transform {
            translation,
            rotation,
            scale,
        };
        if !t.valid() {
            return Err(ContentError::new(
                ErrorKind::InvalidTransform,
                span,
                "invalid transform/quaternion",
            ));
        }
        self.symbol(';')?;
        Ok(t)
    }
    fn component(&mut self) -> Result<(String, Component), ContentError> {
        let name = self.identifier()?;
        self.symbol('{')?;
        let mut fields = BTreeMap::new();
        while self.peek.as_ref().is_some_and(|t| t.symbol != Some('}')) {
            let key = self.identifier()?;
            let kind = self.word()?;
            let value = match kind.text.as_str() {
                "bool" => {
                    let token = self.word()?;
                    Value::Bool(match token.text.as_str() {
                        "true" => true,
                        "false" => false,
                        _ => {
                            return Err(ContentError::new(
                                ErrorKind::Type,
                                token.span,
                                "expected true or false",
                            ));
                        }
                    })
                }
                "int" => {
                    let token = self.word()?;
                    Value::Integer(token.text.parse().map_err(|_| {
                        ContentError::new(ErrorKind::Range, token.span, "expected i64")
                    })?)
                }
                "number" => Value::Number(self.float()?),
                "text" => Value::Text(self.word()?.text),
                "vec2" => Value::Vec2(self.floats()?),
                "vec3" => Value::Vec3(self.floats()?),
                "quat" => Value::Quaternion(self.floats()?),
                "rgba" => {
                    let mut values = [0; 4];
                    for v in &mut values {
                        let t = self.word()?;
                        *v = t.text.parse::<u8>().map_err(|_| {
                            ContentError::new(
                                ErrorKind::Range,
                                t.span,
                                "expected byte color 0..255",
                            )
                        })?;
                    }
                    Value::Color(values)
                }
                "ref" => Value::Reference(self.word()?.text),
                _ => {
                    return Err(ContentError::new(
                        ErrorKind::Type,
                        kind.span,
                        "unknown value kind",
                    ));
                }
            };
            self.symbol(';')?;
            self.values += 1;
            if self.values > MAX_VALUES || fields.len() >= MAX_FIELDS {
                return Err(ContentError::new(
                    ErrorKind::Limit,
                    key.span,
                    "too many fields",
                ));
            }
            if fields
                .insert(
                    key.text,
                    Property {
                        value,
                        span: key.span,
                    },
                )
                .is_some()
            {
                return Err(ContentError::new(
                    ErrorKind::Duplicate,
                    key.span,
                    "duplicate property",
                ));
            }
        }
        self.end()?;
        Ok((
            name.text,
            Component {
                fields,
                span: name.span,
            },
        ))
    }
    fn override_node(&mut self) -> Result<NodeOverride, ContentError> {
        self.symbol('{')?;
        let mut result = NodeOverride::default();
        while self.peek.as_ref().is_some_and(|t| t.symbol != Some('}')) {
            let command = self.word()?;
            match command.text.as_str() {
                "transform" => {
                    if result.local.is_some() {
                        return Err(ContentError::new(
                            ErrorKind::Duplicate,
                            command.span,
                            "transform",
                        ));
                    }
                    result.local = Some(self.transform()?);
                }
                "component" => {
                    let (kind, c) = self.component()?;
                    if result.components.len() >= MAX_COMPONENTS {
                        return Err(ContentError::new(
                            ErrorKind::Limit,
                            command.span,
                            "too many components",
                        ));
                    }
                    if result.components.insert(kind, c).is_some() {
                        return Err(ContentError::new(
                            ErrorKind::Duplicate,
                            command.span,
                            "component",
                        ));
                    }
                }
                _ => {
                    return Err(ContentError::new(
                        ErrorKind::Syntax,
                        command.span,
                        "only transform/component allowed in override",
                    ));
                }
            }
        }
        self.end()?;
        Ok(result)
    }
    fn node(&mut self) -> Result<NodeSpec, ContentError> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(ContentError::new(
                ErrorKind::Limit,
                self.span(),
                "too many authored nodes",
            ));
        }
        let id = self.identifier()?;
        self.symbol('{')?;
        let mut n = NodeSpec {
            id: id.text,
            span: id.span,
            parent: None,
            prefab: None,
            local: None,
            components: BTreeMap::new(),
            overrides: BTreeMap::new(),
        };
        while self.peek.as_ref().is_some_and(|t| t.symbol != Some('}')) {
            let command = self.word()?;
            match command.text.as_str() {
                "parent" => {
                    if n.parent.is_some() {
                        return Err(ContentError::new(
                            ErrorKind::Duplicate,
                            command.span,
                            "parent",
                        ));
                    }
                    n.parent = Some(self.word()?.text);
                    self.symbol(';')?;
                }
                "use" => {
                    if n.prefab.is_some() {
                        return Err(ContentError::new(
                            ErrorKind::Duplicate,
                            command.span,
                            "prefab use",
                        ));
                    }
                    n.prefab = Some(self.identifier()?.text);
                    self.symbol(';')?;
                }
                "transform" => {
                    if n.local.is_some() {
                        return Err(ContentError::new(
                            ErrorKind::Duplicate,
                            command.span,
                            "transform",
                        ));
                    }
                    n.local = Some(self.transform()?);
                }
                "component" => {
                    let (kind, c) = self.component()?;
                    if n.components.len() >= MAX_COMPONENTS {
                        return Err(ContentError::new(
                            ErrorKind::Limit,
                            command.span,
                            "too many components",
                        ));
                    }
                    if n.components.insert(kind, c).is_some() {
                        return Err(ContentError::new(
                            ErrorKind::Duplicate,
                            command.span,
                            "component",
                        ));
                    }
                }
                "override" => {
                    let target = self.word()?;
                    if !path(&target.text) || n.overrides.len() >= MAX_NODES {
                        return Err(ContentError::new(
                            ErrorKind::Limit,
                            target.span,
                            "invalid override path",
                        ));
                    }
                    let value = self.override_node()?;
                    if n.overrides.insert(target.text, value).is_some() {
                        return Err(ContentError::new(
                            ErrorKind::Duplicate,
                            target.span,
                            "override",
                        ));
                    }
                }
                _ => {
                    return Err(ContentError::new(
                        ErrorKind::Syntax,
                        command.span,
                        "unknown node command",
                    ));
                }
            }
        }
        self.end()?;
        Ok(n)
    }
    fn scope(&mut self) -> Result<Vec<NodeSpec>, ContentError> {
        let mut result = Vec::new();
        let mut names = std::collections::BTreeSet::new();
        while self.peek.as_ref().is_some_and(|t| t.symbol != Some('}')) {
            self.literal("node")?;
            let n = self.node()?;
            if !names.insert(n.id.clone()) {
                return Err(ContentError::new(ErrorKind::Duplicate, n.span, &n.id));
            }
            result.push(n);
        }
        Ok(result)
    }
}
pub(super) fn parse(source: &str) -> Result<Document, ContentError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(ContentError::new(
            ErrorKind::Limit,
            SourceSpan::default(),
            "source exceeds 2 MiB",
        ));
    }
    let mut lexer = Lexer {
        source,
        at: 0,
        line: 1,
        column: 1,
        tokens: 0,
    };
    let peek = lexer.next()?;
    let mut p = Parser {
        lexer,
        peek,
        nodes: 0,
        values: 0,
    };
    p.literal("ferravex_scene")?;
    let version = p.word()?;
    if version.text != "1" {
        return Err(ContentError::new(
            ErrorKind::Version,
            version.span,
            "expected authoring version 1",
        ));
    }
    p.symbol(';')?;
    let mut nodes = Vec::new();
    let mut prefabs = BTreeMap::new();
    let mut names = std::collections::BTreeSet::new();
    while p.peek.is_some() {
        let command = p.word()?;
        match command.text.as_str() {
            "node" => {
                let n = p.node()?;
                if !names.insert(n.id.clone()) {
                    return Err(ContentError::new(ErrorKind::Duplicate, n.span, &n.id));
                }
                nodes.push(n);
            }
            "prefab" => {
                let name = p.identifier()?;
                if prefabs.len() >= MAX_PREFABS {
                    return Err(ContentError::new(
                        ErrorKind::Limit,
                        name.span,
                        "too many prefabs",
                    ));
                }
                p.symbol('{')?;
                let value = p.scope()?;
                p.end()?;
                if prefabs.insert(name.text, value).is_some() {
                    return Err(ContentError::new(ErrorKind::Duplicate, name.span, "prefab"));
                }
            }
            _ => {
                return Err(ContentError::new(
                    ErrorKind::Syntax,
                    command.span,
                    "expected node or prefab",
                ));
            }
        }
    }
    Ok(Document { nodes, prefabs })
}
