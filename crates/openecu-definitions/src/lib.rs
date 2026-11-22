//! Versioned JSON `.oedef` definitions.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;
pub mod a2l;
pub use a2l::{
    A2lDocument, A2lImportError, Diagnostic, RawBlock, RawBlockId, RawElement, RawItem, Severity,
    SourceLocation, parse_a2l, parse_a2l_file,
};
pub const OEDF_V1: &str = "1.0";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinitionEcu {
    pub manufacturer: String,
    pub family: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolDefinition {
    pub name: String,
    pub address: u64,
    pub data_type: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinitionFile {
    pub format_version: String,
    pub ecu: DefinitionEcu,
    pub symbols: Vec<SymbolDefinition>,
}
#[derive(Debug, Error)]
pub enum DefinitionError {
    #[error("invalid JSON definition: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported .oedef version `{found}`")]
    UnsupportedVersion { found: String },
    #[error("{field} must not be empty")]
    EmptyField { field: &'static str },
    #[error("symbol `{name}` is duplicated")]
    DuplicateSymbol { name: String },
}
impl DefinitionFile {
    pub fn from_json(json: &str) -> Result<Self, DefinitionError> {
        let x: Self = serde_json::from_str(json)?;
        x.validate()?;
        Ok(x)
    }
    pub fn validate(&self) -> Result<(), DefinitionError> {
        if self.format_version != OEDF_V1 {
            return Err(DefinitionError::UnsupportedVersion {
                found: self.format_version.clone(),
            });
        }
        for (v, f) in [
            (&self.ecu.manufacturer, "ecu.manufacturer"),
            (&self.ecu.family, "ecu.family"),
        ] {
            if v.trim().is_empty() {
                return Err(DefinitionError::EmptyField { field: f });
            }
        }
        let mut n = HashSet::new();
        for s in &self.symbols {
            if s.name.trim().is_empty() {
                return Err(DefinitionError::EmptyField {
                    field: "symbol.name",
                });
            }
            if s.data_type.trim().is_empty() {
                return Err(DefinitionError::EmptyField {
                    field: "symbol.data_type",
                });
            }
            if !n.insert(&s.name) {
                return Err(DefinitionError::DuplicateSymbol {
                    name: s.name.clone(),
                });
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_an_unsupported_format_version() {
        assert!(
            DefinitionFile::from_json(
                r#"{"format_version":"2.0","ecu":{"manufacturer":"A","family":"X"},"symbols":[]}"#
            )
            .is_err()
        )
    }
    #[test]
    fn rejects_duplicate_symbol_names() {
        let x = DefinitionFile {
            format_version: OEDF_V1.into(),
            ecu: DefinitionEcu {
                manufacturer: "A".into(),
                family: "X".into(),
            },
            symbols: vec![
                SymbolDefinition {
                    name: "a".into(),
                    address: 1,
                    data_type: "u16".into(),
                },
                SymbolDefinition {
                    name: "a".into(),
                    address: 2,
                    data_type: "u16".into(),
                },
            ],
        };
        assert!(x.validate().is_err())
    }
    #[test]
    fn parses_a_valid_v1_definition() {
        assert_eq!(
            DefinitionFile::from_json(
                r#"{"format_version":"1.0","ecu":{"manufacturer":"A","family":"X"},"symbols":[]}"#
            )
            .unwrap()
            .format_version,
            OEDF_V1
        )
    }
}
