//! T002-C2.1: planning of ASAP2 `RECORD_LAYOUT` blocks (no axis/CURVE/MAP decoding).
//!
//! The planner turns the typed [`RecordLayoutElement`]s produced by the A2L
//! importer into an ordered [`RecordPlan`] and computes byte offsets **only**
//! when they are deterministic. It never reads binary data and never invents a
//! padding rule: whenever an element would need padding to satisfy its
//! alignment, the layout is reported as ambiguous ([`AxisError::AmbiguousPadding`]).
//!
//! Every error is classified by [`AxisError::verdict`]:
//! [`LayoutVerdict::Malformed`] (broken input), [`LayoutVerdict::Unsupported`]
//! (valid ASAP2 that this planner does not model) or [`LayoutVerdict::Ambiguous`]
//! (the byte layout depends on a rule that is not verified).
//!
//! Two phases:
//! 1. [`RecordPlan::from_layout_with_mod_common`] validates
//!    positions/types/alignments and gives a static offset to every element
//!    that precedes the first element whose size depends on a point count.
//!    [`RecordPlan::from_layout`] remains a compatibility entry point but
//!    rejects the plan because it has no module context.
//! 2. [`RecordPlan::resolve`] takes the point counts (e.g. the value read at
//!    `NO_AXIS_PTS_X`) and returns the final [`Placement`] of every element.
use crate::a2l::{ModCommon, RecordLayout, RecordLayoutElement};
use std::ops::Range;
use thiserror::Error;

/// Axis index of a `RECORD_LAYOUT` element family (`*_X`, `*_Y`, `*_Z`, `*_4`, `*_5`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordAxis {
    X,
    Y,
    Z,
    Fourth,
    Fifth,
}

impl RecordAxis {
    fn from_suffix(s: &str) -> Option<Self> {
        match s {
            "X" => Some(Self::X),
            "Y" => Some(Self::Y),
            "Z" => Some(Self::Z),
            "4" => Some(Self::Fourth),
            "5" => Some(Self::Fifth),
            _ => None,
        }
    }
    fn index(self) -> usize {
        self as usize
    }
}

/// Fixed-size ASAP2 storage types accepted in a `RECORD_LAYOUT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordDataType {
    UByte,
    SByte,
    UWord,
    SWord,
    ULong,
    SLong,
    UInt64,
    Int64,
    Float16,
    Float32,
    Float64,
}

impl RecordDataType {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "UBYTE" => Self::UByte,
            "SBYTE" => Self::SByte,
            "UWORD" => Self::UWord,
            "SWORD" => Self::SWord,
            "ULONG" => Self::ULong,
            "SLONG" => Self::SLong,
            "A_UINT64" => Self::UInt64,
            "A_INT64" => Self::Int64,
            "FLOAT16_IEEE" => Self::Float16,
            "FLOAT32_IEEE" => Self::Float32,
            "FLOAT64_IEEE" => Self::Float64,
            _ => return None,
        })
    }
    /// Size in bytes.
    pub fn size(self) -> u64 {
        match self {
            Self::UByte | Self::SByte => 1,
            Self::UWord | Self::SWord | Self::Float16 => 2,
            Self::ULong | Self::SLong | Self::Float32 => 4,
            Self::UInt64 | Self::Int64 | Self::Float64 => 8,
        }
    }
    /// The `ALIGNMENT_*` keyword that governs this type.
    fn alignment_keyword(self) -> &'static str {
        match self {
            Self::UByte | Self::SByte => "ALIGNMENT_BYTE",
            Self::UWord | Self::SWord => "ALIGNMENT_WORD",
            Self::ULong | Self::SLong => "ALIGNMENT_LONG",
            Self::UInt64 | Self::Int64 => "ALIGNMENT_INT64",
            Self::Float16 => "ALIGNMENT_FLOAT16_IEEE",
            Self::Float32 => "ALIGNMENT_FLOAT32_IEEE",
            Self::Float64 => "ALIGNMENT_FLOAT64_IEEE",
        }
    }
}

