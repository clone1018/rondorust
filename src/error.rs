use std::{fmt, io};

/// A diagnostic with a one-based source position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

/// Parsing, validation, resource, and output errors.
#[derive(Debug)]
pub enum Error {
    Parse(Diagnostic),
    Invalid(String),
    MissingSample(String),
    Io(io::Error),
}

impl Error {
    pub(crate) fn at(line: usize, column: usize, message: impl Into<String>) -> Self {
        Self::Parse(Diagnostic {
            line,
            column,
            message: message.into(),
        })
    }
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(d) => write!(f, "{}:{}: {}", d.line, d.column, d.message),
            Self::Invalid(message) => f.write_str(message),
            Self::MissingSample(name) => write!(f, "sample `{name}` is not in the sample bank"),
            Self::Io(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Io(error) = self {
            Some(error)
        } else {
            None
        }
    }
}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
