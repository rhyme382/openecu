//! Partial, read-only A2L importer.
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};
use thiserror::Error;
const MAX_NESTING: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Unsupported,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub line: usize,
    pub column: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawItem {
    pub keyword: String,
    pub values: Vec<String>,
    pub value_locations: Vec<SourceLocation>,
    pub line: usize,
    pub column: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RawElement {
    Item(RawItem),
    Block(RawBlockId),
}
pub type RawBlockId = usize;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawBlock {
    pub keyword: String,
    pub arguments: Vec<String>,
    pub argument_locations: Vec<SourceLocation>,
    pub elements: Vec<RawElement>,
    pub line: usize,
    pub column: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2lDocument {
    pub project: Project,
    pub diagnostics: Vec<Diagnostic>,
    /// Root-level items and blocks in source order. Blocks refer to
    /// `raw_blocks`, so their contents remain stored exactly once.
    pub root_elements: Vec<RawElement>,
    /// Canonical storage for every preserved raw block. `RawBlockId` values
    /// in the model and in `RawElement::Block` refer into this vector.
    pub raw_blocks: Vec<RawBlock>,
    pub unknown_blocks: Vec<RawBlockId>,
}
impl A2lDocument {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|x| x.severity == Severity::Error)
    }
    pub fn raw_block(&self, id: RawBlockId) -> Option<&RawBlock> {
        self.raw_blocks.get(id)
    }
    pub fn unknown_block(&self, id: RawBlockId) -> Option<&RawBlock> {
        self.raw_block(id)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub modules: Vec<Module>,
    pub extra: Vec<RawItem>,
    pub raw_blocks: Vec<RawBlockId>,
    pub raw_block: RawBlockId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    pub description: Option<String>,
    pub mod_common: Option<ModCommon>,
    pub characteristics: Vec<Characteristic>,
    pub axis_points: Vec<AxisPoints>,
    pub compu_methods: Vec<CompuMethod>,
    pub record_layouts: Vec<RecordLayout>,
    pub measurements: Vec<Measurement>,
    pub extra: Vec<RawItem>,
    pub raw_blocks: Vec<RawBlockId>,
    pub raw_block: RawBlockId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModCommon {
    pub description: Option<String>,
    pub byte_order: Option<String>,
    pub extra: Vec<RawItem>,
    pub raw_blocks: Vec<RawBlockId>,
    pub raw_block: RawBlockId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Characteristic {
    pub name: String,
    pub description: Option<String>,
    pub address: Option<u64>,
    pub characteristic_type: Option<String>,
    pub layout_reference: Option<String>,
    pub conversion_reference: Option<String>,
    pub dimensions: Vec<u32>,
    pub axis_references: Vec<String>,
    pub axis_reference_sources: Vec<SourceLocation>,
    /// Typed projection of every `AXIS_DESCR` child, in source order (T002-C1).
    /// Additive: `dimensions` and `axis_references` keep their T002-B meaning.
    #[serde(default)]
    pub axis_descriptions: Vec<AxisDescription>,
    pub extra: Vec<RawItem>,
    pub raw_blocks: Vec<RawBlockId>,
    pub raw_block: RawBlockId,
    pub source: SourceLocation,
    pub layout_reference_source: Option<SourceLocation>,
    pub conversion_reference_source: Option<SourceLocation>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxisPoints {
    pub name: String,
    pub description: Option<String>,
    pub address: Option<u64>,
    pub layout_reference: Option<String>,
    pub conversion_reference: Option<String>,
    pub max_axis_points: Option<u32>,
    /// ASAP2 `InputQuantity` (positional argument 4), verbatim.
    #[serde(default)]
    pub input_quantity: Option<String>,
    #[serde(default)]
    pub lower_limit: Option<A2lNumber>,
    #[serde(default)]
    pub upper_limit: Option<A2lNumber>,
    /// Value of the optional `PHYS_UNIT` keyword.
    #[serde(default)]
    pub phys_unit: Option<String>,
    pub extra: Vec<RawItem>,
    pub raw_blocks: Vec<RawBlockId>,
    pub raw_block: RawBlockId,
    pub source: SourceLocation,
    pub layout_reference_source: Option<SourceLocation>,
    pub conversion_reference_source: Option<SourceLocation>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompuMethod {
    pub name: String,
    pub description: Option<String>,
    pub conversion_type: Option<String>,
    pub format: Option<String>,
    pub unit: Option<String>,
    pub extra: Vec<RawItem>,
    pub raw_blocks: Vec<RawBlockId>,
    pub raw_block: RawBlockId,
    pub source: SourceLocation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLayout {
    pub name: String,
    /// Typed projection of the recognized data elements, in source order
    /// (T002-C1). Unknown keywords are not listed here; they stay in `extra`.
    #[serde(default)]
    pub elements: Vec<RecordLayoutElement>,
    pub extra: Vec<RawItem>,
    pub raw_blocks: Vec<RawBlockId>,
    pub raw_block: RawBlockId,
    pub source: SourceLocation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Measurement {
    pub name: String,
    pub description: Option<String>,
    pub data_type: Option<String>,
    pub conversion_reference: Option<String>,
    pub unit: Option<String>,
    pub extra: Vec<RawItem>,
    pub raw_blocks: Vec<RawBlockId>,
    pub raw_block: RawBlockId,
    pub source: SourceLocation,
    pub conversion_reference_source: Option<SourceLocation>,
}
/// A finite number from the A2L source, keeping its original spelling.
/// Only finite values can be constructed from source text.
///
/// Deserialization re-validates the invariant: `text` must be a finite number
/// and `value` must be exactly what `text` parses to. This keeps the derived
/// `PartialEq` reflexive, so `Eq` is sound (no NaN can ever be stored).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "A2lNumberRepr")]
pub struct A2lNumber {
    value: f64,
    text: String,
}
#[derive(Deserialize)]
struct A2lNumberRepr {
    value: f64,
    text: String,
}
impl TryFrom<A2lNumberRepr> for A2lNumber {
    type Error = String;
    fn try_from(repr: A2lNumberRepr) -> Result<Self, Self::Error> {
        let parsed = Self::parse(&repr.text)
            .ok_or_else(|| format!("`{}` is not a finite A2L number", repr.text))?;
        if parsed.value.to_bits() != repr.value.to_bits() {
            return Err(format!(
                "value {} does not match its text `{}`",
                repr.value, repr.text
            ));
        }
        Ok(parsed)
    }
}
impl Eq for A2lNumber {}
impl A2lNumber {
    /// Returns `None` for text that is not a finite decimal number.
    pub fn parse(text: &str) -> Option<Self> {
        text.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map(|value| Self {
                value,
                text: text.to_owned(),
            })
    }
    pub fn value(&self) -> f64 {
        self.value
    }
    pub fn text(&self) -> &str {
        &self.text
    }
}
/// ASAP2 `AXIS_DESCR` attribute (first positional argument).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AxisAttribute {
    Std,
    Fix,
    Com,
    Res,
    Curve,
    /// Text not in the recognized set; preserved verbatim, never guessed.
    Unknown(String),
}
impl AxisAttribute {
    fn from_text(text: &str) -> Self {
        match text {
            "STD_AXIS" => Self::Std,
            "FIX_AXIS" => Self::Fix,
            "COM_AXIS" => Self::Com,
            "RES_AXIS" => Self::Res,
            "CURVE_AXIS" => Self::Curve,
            other => Self::Unknown(other.to_owned()),
        }
    }
}
/// `FIX_AXIS_PAR Offset Shift Numberapo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixAxisPar {
    pub offset: i64,
    pub shift: i64,
    pub number_of_points: u32,
}
/// `FIX_AXIS_PAR_DIST Offset Distance Numberapo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixAxisParDist {
    pub offset: i64,
    pub distance: i64,
    pub number_of_points: u32,
}
/// Typed view of one `AXIS_DESCR`. Every field that the source does not
/// declare is `None` / empty; nothing is inferred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxisDescription {
    pub attribute: Option<AxisAttribute>,
    pub input_quantity: Option<String>,
    pub conversion_reference: Option<String>,
    pub max_axis_points: Option<u32>,
    pub lower_limit: Option<A2lNumber>,
    pub upper_limit: Option<A2lNumber>,
    pub axis_pts_ref: Option<String>,
    pub curve_axis_ref: Option<String>,
    pub byte_order: Option<String>,
    pub deposit: Option<String>,
    pub format: Option<String>,
    pub phys_unit: Option<String>,
    pub monotony: Option<String>,
    pub read_only: bool,
    pub fix_axis_par: Option<FixAxisPar>,
    pub fix_axis_par_dist: Option<FixAxisParDist>,
    pub fix_axis_par_list: Option<Vec<A2lNumber>>,
    /// Positional arguments exactly as written, with their locations.
    pub positional_arguments: Vec<String>,
    pub argument_locations: Vec<SourceLocation>,
    pub conversion_reference_source: Option<SourceLocation>,
    pub max_axis_points_source: Option<SourceLocation>,
    pub axis_pts_ref_source: Option<SourceLocation>,
    pub curve_axis_ref_source: Option<SourceLocation>,
    pub extra: Vec<RawItem>,
    pub raw_blocks: Vec<RawBlockId>,
    pub raw_block: RawBlockId,
    pub source: SourceLocation,
}
/// One recognized data element of a `RECORD_LAYOUT`.
///
/// `position` is the ASAP2 position argument as written (no ordering or
/// uniqueness rule is enforced). `data_type` is the verbatim second argument;
/// for `RESERVED` it holds the data-size word. `index_mode` is only set for
/// `FNC_VALUES`, `index_increment` only for `AXIS_PTS_*` / `AXIS_RESCALE_*`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLayoutElement {
    pub keyword: String,
    pub position: Option<u32>,
    pub data_type: Option<String>,
    pub index_mode: Option<String>,
    pub index_increment: Option<String>,
    pub address_type: Option<String>,
    pub max_rescale_pairs: Option<u32>,
    pub fixed_axis_points: Option<u32>,
    pub arguments: Vec<String>,
    pub argument_locations: Vec<SourceLocation>,
    pub source: SourceLocation,
}
#[derive(Debug, Error)]
pub enum A2lImportError {
    #[error("cannot read A2L file: {0}")]
    Io(#[from] std::io::Error),
    #[error("A2L input is not UTF-8")]
    InvalidUtf8,
    #[error("A2L syntax error at {line}:{column}: {message}")]
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },
}
pub fn parse_a2l_file(path: impl AsRef<Path>) -> Result<A2lDocument, A2lImportError> {
    let bytes = fs::read(path)?;
    parse_a2l(std::str::from_utf8(&bytes).map_err(|_| A2lImportError::InvalidUtf8)?)
}
pub fn parse_a2l(source: &str) -> Result<A2lDocument, A2lImportError> {
    build_document(Parser::new(Lexer::new(source).lex()?).parse()?)
}

#[derive(Debug, Clone)]
struct Token {
    text: String,
    line: usize,
    column: usize,
}
impl Token {
    fn loc(&self) -> SourceLocation {
        SourceLocation {
            line: self.line,
            column: self.column,
        }
    }
}
struct Lexer<'a> {
    input: &'a str,
    offset: usize,
    line: usize,
    column: usize,
}
impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            offset: 0,
            line: 1,
            column: 1,
        }
    }
    fn peek(&self) -> Option<char> {
        self.input[self.offset..].chars().next()
    }
    fn take(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.offset += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.column = 1
        } else {
            self.column += 1
        };
        Some(c)
    }
    fn starts(&self, s: &str) -> bool {
        self.input[self.offset..].starts_with(s)
    }
    fn skip(&mut self) -> Result<(), A2lImportError> {
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.take();
            }
            if !self.starts("/*") {
                return Ok(());
            }
            let (line, column) = (self.line, self.column);
            self.take();
            self.take();
            while self.offset < self.input.len() && !self.starts("*/") {
                self.take();
            }
            if self.offset == self.input.len() {
                return Err(A2lImportError::Syntax {
                    line,
                    column,
                    message: "unterminated comment".into(),
                });
            }
            self.take();
            self.take();
        }
    }
    fn quoted(&mut self) -> Result<String, A2lImportError> {
        let (line, column) = (self.line, self.column);
        self.take();
        let mut s = String::new();
        while let Some(c) = self.take() {
            match c {
                '"' => return Ok(s),
                '\\' => s.push(self.take().ok_or_else(|| A2lImportError::Syntax {
                    line,
                    column,
                    message: "unterminated string".into(),
                })?),
                _ => s.push(c),
            }
        }
        Err(A2lImportError::Syntax {
            line,
            column,
            message: "unterminated string".into(),
        })
    }
    fn word(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_whitespace() || self.starts("/*") {
                break;
            }
            s.push(self.take().expect("peek succeeded"));
        }
        s
    }
    fn lex(mut self) -> Result<Lexed, A2lImportError> {
        let mut out = vec![];
        while self.offset < self.input.len() {
            self.skip()?;
            if self.offset == self.input.len() {
                break;
            }
            let (line, column) = (self.line, self.column);
            let text = if self.peek() == Some('"') {
                self.quoted()?
            } else {
                self.word()
            };
            if !text.is_empty() {
                out.push(Token { text, line, column });
            }
        }
        Ok(Lexed {
            tokens: out,
            eof: SourceLocation {
                line: self.line,
                column: self.column,
            },
        })
    }
}
struct Lexed {
    tokens: Vec<Token>,
    eof: SourceLocation,
}
#[derive(Debug, Clone)]
struct Block {
    keyword: Token,
    args: Vec<Token>,
    items: Vec<RawItem>,
    elements: Vec<Element>,
    raw_block: Option<RawBlockId>,
}
#[derive(Debug, Clone)]
enum Element {
    Item(RawItem),
    Block(Block),
}
#[derive(Debug, Clone)]
enum RootElement {
    Item(RawItem),
    Block(Block),
}
struct Parsed {
    roots: Vec<RootElement>,
}
struct Open {
    keyword: Token,
    args: Vec<Token>,
    items: Vec<RawItem>,
    elements: Vec<Element>,
}
impl Open {
    fn done(self) -> Block {
        Block {
            keyword: self.keyword,
            args: self.args,
            items: self.items,
            elements: self.elements,
            raw_block: None,
        }
    }
}
struct Parser {
    tokens: Vec<Token>,
    position: usize,
    eof: SourceLocation,
}
impl Parser {
    fn new(lexed: Lexed) -> Self {
        Self {
            tokens: lexed.tokens,
            position: 0,
            eof: lexed.eof,
        }
    }
    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }
    fn next(&mut self, msg: &str) -> Result<Token, A2lImportError> {
        let v = self
            .current()
            .cloned()
            .ok_or_else(|| A2lImportError::Syntax {
                line: self.eof.line,
                column: self.eof.column,
                message: msg.into(),
            })?;
        self.position += 1;
        Ok(v)
    }
    fn err(t: &Token, message: impl Into<String>) -> A2lImportError {
        A2lImportError::Syntax {
            line: t.line,
            column: t.column,
            message: message.into(),
        }
    }
    fn parse(mut self) -> Result<Parsed, A2lImportError> {
        let mut roots = vec![];
        let mut stack: Vec<Open> = vec![];
        while let Some(t) = self.current().cloned() {
            match t.text.as_str() {
                "/begin" => {
                    if stack.len() >= MAX_NESTING {
                        return Err(Self::err(&t, "nesting limit exceeded"));
                    }
                    self.position += 1;
                    let keyword = self.next("expected block name after /begin")?;
                    let mut args = vec![];
                    let count = arity(&keyword.text);
                    if count == 0 {
                        while self.current().is_some_and(|x| {
                            x.line == keyword.line && x.text != "/begin" && x.text != "/end"
                        }) {
                            args.push(self.next("expected positional argument")?);
                        }
                    } else {
                        for index in 0..count {
                            let previous_line =
                                args.last().map_or(keyword.line, |a: &Token| a.line);
                            if self.current().is_some_and(|x| {
                                x.text != "/begin"
                                    && x.text != "/end"
                                    && !is_internal_keyword(&keyword.text, &x.text)
                                    && !is_foreign_in_numeric_slot(
                                        &keyword.text,
                                        index,
                                        x,
                                        previous_line,
                                    )
                            }) {
                                args.push(self.next("expected positional argument")?)
                            } else {
                                break;
                            }
                        }
                    }
                    stack.push(Open {
                        keyword,
                        args,
                        items: vec![],
                        elements: vec![],
                    });
                }
                "/end" => {
                    self.position += 1;
                    let end = self.next("expected block name after /end")?;
                    let open = stack
                        .pop()
                        .ok_or_else(|| Self::err(&end, format!("unexpected /end {}", end.text)))?;
                    if open.keyword.text != end.text {
                        return Err(Self::err(
                            &end,
                            format!(
                                "/end {} does not match /begin {}",
                                end.text, open.keyword.text
                            ),
                        ));
                    }
                    let block = open.done();
                    if let Some(p) = stack.last_mut() {
                        p.elements.push(Element::Block(block));
                    } else {
                        roots.push(RootElement::Block(block))
                    }
                }
                _ => {
                    let keyword = self.next("expected keyword")?;
                    let mut values = vec![];
                    let mut value_locations = vec![];
                    while let Some(v) = self.current() {
                        if v.line != keyword.line || matches!(v.text.as_str(), "/begin" | "/end") {
                            break;
                        }
                        values.push(v.text.clone());
                        value_locations.push(v.loc());
                        self.position += 1;
                    }
                    let item = RawItem {
                        keyword: keyword.text,
                        values,
                        value_locations,
                        line: keyword.line,
                        column: keyword.column,
                    };
                    if let Some(parent) = stack.last_mut() {
                        parent.items.push(item.clone());
                        parent.elements.push(Element::Item(item));
                    } else {
                        roots.push(RootElement::Item(item));
                    }
                }
            }
        }
        if let Some(open) = stack.last() {
            return Err(A2lImportError::Syntax {
                line: self.eof.line,
                column: self.eof.column,
                message: format!("unterminated /begin {}", open.keyword.text),
            });
        }
        Ok(Parsed { roots })
    }
}
fn arity(k: &str) -> usize {
    match k {
        "PROJECT" | "MODULE" => 2,
        "MOD_COMMON" | "RECORD_LAYOUT" => 1,
        "AXIS_DESCR" => 6,
        "CHARACTERISTIC" => 9,
        "AXIS_PTS" => 10,
        "COMPU_METHOD" => 5,
        "MEASUREMENT" => 8,
        _ => 0,
    }
}
/// `AXIS_DESCR` slots 3..=5 (max points, lower, upper) are numeric. A token on
/// a later line than the previous argument that is not a number there cannot
/// be a valid argument, so it is the start of a (vendor) keyword. Same-line
/// tokens keep being consumed so typos still yield "invalid number" errors;
/// non-numeric slots (attribute, quantity, conversion) stay ambiguous.
fn is_foreign_in_numeric_slot(
    block: &str,
    index: usize,
    token: &Token,
    previous_line: usize,
) -> bool {
    if block != "AXIS_DESCR" || token.line == previous_line {
        return false;
    }
    match index {
        3 => token.text.parse::<u32>().is_err(),
        4 | 5 => A2lNumber::parse(&token.text).is_none(),
        _ => false,
    }
}
fn is_internal_keyword(block: &str, keyword: &str) -> bool {
    match block {
        "CHARACTERISTIC" => matches!(
            keyword,
            "ARRAY_SIZE"
                | "BIT_MASK"
                | "BYTE_ORDER"
                | "CALIBRATION_ACCESS"
                | "COMPU_TAB_REF"
                | "COMPARISON_QUANTITY"
                | "DISCRETE"
                | "MATRIX_DIM"
                | "DISPLAY_IDENTIFIER"
                | "ECU_ADDRESS_EXTENSION"
                | "EXTENDED_LIMITS"
                | "FORMAT"
                | "FUNCTION_LIST"
                | "GUARD_RAILS"
                | "MAX_REFRESH"
                | "NUMBER"
                | "PHYS_UNIT"
                | "READ_ONLY"
                | "REF_MEMORY_SEGMENT"
                | "STEP_SIZE"
                | "SYMBOL_LINK"
        ),
        "AXIS_PTS" => matches!(
            keyword,
            "ARRAY_SIZE"
                | "BYTE_ORDER"
                | "CALIBRATION_ACCESS"
                | "DEPOSIT"
                | "DISPLAY_IDENTIFIER"
                | "ECU_ADDRESS_EXTENSION"
                | "EXTENDED_LIMITS"
                | "FORMAT"
                | "FUNCTION_LIST"
                | "GUARD_RAILS"
                | "MAX_GRAD"
                | "MONOTONY"
                | "PHYS_UNIT"
                | "READ_ONLY"
                | "REF_MEMORY_SEGMENT"
                | "STEP_SIZE"
                | "SYMBOL_LINK"
        ),
        "MEASUREMENT" => matches!(
            keyword,
            "ARRAY_SIZE"
                | "BIT_MASK"
                | "BYTE_ORDER"
                | "DISCRETE"
                | "DISPLAY_IDENTIFIER"
                | "ECU_ADDRESS_EXTENSION"
                | "FORMAT"
                | "FUNCTION_LIST"
                | "MAX_REFRESH"
                | "MATRIX_DIM"
                | "PHYS_UNIT"
                | "READ_WRITE"
                | "REF_MEMORY_SEGMENT"
                | "SYMBOL_LINK"
        ),
        "COMPU_METHOD" => matches!(
            keyword,
            "COEFFS"
                | "COEFFS_LINEAR"
                | "COMPU_TAB_REF"
                | "FORMAT"
                | "REF_UNIT"
                | "STATUS_STRING_REF"
        ),
        "RECORD_LAYOUT" => {
            record_layout_shape(keyword).is_some()
                || is_alignment(keyword)
                || keyword == "STATIC_RECORD_LAYOUT"
        }
        // `MAX_AXIS_PTS` is not an ASAP2 AXIS_DESCR keyword (the maximum is the
        // 4th positional argument); it is kept because T002-A/B fixtures use it.
        "AXIS_DESCR" => matches!(
            keyword,
            "AXIS_PTS_REF"
                | "BYTE_ORDER"
                | "CURVE_AXIS_REF"
                | "DEPOSIT"
                | "EXTENDED_LIMITS"
                | "FIX_AXIS_PAR"
                | "FIX_AXIS_PAR_DIST"
                | "FIX_AXIS_PAR_LIST"
                | "FORMAT"
                | "MAX_AXIS_PTS"
                | "MAX_GRAD"
                | "MONOTONY"
                | "PHYS_UNIT"
                | "READ_ONLY"
                | "STEP_SIZE"
        ),
        "MOD_COMMON" => {
            is_alignment(keyword)
                || matches!(
                    keyword,
                    "BYTE_ORDER" | "DATA_SIZE" | "DEPOSIT" | "S_REC_LAYOUT"
                )
        }
        _ => false,
    }
}
fn is_alignment(keyword: &str) -> bool {
    matches!(
        keyword,
        "ALIGNMENT_BYTE"
            | "ALIGNMENT_WORD"
            | "ALIGNMENT_LONG"
            | "ALIGNMENT_INT64"
            | "ALIGNMENT_FLOAT16_IEEE"
            | "ALIGNMENT_FLOAT32_IEEE"
            | "ALIGNMENT_FLOAT64_IEEE"
    )
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutShape {
    /// `Position Datatype IndexMode Addresstype`
    Fnc,
    /// `Position Datatype IndexIncr Addressing`
    AxisPts,
    /// `Position Datatype MaxNoOfRescalePairs IndexIncr Addressing`
    Rescale,
    /// `Position Datatype`
    PositionType,
    /// `Position DataSize`
    Reserved,
    /// `NumberOfAxisPoints`
    FixedCount,
}
impl LayoutShape {
    fn min_args(self) -> usize {
        match self {
            Self::Fnc | Self::AxisPts => 4,
            Self::Rescale => 5,
            Self::PositionType | Self::Reserved => 2,
            Self::FixedCount => 1,
        }
    }

    fn exact_args(self) -> usize {
        self.min_args()
    }
}
fn record_layout_shape(keyword: &str) -> Option<LayoutShape> {
    match keyword {
        "FNC_VALUES" => return Some(LayoutShape::Fnc),
        "IDENTIFICATION" | "RIP_ADDR_W" => return Some(LayoutShape::PositionType),
        "RESERVED" => return Some(LayoutShape::Reserved),
        _ => {}
    }
    const AXES: [&str; 5] = ["X", "Y", "Z", "4", "5"];
    const FAMILIES: [(&str, LayoutShape); 10] = [
        ("AXIS_PTS_", LayoutShape::AxisPts),
        ("AXIS_RESCALE_", LayoutShape::Rescale),
        ("FIX_NO_AXIS_PTS_", LayoutShape::FixedCount),
        ("DIST_OP_", LayoutShape::PositionType),
        ("NO_AXIS_PTS_", LayoutShape::PositionType),
        ("NO_RESCALE_", LayoutShape::PositionType),
        ("OFFSET_", LayoutShape::PositionType),
        ("RIP_ADDR_", LayoutShape::PositionType),
        ("SHIFT_OP_", LayoutShape::PositionType),
        ("SRC_ADDR_", LayoutShape::PositionType),
    ];
    FAMILIES.into_iter().find_map(|(prefix, shape)| {
        keyword
            .strip_prefix(prefix)
            .filter(|axis| AXES.contains(axis))
            .map(|_| shape)
    })
}
fn children(b: &Block) -> impl Iterator<Item = &Block> {
    b.elements.iter().filter_map(|element| match element {
        Element::Block(child) => Some(child),
        Element::Item(_) => None,
    })
}
fn store_raw(b: &Block, store: &mut Vec<RawBlock>) -> RawBlockId {
    if let Some(id) = b.raw_block {
        return id;
    }
    let id = store.len();
    store.push(RawBlock {
        keyword: b.keyword.text.clone(),
        arguments: b.args.iter().map(|x| x.text.clone()).collect(),
        argument_locations: b.args.iter().map(Token::loc).collect(),
        elements: vec![],
        line: b.keyword.line,
        column: b.keyword.column,
    });
    let elements = b
        .elements
        .iter()
        .map(|element| match element {
            Element::Item(item) => RawElement::Item(item.clone()),
            Element::Block(child) => RawElement::Block(store_raw(child, store)),
        })
        .collect();
    store[id].elements = elements;
    id
}
fn raw_block(b: &Block) -> RawBlockId {
    b.raw_block.expect("raw blocks assigned before projection")
}
fn assign_raw_blocks(b: &mut Block, store: &mut Vec<RawBlock>) -> RawBlockId {
    let id = store.len();
    b.raw_block = Some(id);
    store.push(RawBlock {
        keyword: b.keyword.text.clone(),
        arguments: b.args.iter().map(|x| x.text.clone()).collect(),
        argument_locations: b.args.iter().map(Token::loc).collect(),
        elements: vec![],
        line: b.keyword.line,
        column: b.keyword.column,
    });
    let elements = b
        .elements
        .iter_mut()
        .map(|element| match element {
            Element::Item(item) => RawElement::Item(item.clone()),
            Element::Block(child) => RawElement::Block(assign_raw_blocks(child, store)),
        })
        .collect();
    store[id].elements = elements;
    id
}
fn child_raw_blocks(b: &Block, store: &[RawBlock]) -> Vec<RawBlockId> {
    store[raw_block(b)]
        .elements
        .iter()
        .filter_map(|element| match element {
            RawElement::Block(id) => Some(*id),
            RawElement::Item(_) => None,
        })
        .collect()
}
fn arg(b: &Block, n: usize) -> Option<String> {
    b.args.get(n).map(|x| x.text.clone())
}
fn arg_loc(b: &Block, n: usize) -> Option<SourceLocation> {
    b.args.get(n).map(Token::loc)
}
fn value(items: &[RawItem], key: &str) -> Option<String> {
    items
        .iter()
        .find(|x| x.keyword == key)
        .and_then(|x| x.values.first())
        .cloned()
}
fn diagnostic(
    ds: &mut Vec<Diagnostic>,
    severity: Severity,
    message: impl Into<String>,
    at: SourceLocation,
) {
    ds.push(Diagnostic {
        severity,
        message: message.into(),
        line: at.line,
        column: at.column,
    })
}
fn diagnose_item(item: &RawItem, ds: &mut Vec<Diagnostic>) {
    diagnostic(
        ds,
        Severity::Unsupported,
        format!(
            "unsupported A2L keyword `{}` preserved as raw data",
            item.keyword
        ),
        SourceLocation {
            line: item.line,
            column: item.column,
        },
    );
}
fn diagnose_uninterpreted_items(block: &Block, ds: &mut Vec<Diagnostic>) {
    for item in &block.items {
        if !is_internal_keyword(&block.keyword.text, &item.keyword) {
            diagnose_item(item, ds);
        }
    }
}
fn incomplete(b: &Block, n: usize, name: &str, ds: &mut Vec<Diagnostic>) {
    if b.args.len() < n {
        diagnostic(
            ds,
            Severity::Error,
            format!("{name} declaration is incomplete"),
            b.keyword.loc(),
        )
    }
}
fn u64_value(text: &str, at: SourceLocation, ds: &mut Vec<Diagnostic>) -> Option<u64> {
    let r = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .map_or_else(|| text.parse(), |x| u64::from_str_radix(x, 16));
    match r {
        Ok(x) => Some(x),
        Err(_) => {
            diagnostic(
                ds,
                Severity::Error,
                format!("invalid unsigned number `{text}`"),
                at,
            );
            None
        }
    }
}
fn u64_arg(b: &Block, n: usize, ds: &mut Vec<Diagnostic>) -> Option<u64> {
    arg(b, n)
        .zip(arg_loc(b, n))
        .and_then(|(x, l)| u64_value(&x, l, ds))
}
fn u32_arg(b: &Block, n: usize, ds: &mut Vec<Diagnostic>) -> Option<u32> {
    let (text, at) = arg(b, n).zip(arg_loc(b, n))?;
    let x = u64_value(&text, at, ds)?;
    u32::try_from(x)
        .map_err(|_| {
            diagnostic(
                ds,
                Severity::Error,
                format!("unsigned number outside u32 range `{text}`"),
                at,
            )
        })
        .ok()
}
fn u32_item(i: &RawItem, ds: &mut Vec<Diagnostic>) -> Option<u32> {
    let text = i.values.first()?;
    let at = i
        .value_locations
        .first()
        .copied()
        .unwrap_or(SourceLocation {
            line: i.line,
            column: i.column,
        });
    let x = u64_value(text, at, ds)?;
    u32::try_from(x)
        .map_err(|_| {
            diagnostic(
                ds,
                Severity::Error,
                format!("unsigned number outside u32 range `{text}`"),
                at,
            )
        })
        .ok()
}

fn u32_at(text: &str, at: SourceLocation, ds: &mut Vec<Diagnostic>) -> Option<u32> {
    let x = u64_value(text, at, ds)?;
    u32::try_from(x)
        .map_err(|_| {
            diagnostic(
                ds,
                Severity::Error,
                format!("unsigned number outside u32 range `{text}`"),
                at,
            )
        })
        .ok()
}
fn i64_at(text: &str, at: SourceLocation, ds: &mut Vec<Diagnostic>) -> Option<i64> {
    text.parse().ok().or_else(|| {
        diagnostic(ds, Severity::Error, format!("invalid integer `{text}`"), at);
        None
    })
}
fn number_at(text: &str, at: SourceLocation, ds: &mut Vec<Diagnostic>) -> Option<A2lNumber> {
    let n = A2lNumber::parse(text);
    if n.is_none() {
        diagnostic(
            ds,
            Severity::Error,
            format!("invalid finite number `{text}`"),
            at,
        );
    }
    n
}
fn number_arg(b: &Block, n: usize, ds: &mut Vec<Diagnostic>) -> Option<A2lNumber> {
    let (text, at) = arg(b, n).zip(arg_loc(b, n))?;
    number_at(&text, at, ds)
}
fn item_loc(i: &RawItem) -> SourceLocation {
    SourceLocation {
        line: i.line,
        column: i.column,
    }
}
fn value_loc(i: &RawItem, n: usize) -> SourceLocation {
    i.value_locations.get(n).copied().unwrap_or(item_loc(i))
}
fn first_item<'a>(items: &'a [RawItem], key: &str) -> Option<&'a RawItem> {
    items.iter().find(|x| x.keyword == key)
}
/// Three-value `FIX_AXIS_PAR*` item: signed, signed, unsigned (u32).
fn fix_triple(items: &[RawItem], key: &str, ds: &mut Vec<Diagnostic>) -> Option<(i64, i64, u32)> {
    let declarations: Vec<_> = items.iter().filter(|item| item.keyword == key).collect();
    let item = declarations.first().copied()?;
    if declarations.len() > 1 {
        for duplicate in declarations.iter().skip(1) {
            diagnostic(
                ds,
                Severity::Error,
                format!("duplicate {key} declaration is ambiguous"),
                item_loc(duplicate),
            );
        }
    }
    if item.values.len() < 3 {
        diagnostic(
            ds,
            Severity::Error,
            format!("{key} is incomplete"),
            item_loc(item),
        );
        return None;
    }
    if item.values.len() > 3 {
        diagnostic(
            ds,
            Severity::Error,
            format!("{key} has extra parameters; expected exactly three"),
            value_loc(item, 3),
        );
        return None;
    }
    let a = i64_at(&item.values[0], value_loc(item, 0), ds);
    let b = i64_at(&item.values[1], value_loc(item, 1), ds);
    let c = u32_at(&item.values[2], value_loc(item, 2), ds);
    Some((a?, b?, c?))
}
fn axis_description(b: &Block, ds: &mut Vec<Diagnostic>, store: &[RawBlock]) -> AxisDescription {
    if b.args.is_empty() {
        diagnostic(
            ds,
            Severity::Warning,
            "AXIS_DESCR has no positional arguments (legacy form); attribute, quantity, conversion and limits are unavailable",
            b.keyword.loc(),
        );
    } else {
        incomplete(b, 6, "AXIS_DESCR", ds);
    }
    let attribute = arg(b, 0).map(|t| AxisAttribute::from_text(&t));
    if let (Some(AxisAttribute::Unknown(text)), Some(at)) = (&attribute, arg_loc(b, 0)) {
        diagnostic(
            ds,
            Severity::Unsupported,
            format!("unrecognized axis attribute `{text}` preserved verbatim"),
            at,
        );
    }
    let max_axis_points = arg(b, 3)
        .zip(arg_loc(b, 3))
        .and_then(|(t, at)| u32_at(&t, at, ds));
    let lower_limit = number_arg(b, 4, ds);
    let upper_limit = number_arg(b, 5, ds);
    let text_item = |key: &str| value(&b.items, key);
    let axis_pts_ref_item = first_item(&b.items, "AXIS_PTS_REF").filter(|i| !i.values.is_empty());
    let curve_ref_item = first_item(&b.items, "CURVE_AXIS_REF").filter(|i| !i.values.is_empty());
    if attribute == Some(AxisAttribute::Com) && axis_pts_ref_item.is_none() {
        diagnostic(
            ds,
            Severity::Warning,
            "COM_AXIS declares no AXIS_PTS_REF; no shared axis is assumed",
            b.keyword.loc(),
        );
    }
    let fix_axis_par =
        fix_triple(&b.items, "FIX_AXIS_PAR", ds).map(|(offset, shift, number_of_points)| {
            FixAxisPar {
                offset,
                shift,
                number_of_points,
            }
        });
    let fix_axis_par_dist = fix_triple(&b.items, "FIX_AXIS_PAR_DIST", ds).map(
        |(offset, distance, number_of_points)| FixAxisParDist {
            offset,
            distance,
            number_of_points,
        },
    );
    let fix_axis_par_list_items: Vec<_> = b
        .items
        .iter()
        .filter(|item| item.keyword == "FIX_AXIS_PAR_LIST")
        .collect();
    for duplicate in fix_axis_par_list_items.iter().skip(1) {
        diagnostic(
            ds,
            Severity::Error,
            "duplicate FIX_AXIS_PAR_LIST declaration is ambiguous",
            item_loc(duplicate),
        );
    }
    let fix_axis_par_list = fix_axis_par_list_items.first().copied().and_then(|item| {
        if item.values.is_empty() {
            diagnostic(
                ds,
                Severity::Error,
                "FIX_AXIS_PAR_LIST is incomplete",
                item_loc(item),
            );
            return None;
        }
        let parsed: Vec<_> = item
            .values
            .iter()
            .enumerate()
            .map(|(n, t)| number_at(t, value_loc(item, n), ds))
            .collect();
        parsed.into_iter().collect()
    });
    AxisDescription {
        attribute,
        input_quantity: arg(b, 1),
        conversion_reference: arg(b, 2),
        max_axis_points,
        lower_limit,
        upper_limit,
        axis_pts_ref: axis_pts_ref_item.map(|i| i.values[0].clone()),
        curve_axis_ref: curve_ref_item.map(|i| i.values[0].clone()),
        byte_order: text_item("BYTE_ORDER"),
        deposit: text_item("DEPOSIT"),
        format: text_item("FORMAT"),
        phys_unit: text_item("PHYS_UNIT"),
        monotony: text_item("MONOTONY"),
        read_only: first_item(&b.items, "READ_ONLY").is_some(),
        fix_axis_par,
        fix_axis_par_dist,
        fix_axis_par_list,
        positional_arguments: b.args.iter().map(|t| t.text.clone()).collect(),
        argument_locations: b.args.iter().map(Token::loc).collect(),
        conversion_reference_source: arg_loc(b, 2),
        max_axis_points_source: arg_loc(b, 3),
        axis_pts_ref_source: axis_pts_ref_item.map(|i| value_loc(i, 0)),
        curve_axis_ref_source: curve_ref_item.map(|i| value_loc(i, 0)),
        extra: b.items.clone(),
        raw_blocks: child_raw_blocks(b, store),
        raw_block: raw_block(b),
        source: b.keyword.loc(),
    }
}
fn record_layout_elements(b: &Block, ds: &mut Vec<Diagnostic>) -> Vec<RecordLayoutElement> {
    let mut out = vec![];
    for item in &b.items {
        let Some(shape) = record_layout_shape(&item.keyword) else {
            continue;
        };
        if item.values.len() < shape.min_args() {
            diagnostic(
                ds,
                Severity::Error,
                format!("{} is incomplete", item.keyword),
                item_loc(item),
            );
        }
        if item.values.len() > shape.exact_args() {
            diagnostic(
                ds,
                Severity::Error,
                format!(
                    "{} has too many arguments: expected {}, found {}",
                    item.keyword,
                    shape.exact_args(),
                    item.values.len()
                ),
                item_loc(item),
            );
        }
        let text = |n: usize| item.values.get(n).cloned();
        let number = |n: usize, ds: &mut Vec<Diagnostic>| {
            item.values
                .get(n)
                .and_then(|t| u32_at(t, value_loc(item, n), ds))
        };
        let mut e = RecordLayoutElement {
            keyword: item.keyword.clone(),
            position: None,
            data_type: None,
            index_mode: None,
            index_increment: None,
            address_type: None,
            max_rescale_pairs: None,
            fixed_axis_points: None,
            arguments: item.values.clone(),
            argument_locations: item.value_locations.clone(),
            source: item_loc(item),
        };
        if shape == LayoutShape::FixedCount {
            e.fixed_axis_points = number(0, ds);
        } else {
            e.position = number(0, ds);
            e.data_type = text(1);
        }
        match shape {
            LayoutShape::Fnc => {
                e.index_mode = text(2);
                e.address_type = text(3);
            }
            LayoutShape::AxisPts => {
                e.index_increment = text(2);
                e.address_type = text(3);
            }
            LayoutShape::Rescale => {
                e.max_rescale_pairs = number(2, ds);
                e.index_increment = text(3);
                e.address_type = text(4);
            }
            LayoutShape::PositionType | LayoutShape::Reserved | LayoutShape::FixedCount => {}
        }
        out.push(e);
    }
    out
}
fn finite_float_arg(b: &Block, n: usize, ds: &mut Vec<Diagnostic>) {
    if let Some(token) = b.args.get(n) {
        match token.text.parse::<f64>() {
            Ok(v) if v.is_finite() => {}
            _ => diagnostic(
                ds,
                Severity::Error,
                format!("invalid finite number `{}`", token.text),
                token.loc(),
            ),
        }
    }
}