/// Role of a planned element. Only these roles are placed by this phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    /// `FNC_VALUES`: the table values (count supplied by the caller).
    FncValues,
    /// `AXIS_PTS_<axis>`: the axis points (count = axis point count).
    AxisPts(RecordAxis),
    /// `NO_AXIS_PTS_<axis>`: a scalar holding the axis point count.
    NoAxisPts(RecordAxis),
    /// `IDENTIFICATION`: a scalar.
    Identification,
}

/// How the size of an element is determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extent {
    /// One value of the element type.
    Scalar,
    /// `count × element size`, count taken from [`PointCounts`] (or `FIX_NO_AXIS_PTS_*`).
    PerPoint(CountSource),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountSource {
    FncValues,
    Axis(RecordAxis),
}

/// One validated element, in ASAP2 position order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedElement {
    pub keyword: String,
    pub kind: ElementKind,
    /// ASAP2 position (≥ 1, contiguous across the record).
    pub position: u32,
    pub data_type: RecordDataType,
    pub extent: Extent,
    /// Original indexing mode, retained for the later decoder.
    pub index_mode: Option<String>,
    /// Original indexing increment, retained for the later decoder.
    pub index_increment: Option<String>,
    /// Alignment (bytes) the element must satisfy: the declared `ALIGNMENT_*`
    /// for its type, else its natural alignment (= its size).
    pub required_alignment: u64,
    /// `true` when `required_alignment` came from an `ALIGNMENT_*` directive.
    pub alignment_declared: bool,
    /// Offset from the record start, known when every earlier element has a
    /// known size; `None` after the first count-dependent element.
    pub static_offset: Option<u64>,
    /// Size in bytes when it does not depend on a point count.
    pub static_size: Option<u64>,
}

/// Validated, order-resolved description of a record; offsets are not final
/// until [`RecordPlan::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordPlan {
    layout_name: String,
    elements: Vec<PlannedElement>,
    /// Alignment the record start address must satisfy (max of the elements').
    required_base_alignment: u64,
    fixed_axis_points: [Option<u32>; 5],
}

/// Number of points per axis and of `FNC_VALUES` items, supplied by the caller
/// (read from `NO_AXIS_PTS_*` or computed from `MATRIX_DIM`/`AXIS_DESCR`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PointCounts {
    fnc_values: Option<u64>,
    axes: [Option<u64>; 5],
}

impl PointCounts {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_axis(mut self, axis: RecordAxis, points: u64) -> Self {
        self.axes[axis.index()] = Some(points);
        self
    }
    pub fn with_fnc_values(mut self, items: u64) -> Self {
        self.fnc_values = Some(items);
        self
    }
    /// Number of `FNC_VALUES` items for the given axis point counts
    /// (checked product; an empty slice is rejected).
    pub fn fnc_items_from_dimensions(dimensions: &[u64]) -> Result<u64, AxisError> {
        if dimensions.is_empty() {
            return Err(AxisError::MissingCount {
                keyword: "FNC_VALUES".into(),
            });
        }
        dimensions
            .iter()
            .try_fold(1u64, |acc, &d| {
                if d > MAX_POINT_COUNT {
                    return Err(AxisError::LimitExceeded {
                        what: "dimension",
                        value: d,
                        maximum: MAX_POINT_COUNT,
                    });
                }
                acc.checked_mul(d).ok_or(AxisError::Overflow {
                    operation: "item count product",
                })
            })
            .and_then(|items| {
                if items > MAX_POINT_COUNT {
                    Err(AxisError::LimitExceeded {
                        what: "item count",
                        value: items,
                        maximum: MAX_POINT_COUNT,
                    })
                } else {
                    Ok(items)
                }
            })
    }
}

/// Final position of one element inside the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub kind: ElementKind,
    pub offset: u64,
    pub size: u64,
}

