//! Load/validation errors of the game data.

use std::fmt;
use std::path::PathBuf;

use thiserror::Error;

/// One problem found while loading or validating the game data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefsProblem {
    /// Data file (relative to the data directory) the problem was found in.
    pub file: String,
    /// Affected definition, e.g. `building 'sawmill'`; empty for file-level problems.
    pub item: String,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for DefsProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.item.is_empty() {
            write!(f, "{}: {}", self.file, self.message)
        } else {
            write!(f, "{}: {}: {}", self.file, self.item, self.message)
        }
    }
}

/// Error returned by [`Defs::load`](super::Defs::load).
#[derive(Debug, Error)]
pub enum DefsError {
    /// The data directory itself does not exist or is not a directory.
    #[error("game data directory not found: {}", .0.display())]
    MissingDirectory(PathBuf),
    /// One or more files could not be read/parsed, or definitions are inconsistent.
    /// All problems found are listed, not only the first one.
    #[error("{} problem(s) in game data:\n{}", .0.len(), format_problems(.0))]
    Invalid(Vec<DefsProblem>),
}

impl DefsError {
    /// All individual problems (empty for [`DefsError::MissingDirectory`]).
    pub fn problems(&self) -> &[DefsProblem] {
        match self {
            DefsError::MissingDirectory(_) => &[],
            DefsError::Invalid(problems) => problems,
        }
    }
}

fn format_problems(problems: &[DefsProblem]) -> String {
    problems.iter().map(|p| format!("  - {p}")).collect::<Vec<_>>().join("\n")
}

/// Where a definition came from; used to attribute problems.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Loc<'a> {
    /// Data file relative to the data directory.
    pub file: &'a str,
    /// Kind of definition, e.g. `building`.
    pub kind: &'static str,
    /// Canonical string id of the definition.
    pub key: &'a str,
}

impl Loc<'_> {
    fn item(&self) -> String {
        if self.key.is_empty() { self.kind.to_string() } else { format!("{} '{}'", self.kind, self.key) }
    }
}

/// Collects problems instead of failing on the first one.
#[derive(Debug, Default)]
pub(crate) struct Problems {
    list: Vec<DefsProblem>,
}

impl Problems {
    /// Records a problem of a definition.
    pub fn at(&mut self, loc: &Loc<'_>, message: impl Into<String>) {
        self.list.push(DefsProblem { file: loc.file.to_string(), item: loc.item(), message: message.into() });
    }

    /// Records a file-level problem.
    pub fn file(&mut self, file: &str, message: impl Into<String>) {
        self.list.push(DefsProblem { file: file.to_string(), item: String::new(), message: message.into() });
    }

    /// Records a problem when `ok` is false.
    pub fn check(&mut self, ok: bool, loc: &Loc<'_>, message: impl FnOnce() -> String) {
        if !ok {
            self.at(loc, message());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Converts the collection into a result.
    pub fn into_result<T>(self, value: T) -> Result<T, DefsError> {
        if self.list.is_empty() { Ok(value) } else { Err(DefsError::Invalid(self.list)) }
    }

    pub fn into_error(self) -> DefsError {
        DefsError::Invalid(self.list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problems_render_with_location() {
        let mut p = Problems::default();
        let loc = Loc { file: "buildings/buildings.ron", kind: "building", key: "sawmill" };
        p.at(&loc, "unknown resource 'wod'");
        p.file("roads.ron", "cannot parse");
        p.check(true, &loc, || unreachable!());
        let err = p.into_result(()).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("2 problem(s)"));
        assert!(text.contains("buildings/buildings.ron: building 'sawmill': unknown resource 'wod'"));
        assert!(text.contains("roads.ron: cannot parse"));
        assert_eq!(err.problems().len(), 2);
    }
}