fn build_document(mut parsed: Parsed) -> Result<A2lDocument, A2lImportError> {
    let mut raw_blocks = vec![];
    for root in &mut parsed.roots {
        if let RootElement::Block(block) = root {
            assign_raw_blocks(block, &mut raw_blocks);
        }
    }
    let root_elements = parsed
        .roots
        .iter()
        .map(|root| match root {
            RootElement::Item(item) => RawElement::Item(item.clone()),
            RootElement::Block(block) => RawElement::Block(raw_block(block)),
        })
        .collect();
    let project_index = parsed
        .roots
        .iter()
        .position(|x| matches!(x, RootElement::Block(block) if block.keyword.text == "PROJECT"))
        .ok_or(A2lImportError::Syntax {
            line: 1,
            column: 1,
            message: "missing PROJECT block".into(),
        })?;
    let RootElement::Block(project) = parsed.roots.remove(project_index) else {
        unreachable!("PROJECT index identifies a block")
    };
    let mut ds = vec![];
    let mut unknown = vec![];
    for root in &parsed.roots {
        match root {
            RootElement::Item(item) if item.keyword == "ASAP2_VERSION" => {}
            RootElement::Item(item) => diagnose_item(item, &mut ds),
            RootElement::Block(block) => {
                diagnostic(
                    &mut ds,
                    Severity::Error,
                    "A2L input must contain exactly one PROJECT root block",
                    block.keyword.loc(),
                );
                preserve(block, &mut vec![], &mut unknown, &raw_blocks, &mut ds);
            }
        }
    }
    incomplete(&project, 2, "PROJECT", &mut ds);
    let mut model = Project {
        name: arg(&project, 0).unwrap_or_default(),
        description: arg(&project, 1),
        version: None,
        modules: vec![],
        extra: project.items.clone(),
        raw_blocks: vec![],
        raw_block: raw_block(&project),
    };
    for i in &project.items {
        if i.keyword == "ASAM_VERSION" && i.values.len() >= 2 {
            model.version = Some(format!("{}.{}", i.values[0], i.values[1]));
        }
    }
    for root in &parsed.roots {
        if let RootElement::Item(item) = root
            && item.keyword == "ASAP2_VERSION"
            && item.values.len() >= 2
        {
            model.version = Some(format!("{}.{}", item.values[0], item.values[1]));
        }
    }
    for child in children(&project) {
        match child.keyword.text.as_str() {
            "MODULE" => model
                .modules
                .push(module(child, &mut ds, &mut unknown, &raw_blocks)),
            "HEADER" => {
                for i in &child.items {
                    if i.keyword == "ASAM_VERSION" && i.values.len() >= 2 {
                        model.version = Some(format!("{}.{}", i.values[0], i.values[1]));
                    }
                }
                model.raw_blocks.push(store_raw(child, &mut raw_blocks));
            }
            _ => {
                preserve(
                    child,
                    &mut model.raw_blocks,
                    &mut unknown,
                    &raw_blocks,
                    &mut ds,
                );
            }
        }
    }
    validate(&model, &mut ds);
    Ok(A2lDocument {
        project: model,
        diagnostics: ds,
        root_elements,
        raw_blocks,
        unknown_blocks: unknown,
    })
}
fn module(
    b: &Block,
    ds: &mut Vec<Diagnostic>,
    unknown: &mut Vec<RawBlockId>,
    store: &[RawBlock],
) -> Module {
    incomplete(b, 2, "MODULE", ds);
    diagnose_uninterpreted_items(b, ds);
    let mut m = Module {
        name: arg(b, 0).unwrap_or_default(),
        description: arg(b, 1),
        mod_common: None,
        characteristics: vec![],
        axis_points: vec![],
        compu_methods: vec![],
        record_layouts: vec![],
        measurements: vec![],
        extra: b.items.clone(),
        raw_blocks: vec![],
        raw_block: raw_block(b),
    };
    for c in children(b) {
        match c.keyword.text.as_str() {
            "MOD_COMMON" => {
                if m.mod_common.is_some() {
                    diagnostic(
                        ds,
                        Severity::Error,
                        "MODULE contains multiple MOD_COMMON blocks",
                        c.keyword.loc(),
                    );
                    preserve(c, &mut m.raw_blocks, unknown, store, ds);
                } else {
                    diagnose_uninterpreted_items(c, ds);
                    diagnose_uninterpreted_children(c, ds);
                    m.mod_common = Some(ModCommon {
                        description: arg(c, 0),
                        byte_order: value(&c.items, "BYTE_ORDER"),
                        extra: c.items.clone(),
                        raw_blocks: child_raw_blocks(c, store),
                        raw_block: raw_block(c),
                    })
                }
            }
            "CHARACTERISTIC" => m.characteristics.push(characteristic(c, ds, store)),
            "AXIS_PTS" => m.axis_points.push(axis(c, ds, store, unknown)),
            "COMPU_METHOD" => m.compu_methods.push(compu(c, ds, store, unknown)),
            "RECORD_LAYOUT" => {
                diagnose_uninterpreted_items(c, ds);
                diagnose_uninterpreted_children(c, ds);
                incomplete(c, 1, "RECORD_LAYOUT", ds);
                m.record_layouts.push(RecordLayout {
                    name: arg(c, 0).unwrap_or_default(),
                    elements: record_layout_elements(c, ds),
                    extra: c.items.clone(),
                    raw_blocks: child_raw_blocks(c, store),
                    raw_block: raw_block(c),
                    source: c.keyword.loc(),
                })
            }
            "MEASUREMENT" => m.measurements.push(measurement(c, ds, store, unknown)),
            _ => {
                preserve(c, &mut m.raw_blocks, unknown, store, ds);
            }
        }
    }
    m
}
fn characteristic(b: &Block, ds: &mut Vec<Diagnostic>, store: &[RawBlock]) -> Characteristic {
    incomplete(b, 9, "CHARACTERISTIC", ds);
    diagnose_uninterpreted_items(b, ds);
    for index in [5, 7, 8] {
        finite_float_arg(b, index, ds);
    }
    let mut dimensions = vec![];
    let mut axis_references = vec![];
    let mut axis_reference_sources = vec![];
    let mut axis_descriptions = vec![];
    let raw_block = raw_block(b);
    let raw_blocks = child_raw_blocks(b, store);
    for c in children(b) {
        if c.keyword.text == "AXIS_DESCR" {
            axis_descriptions.push(axis_description(c, ds, store));
            if let Some(item) = c.items.iter().find(|item| item.keyword == "AXIS_PTS_REF")
                && let Some(value) = item.values.first()
            {
                axis_references.push(value.clone());
                axis_reference_sources.push(item.value_locations.first().copied().unwrap_or(
                    SourceLocation {
                        line: item.line,
                        column: item.column,
                    },
                ));
            }
            for i in c.items.iter().filter(|x| x.keyword == "MAX_AXIS_PTS") {
                if let Some(v) = u32_item(i, ds) {
                    dimensions.push(v)
                }
            }
            diagnose_uninterpreted_items(c, ds);
            diagnose_uninterpreted_children(c, ds);
        } else {
            diagnose_uninterpreted_tree(c, ds);
        }
    }
    let layout_reference_source = arg_loc(b, 4);
    let conversion_reference_source = arg_loc(b, 6);
    Characteristic {
        name: arg(b, 0).unwrap_or_default(),
        description: arg(b, 1),
        address: u64_arg(b, 3, ds),
        characteristic_type: arg(b, 2),
        layout_reference: arg(b, 4),
        conversion_reference: arg(b, 6),
        dimensions,
        axis_references,
        axis_reference_sources,
        axis_descriptions,
        extra: b.items.clone(),
        raw_blocks,
        raw_block,
        source: b.keyword.loc(),
        layout_reference_source,
        conversion_reference_source,
    }
}
fn axis(
    b: &Block,
    ds: &mut Vec<Diagnostic>,
    store: &[RawBlock],
    _unknown: &mut Vec<RawBlockId>,
) -> AxisPoints {
    incomplete(b, 10, "AXIS_PTS", ds);
    diagnose_uninterpreted_items(b, ds);
    finite_float_arg(b, 5, ds);
    let lower_limit = number_arg(b, 8, ds);
    let upper_limit = number_arg(b, 9, ds);
    let layout_reference_source = arg_loc(b, 4);
    let conversion_reference_source = arg_loc(b, 6);
    let raw_block = raw_block(b);
    let raw_blocks = child_raw_blocks(b, store);
    for child in children(b) {
        diagnose_uninterpreted_tree(child, ds);
    }
    AxisPoints {
        name: arg(b, 0).unwrap_or_default(),
        description: arg(b, 1),
        address: u64_arg(b, 2, ds),
        layout_reference: arg(b, 4),
        conversion_reference: arg(b, 6),
        max_axis_points: u32_arg(b, 7, ds),
        input_quantity: arg(b, 3),
        lower_limit,
        upper_limit,
        phys_unit: value(&b.items, "PHYS_UNIT"),
        extra: b.items.clone(),
        raw_blocks,
        raw_block,
        source: b.keyword.loc(),
        layout_reference_source,
        conversion_reference_source,
    }
}
fn compu(
    b: &Block,
    ds: &mut Vec<Diagnostic>,
    store: &[RawBlock],
    _unknown: &mut Vec<RawBlockId>,
) -> CompuMethod {
    incomplete(b, 5, "COMPU_METHOD", ds);
    diagnose_uninterpreted_items(b, ds);
    diagnose_uninterpreted_children(b, ds);
    CompuMethod {
        name: arg(b, 0).unwrap_or_default(),
        description: arg(b, 1),
        conversion_type: arg(b, 2),
        format: arg(b, 3),
        unit: arg(b, 4),
        extra: b.items.clone(),
        raw_blocks: child_raw_blocks(b, store),
        raw_block: raw_block(b),
        source: b.keyword.loc(),
    }
}
fn measurement(
    b: &Block,
    ds: &mut Vec<Diagnostic>,
    store: &[RawBlock],
    _unknown: &mut Vec<RawBlockId>,
) -> Measurement {
    incomplete(b, 8, "MEASUREMENT", ds);
    diagnose_uninterpreted_items(b, ds);
    u32_arg(b, 4, ds);
    for index in [5, 6, 7] {
        finite_float_arg(b, index, ds);
    }
    let conversion_reference_source = arg_loc(b, 3);
    let raw_block = raw_block(b);
    let raw_blocks = child_raw_blocks(b, store);
    for child in children(b) {
        diagnose_uninterpreted_tree(child, ds);
    }
    Measurement {
        name: arg(b, 0).unwrap_or_default(),
        description: arg(b, 1),
        data_type: arg(b, 2),
        conversion_reference: arg(b, 3),
        unit: value(&b.items, "PHYS_UNIT"),
        extra: b.items.clone(),
        raw_blocks,
        raw_block,
        source: b.keyword.loc(),
        conversion_reference_source,
    }
}
fn preserve(
    b: &Block,
    dst: &mut Vec<RawBlockId>,
    all: &mut Vec<RawBlockId>,
    _store: &[RawBlock],
    ds: &mut Vec<Diagnostic>,
) -> RawBlockId {
    diagnose_uninterpreted_tree(b, ds);
    let id = raw_block(b);
    dst.push(id);
    all.push(id);
    id
}
fn diagnose_uninterpreted_tree(block: &Block, ds: &mut Vec<Diagnostic>) {
    diagnostic(
        ds,
        Severity::Unsupported,
        format!(
            "unsupported A2L block `{}` preserved as raw data",
            block.keyword.text
        ),
        block.keyword.loc(),
    );
    diagnose_uninterpreted_items(block, ds);
    for child in children(block) {
        diagnose_uninterpreted_tree(child, ds);
    }
}
fn diagnose_uninterpreted_children(block: &Block, ds: &mut Vec<Diagnostic>) {
    for child in children(block) {
        diagnose_uninterpreted_tree(child, ds);
    }
}
fn validate(p: &Project, ds: &mut Vec<Diagnostic>) {
    for m in &p.modules {
        let mut seen = HashMap::new();
        for (name, source) in m
            .characteristics
            .iter()
            .map(|x| (&x.name, x.source))
            .chain(m.axis_points.iter().map(|x| (&x.name, x.source)))
            .chain(m.compu_methods.iter().map(|x| (&x.name, x.source)))
            .chain(m.record_layouts.iter().map(|x| (&x.name, x.source)))
            .chain(m.measurements.iter().map(|x| (&x.name, x.source)))
        {
            if seen.insert(name, ()).is_some() {
                diagnostic(
                    ds,
                    Severity::Error,
                    format!("duplicate identifier `{name}` in MODULE `{}`", m.name),
                    source,
                )
            }
        }
        let conversions: HashSet<_> = m.compu_methods.iter().map(|x| x.name.as_str()).collect();
        let layouts: HashSet<_> = m.record_layouts.iter().map(|x| x.name.as_str()).collect();
        let axes: HashSet<_> = m.axis_points.iter().map(|x| x.name.as_str()).collect();
        let characteristics: HashSet<_> =
            m.characteristics.iter().map(|x| x.name.as_str()).collect();
        for c in &m.characteristics {
            reference(
                &c.conversion_reference,
                c.conversion_reference_source,
                &conversions,
                "conversion",
                &c.name,
                &["NO_COMPU_METHOD"],
                ds,
            );
            reference(
                &c.layout_reference,
                c.layout_reference_source,
                &layouts,
                "record layout",
                &c.name,
                &["NO_RECORD_LAYOUT"],
                ds,
            );
            for d in &c.axis_descriptions {
                reference(
                    &d.conversion_reference,
                    d.conversion_reference_source,
                    &conversions,
                    "conversion",
                    &c.name,
                    &["NO_COMPU_METHOD"],
                    ds,
                );
                if let Some(target) = &d.curve_axis_ref
                    && !characteristics.contains(target.as_str())
                {
                    diagnostic(
                        ds,
                        Severity::Error,
                        format!(
                            "CHARACTERISTIC `{}` references missing CURVE_AXIS_REF `{target}`",
                            c.name
                        ),
                        d.curve_axis_ref_source.unwrap_or(d.source),
                    )
                }
            }
            for (a, source) in c.axis_references.iter().zip(&c.axis_reference_sources) {
                if !axes.contains(a.as_str()) {
                    diagnostic(
                        ds,
                        Severity::Error,
                        format!(
                            "CHARACTERISTIC `{}` references missing AXIS_PTS `{a}`",
                            c.name
                        ),
                        *source,
                    )
                }
            }
        }
        for a in &m.axis_points {
            reference(
                &a.conversion_reference,
                a.conversion_reference_source,
                &conversions,
                "conversion",
                &a.name,
                &["NO_COMPU_METHOD"],
                ds,
            );
            reference(
                &a.layout_reference,
                a.layout_reference_source,
                &layouts,
                "record layout",
                &a.name,
                &["NO_RECORD_LAYOUT"],
                ds,
            )
        }
        for x in &m.measurements {
            reference(
                &x.conversion_reference,
                x.conversion_reference_source,
                &conversions,
                "conversion",
                &x.name,
                &["NO_COMPU_METHOD"],
                ds,
            )
        }
    }
}
fn reference(
    v: &Option<String>,
    at: Option<SourceLocation>,
    available: &HashSet<&str>,
    kind: &str,
    owner: &str,
    sentinels: &[&str],
    ds: &mut Vec<Diagnostic>,
) {
    if let Some(v) = v
        && !sentinels.contains(&v.as_str())
        && !available.contains(v.as_str())
    {
        diagnostic(
            ds,
            Severity::Error,
            format!("`{owner}` references missing {kind} `{v}`"),
            at.unwrap_or(SourceLocation { line: 1, column: 1 }),
        )
    }
}