impl Placement {
    /// Exclusive end offset (checked).
    pub fn end(&self) -> Result<u64, AxisError> {
        checked_add(self.offset, self.size)
    }
    /// Byte range relative to the record start, with checked `usize` conversion.
    pub fn range(&self) -> Result<Range<usize>, AxisError> {
        Ok(to_usize(self.offset)?..to_usize(self.end()?)?)
    }
}

/// All elements placed, with the total record size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRecord {
    pub placements: Vec<Placement>,
    pub total_size: u64,
}

impl ResolvedRecord {
    pub fn placement(&self, kind: ElementKind) -> Option<&Placement> {
        self.placements.iter().find(|p| p.kind == kind)
    }
}

/// Classification of a planning failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutVerdict {
    /// The layout is broken (bad positions, missing fields, overflow…).
    Malformed,
    /// The layout may be valid ASAP2 but is not modelled by this planner.
    Unsupported,
    /// The byte layout depends on a rule that is not verified (padding).
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AxisError {
    #[error("element `{keyword}` is incomplete: missing {missing}")]
    IncompleteElement {
        keyword: String,
        missing: &'static str,
    },
    #[error("element `{keyword}` has invalid position {position} (positions start at 1)")]
    InvalidPosition { keyword: String, position: u32 },
    #[error("position {position} is used by both `{first}` and `{second}`")]
    DuplicatePosition {
        position: u32,
        first: String,
        second: String,
    },
    #[error("position gap: expected {expected}, found {found}")]
    PositionGap { expected: u32, found: u32 },
    #[error("element `{keyword}` appears more than once")]
    DuplicateElement { keyword: String },
    #[error("element `{keyword}` is not supported by the record planner")]
    UnsupportedElement { keyword: String },
    #[error("element `{keyword}`: unsupported data type `{data_type}`")]
    UnsupportedDataType { keyword: String, data_type: String },
    #[error("element `{keyword}`: unsupported address type `{address_type}` (only DIRECT)")]
    UnsupportedAddressType {
        keyword: String,
        address_type: String,
    },
    #[error("element `{keyword}` has {found} arguments; expected exactly {expected}")]
    InvalidArity {
        keyword: String,
        expected: usize,
        found: usize,
    },
    #[error("element `{keyword}` has unsupported index mode `{mode}`")]
    UnsupportedIndexMode { keyword: String, mode: String },
    #[error("MOD_COMMON context is unknown; use from_layout_with_mod_common")]
    UnknownModCommonContext,
    #[error("invalid `{keyword}`: {reason}")]
    InvalidAlignment {
        keyword: String,
        reason: &'static str,
    },
    #[error(
        "ambiguous padding: `{keyword}` at offset {offset} is not a multiple of its alignment \
         {required_alignment}; the padding rule is not verified (aligned offset would be \
         {padded_offset:?})"
    )]
    AmbiguousPadding {
        keyword: String,
        offset: u64,
        required_alignment: u64,
        padded_offset: Option<u64>,
    },
    #[error(
        "axis {axis:?} point count is given by both FIX_NO_AXIS_PTS and the caller with different values"
    )]
    ConflictingCounts { axis: RecordAxis },
    #[error("axis {axis:?} has both NO_AXIS_PTS and FIX_NO_AXIS_PTS declarations")]
    ConflictingAxisCountDeclarations { axis: RecordAxis },
    #[error("{what} value {value} exceeds the conservative limit {maximum}")]
    LimitExceeded {
        what: &'static str,
        value: u64,
        maximum: u64,
    },
    #[error("missing point count for `{keyword}`")]
    MissingCount { keyword: String },
    #[error("arithmetic overflow in {operation}")]
    Overflow { operation: &'static str },
    #[error("value {value} does not fit in usize")]
    DoesNotFitUsize { value: u64 },
    #[error("internal record-plan invariant violated: {detail}")]
    InvariantViolation { detail: &'static str },
    #[error("record address {address:#x} is not a multiple of the required alignment {required}")]
    AddressMisaligned { address: u64, required: u64 },
}

impl AxisError {
    pub fn verdict(&self) -> LayoutVerdict {
        match self {
            Self::UnsupportedElement { .. }
            | Self::UnsupportedDataType { .. }
            | Self::UnsupportedAddressType { .. }
            | Self::UnsupportedIndexMode { .. } => LayoutVerdict::Unsupported,
            Self::UnknownModCommonContext => LayoutVerdict::Ambiguous,
            Self::AmbiguousPadding { .. }
            | Self::ConflictingCounts { .. }
            | Self::ConflictingAxisCountDeclarations { .. }
            | Self::PositionGap { .. }
            | Self::MissingCount { .. } => LayoutVerdict::Ambiguous,
            _ => LayoutVerdict::Malformed,
        }
    }
}

/// Checked addition.
pub fn checked_add(a: u64, b: u64) -> Result<u64, AxisError> {
    a.checked_add(b).ok_or(AxisError::Overflow {
        operation: "addition",
    })
}

/// Checked multiplication.
pub fn checked_mul(a: u64, b: u64) -> Result<u64, AxisError> {
    a.checked_mul(b).ok_or(AxisError::Overflow {
        operation: "multiplication",
    })
}

/// Smallest multiple of `alignment` that is `>= value`.
/// `alignment` must be a non-zero power of two.
pub fn align_up(value: u64, alignment: u64) -> Result<u64, AxisError> {
    if !alignment.is_power_of_two() {
        return Err(AxisError::InvalidAlignment {
            keyword: "alignment".into(),
            reason: "must be a non-zero power of two",
        });
    }
    let mask = alignment - 1;
    Ok(checked_add(value, mask)? & !mask)
}

/// Checked `u64` → `usize` conversion.
pub fn to_usize(value: u64) -> Result<usize, AxisError> {
    usize::try_from(value).map_err(|_| AxisError::DoesNotFitUsize { value })
}

const MAX_POINT_COUNT: u64 = 1 << 32;
const MAX_RECORD_SIZE: u64 = 1 << 40;

struct Candidate<'a> {
    element: &'a RecordLayoutElement,
    position: u32,
}

impl RecordPlan {
    /// Compatibility entry point without module context.
    ///
    /// A reliable plan cannot be built without knowing whether the module has
    /// `MOD_COMMON` alignment directives, so this method always returns an
    /// ambiguity error. Use [`Self::from_layout_with_mod_common`] instead.
    pub fn from_layout(_layout: &RecordLayout) -> Result<Self, AxisError> {
        Err(AxisError::UnknownModCommonContext)
    }

    /// Builds a plan with the module-level alignment context. Callers that
    /// have a `MOD_COMMON` must use this entry point; the context is never
    /// silently discarded.
    pub fn from_layout_with_mod_common(
        layout: &RecordLayout,
        mod_common: Option<&ModCommon>,
    ) -> Result<Self, AxisError> {
        let mut fixed_axis_points = [None; 5];
        let mut candidates: Vec<Candidate> = vec![];
        for e in &layout.elements {
            if let Some(axis) = e
                .keyword
                .strip_prefix("FIX_NO_AXIS_PTS_")
                .and_then(RecordAxis::from_suffix)
            {
                let n = e.fixed_axis_points.ok_or(AxisError::IncompleteElement {
                    keyword: e.keyword.clone(),
                    missing: "number of axis points",
                })?;
                if fixed_axis_points[axis.index()].replace(n).is_some() {
                    return Err(AxisError::DuplicateElement {
                        keyword: e.keyword.clone(),
                    });
                }
                continue;
            }
            let position = e.position.ok_or(AxisError::IncompleteElement {
                keyword: e.keyword.clone(),
                missing: "position",
            })?;
            if position == 0 {
                return Err(AxisError::InvalidPosition {
                    keyword: e.keyword.clone(),
                    position,
                });
            }
            candidates.push(Candidate {
                element: e,
                position,
            });
        }
        candidates.sort_by_key(|c| c.position);
        let mut expected = 1u32;
        for (i, c) in candidates.iter().enumerate() {
            if i > 0 && candidates[i - 1].position == c.position {
                return Err(AxisError::DuplicatePosition {
                    position: c.position,
                    first: candidates[i - 1].element.keyword.clone(),
                    second: c.element.keyword.clone(),
                });
            }
            if c.position != expected {
                return Err(AxisError::PositionGap {
                    expected,
                    found: c.position,
                });
            }
            expected = expected.checked_add(1).ok_or(AxisError::Overflow {
                operation: "position",
            })?;
        }
        // Keywords of items we could not place: anything in `extra` that is
        // neither a modelled element nor an ALIGNMENT_* directive.
        for item in &layout.extra {
            let known = item.keyword.starts_with("ALIGNMENT_")
                || layout.elements.iter().any(|e| e.keyword == item.keyword);
            if !known {
                return Err(AxisError::UnsupportedElement {
                    keyword: item.keyword.clone(),
                });
            }
        }
        let alignments = declared_alignments(layout, mod_common)?;
        for axis in [
            RecordAxis::X,
            RecordAxis::Y,
            RecordAxis::Z,
            RecordAxis::Fourth,
            RecordAxis::Fifth,
        ] {
            let has_fixed = fixed_axis_points[axis.index()].is_some();
            let has_no_axis = candidates
                .iter()
                .any(|c| c.element.keyword == format!("NO_AXIS_PTS_{}", axis_suffix(axis)));
            if has_fixed && has_no_axis {
                return Err(AxisError::ConflictingAxisCountDeclarations { axis });
            }
        }
        let mut elements = Vec::with_capacity(candidates.len());
        for c in &candidates {
            elements.push(plan_element(c, &alignments)?);
        }
        for (i, e) in elements.iter().enumerate() {
            if elements[..i].iter().any(|p| p.kind == e.kind) {
                return Err(AxisError::DuplicateElement {
                    keyword: e.keyword.clone(),
                });
            }
        }
        let required_base_alignment = elements
            .iter()
            .map(|e| e.required_alignment)
            .max()
            .unwrap_or(1);
        let mut plan = Self {
            layout_name: layout.name.clone(),
            elements,
            required_base_alignment,
            fixed_axis_points,
        };
        let (offsets, _) = plan.walk(&PointCounts::new(), false)?;
        for (e, p) in plan.elements.iter_mut().zip(offsets) {
            e.static_offset = p.map(|p| p.0);
        }
        Ok(plan)
    }

    pub fn element(&self, kind: ElementKind) -> Option<&PlannedElement> {
        self.elements.iter().find(|e| e.kind == kind)
    }

    pub fn elements(&self) -> &[PlannedElement] {
        &self.elements
    }

    pub fn required_base_alignment(&self) -> u64 {
        self.required_base_alignment
    }

    /// `FIX_NO_AXIS_PTS_<axis>` value, when declared.
    pub fn fixed_axis_points(&self, axis: RecordAxis) -> Option<u32> {
        self.fixed_axis_points[axis.index()]
    }

    /// Checks that a record start address satisfies the strictest alignment.
    pub fn check_base_address(&self, address: u64) -> Result<(), AxisError> {
        if address.is_multiple_of(self.required_base_alignment) {
            Ok(())
        } else {
            Err(AxisError::AddressMisaligned {
                address,
                required: self.required_base_alignment,
            })
        }
    }

    /// Final placement of every element. Every count-dependent element needs a
    /// count in `counts` (or a `FIX_NO_AXIS_PTS_*` declaration).
    pub fn resolve(&self, counts: &PointCounts) -> Result<ResolvedRecord, AxisError> {
        let (offsets, end) = self.walk(counts, true)?;
        let mut placements = Vec::with_capacity(self.elements.len());
        for (e, p) in self.elements.iter().zip(offsets) {
            let Some((offset, Some(size))) = p else {
                return Err(AxisError::InvariantViolation {
                    detail: "strict resolution produced an incomplete placement",
                });
            };
            placements.push(Placement {
                kind: e.kind,
                offset,
                size,
            });
        }
        let Some(total_size) = end else {
            return Err(AxisError::InvariantViolation {
                detail: "strict resolution produced no total size",
            });
        };
        Ok(ResolvedRecord {
            placements,
            total_size,
        })
    }

    /// Resolves the statically known prefix of a record. This is used to read
    /// a later `NO_AXIS_PTS_*` counter before the count-dependent elements
    /// that follow it are known.
    pub fn resolve_prefix(&self, counts: &PointCounts) -> Result<Vec<Placement>, AxisError> {
        let (offsets, _) = self.walk(counts, false)?;
        let mut placements = Vec::new();
        for (element, placement) in self.elements.iter().zip(offsets) {
            let Some((offset, Some(size))) = placement else {
                break;
            };
            placements.push(Placement {
                kind: element.kind,
                offset,
                size,
            });
        }
        Ok(placements)
    }

    fn count_for(
        &self,
        source: CountSource,
        counts: &PointCounts,
    ) -> Result<Option<u64>, AxisError> {
        let checked = |count: Option<u64>, what| {
            count.map_or(Ok(None), |value| {
                if value > MAX_POINT_COUNT {
                    Err(AxisError::LimitExceeded {
                        what,
                        value,
                        maximum: MAX_POINT_COUNT,
                    })
                } else {
                    Ok(Some(value))
                }
            })
        };
        match source {
            CountSource::FncValues => checked(counts.fnc_values, "FNC_VALUES count"),
            CountSource::Axis(axis) => {
                let fixed = self.fixed_axis_points[axis.index()].map(u64::from);
                match (fixed, counts.axes[axis.index()]) {
                    (Some(f), Some(c)) if f != c => Err(AxisError::ConflictingCounts { axis }),
                    (Some(f), _) => checked(Some(f), "axis point count"),
                    (None, c) => checked(c, "axis point count"),
                }
            }
        }
    }

    /// Walks the record. Returns per element `(offset, size)` (offset `Some`
    /// while every earlier size is known) and the end offset. With `strict`,
    /// a missing count is an error instead of stopping the walk.
    #[allow(clippy::type_complexity)]
    fn walk(
        &self,
        counts: &PointCounts,
        strict: bool,
    ) -> Result<(Vec<Option<(u64, Option<u64>)>>, Option<u64>), AxisError> {
        let mut cursor = Some(0u64);
        let mut out = Vec::with_capacity(self.elements.len());
        for e in &self.elements {
            let Some(offset) = cursor else {
                if strict {
                    return Err(AxisError::InvariantViolation {
                        detail: "strict walk lost its cursor",
                    });
                }
                out.push(None);
                continue;
            };
            if !offset.is_multiple_of(e.required_alignment) {
                return Err(AxisError::AmbiguousPadding {
                    keyword: e.keyword.clone(),
                    offset,
                    required_alignment: e.required_alignment,
                    padded_offset: align_up(offset, e.required_alignment).ok(),
                });
            }
            let size = match e.extent {
                Extent::Scalar => Some(e.data_type.size()),
                Extent::PerPoint(source) => match self.count_for(source, counts)? {
                    Some(n) => Some(checked_mul(n, e.data_type.size())?),
                    None if strict => {
                        return Err(AxisError::MissingCount {
                            keyword: e.keyword.clone(),
                        });
                    }
                    None => None,
                },
            };
            cursor = match size {
                Some(s) => {
                    let end = checked_add(offset, s)?;
                    if end > MAX_RECORD_SIZE {
                        return Err(AxisError::LimitExceeded {
                            what: "record size",
                            value: end,
                            maximum: MAX_RECORD_SIZE,
                        });
                    }
                    Some(end)
                }
                None => None,
            };
            out.push(Some((offset, size)));
        }
        Ok((out, cursor))
    }
}

/// `ALIGNMENT_*` directives: one power-of-two value each, no duplicates.
fn declared_alignments(
    layout: &RecordLayout,
    mod_common: Option<&ModCommon>,
) -> Result<Vec<(String, u64)>, AxisError> {
    let mut out: Vec<(String, u64)> = vec![];
    let layout_items = layout
        .extra
        .iter()
        .filter(|i| i.keyword.starts_with("ALIGNMENT_"));
    let common_items = mod_common
        .into_iter()
        .flat_map(|m| m.extra.iter())
        .filter(|i| i.keyword.starts_with("ALIGNMENT_"));
    for item in layout_items.chain(common_items) {
        let known = [
            "ALIGNMENT_BYTE",
            "ALIGNMENT_WORD",
            "ALIGNMENT_LONG",
            "ALIGNMENT_INT64",
            "ALIGNMENT_FLOAT16_IEEE",
            "ALIGNMENT_FLOAT32_IEEE",
            "ALIGNMENT_FLOAT64_IEEE",
        ];
        if !known.contains(&item.keyword.as_str()) {
            return Err(AxisError::InvalidAlignment {
                keyword: item.keyword.clone(),
                reason: "unknown alignment directive",
            });
        }
        if out.iter().any(|(k, _)| *k == item.keyword) {
            return Err(AxisError::InvalidAlignment {
                keyword: item.keyword.clone(),
                reason: "duplicate directive",
            });
        }
        let value = match item.values.as_slice() {
            [v] => v.parse::<u64>().ok().filter(|v| v.is_power_of_two()),
            _ => None,
        }
        .ok_or(AxisError::InvalidAlignment {
            keyword: item.keyword.clone(),
            reason: "needs exactly one power-of-two byte count",
        })?;
        out.push((item.keyword.clone(), value));
    }
    Ok(out)
}

fn axis_suffix(axis: RecordAxis) -> &'static str {
    match axis {
        RecordAxis::X => "X",
        RecordAxis::Y => "Y",
        RecordAxis::Z => "Z",
        RecordAxis::Fourth => "4",
        RecordAxis::Fifth => "5",
    }
}

fn classify(keyword: &str) -> Option<(ElementKind, Extent)> {
    if keyword == "FNC_VALUES" {
        return Some((
            ElementKind::FncValues,
            Extent::PerPoint(CountSource::FncValues),
        ));
    }
    if keyword == "IDENTIFICATION" {
        return Some((ElementKind::Identification, Extent::Scalar));
    }
    if let Some(axis) = keyword
        .strip_prefix("AXIS_PTS_")
        .and_then(RecordAxis::from_suffix)
    {
        return Some((
            ElementKind::AxisPts(axis),
            Extent::PerPoint(CountSource::Axis(axis)),
        ));
    }
    keyword
        .strip_prefix("NO_AXIS_PTS_")
        .and_then(RecordAxis::from_suffix)
        .map(|axis| (ElementKind::NoAxisPts(axis), Extent::Scalar))
}

fn plan_element(c: &Candidate, alignments: &[(String, u64)]) -> Result<PlannedElement, AxisError> {
    let e = c.element;
    let missing = |missing: &'static str| AxisError::IncompleteElement {
        keyword: e.keyword.clone(),
        missing,
    };
    let (kind, extent) = classify(&e.keyword).ok_or_else(|| AxisError::UnsupportedElement {
        keyword: e.keyword.clone(),
    })?;
    let type_text = e.data_type.as_deref().ok_or_else(|| missing("data type"))?;
    let data_type =
        RecordDataType::parse(type_text).ok_or_else(|| AxisError::UnsupportedDataType {
            keyword: e.keyword.clone(),
            data_type: type_text.to_string(),
        })?;
    let expected = match kind {
        ElementKind::FncValues | ElementKind::AxisPts(_) => 4,
        ElementKind::Identification | ElementKind::NoAxisPts(_) => 2,
    };
    if e.arguments.len() > expected {
        return Err(AxisError::InvalidArity {
            keyword: e.keyword.clone(),
            expected,
            found: e.arguments.len(),
        });
    }
    match kind {
        ElementKind::FncValues => {
            let mode = e
                .index_mode
                .as_deref()
                .ok_or_else(|| missing("index mode"))?;
            if !matches!(mode, "ROW_DIR" | "COLUMN_DIR") {
                return Err(AxisError::UnsupportedIndexMode {
                    keyword: e.keyword.clone(),
                    mode: mode.to_owned(),
                });
            }
        }
        ElementKind::AxisPts(_) => {
            let increment = e
                .index_increment
                .as_deref()
                .ok_or_else(|| missing("index increment"))?;
            if increment != "INDEX_INCR" {
                return Err(AxisError::UnsupportedIndexMode {
                    keyword: e.keyword.clone(),
                    mode: increment.to_owned(),
                });
            }
        }
        _ => {}
    }
    if matches!(kind, ElementKind::FncValues | ElementKind::AxisPts(_)) {
        let address_type = e
            .address_type
            .as_deref()
            .ok_or_else(|| missing("address type"))?;
        if address_type != "DIRECT" {
            return Err(AxisError::UnsupportedAddressType {
                keyword: e.keyword.clone(),
                address_type: address_type.to_string(),
            });
        }
    }
    let declared = alignments
        .iter()
        .find(|(k, _)| k == data_type.alignment_keyword())
        .map(|(_, v)| *v);
    Ok(PlannedElement {
        keyword: e.keyword.clone(),
        kind,
        position: c.position,
        data_type,
        extent,
        index_mode: e.index_mode.clone(),
        index_increment: e.index_increment.clone(),
        required_alignment: declared.unwrap_or_else(|| data_type.size()),
        alignment_declared: declared.is_some(),
        static_offset: None,
        static_size: matches!(extent, Extent::Scalar).then(|| data_type.size()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_up_rounds_and_rejects_bad_alignment() {
        assert_eq!(align_up(0, 4).unwrap(), 0);
        assert_eq!(align_up(1, 4).unwrap(), 4);
        assert_eq!(align_up(4, 4).unwrap(), 4);
        assert_eq!(align_up(5, 1).unwrap(), 5);
        assert!(align_up(1, 0).is_err());
        assert!(align_up(1, 3).is_err());
    }

    #[test]
    fn align_up_overflow_is_an_error() {
        assert_eq!(
            align_up(u64::MAX, 2),
            Err(AxisError::Overflow {
                operation: "addition"
            })
        );
        assert_eq!(align_up(u64::MAX, 1).unwrap(), u64::MAX);
    }

    #[test]
    fn checked_arithmetic_reports_overflow() {
        assert!(checked_add(u64::MAX, 1).is_err());
        assert!(checked_mul(u64::MAX, 2).is_err());
        assert_eq!(checked_mul(3, 4).unwrap(), 12);
    }

    #[test]
    fn to_usize_is_checked() {
        assert_eq!(to_usize(7).unwrap(), 7usize);
        if usize::BITS < 64 {
            assert!(to_usize(u64::MAX).is_err());
        }
    }

    #[test]
    fn fnc_items_product_is_checked() {
        assert_eq!(PointCounts::fnc_items_from_dimensions(&[3, 4]).unwrap(), 12);
        assert!(PointCounts::fnc_items_from_dimensions(&[u64::MAX, 2]).is_err());
        assert!(PointCounts::fnc_items_from_dimensions(&[]).is_err());
    }
}
