
// use crate::url;
use std::error;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::io;
// use url;

#[derive(Debug)]
pub enum Error {

    UrlParseError(url::ParseError),
    Unexpected,
    IOError(io::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {

        match *self {
            Error::UrlParseError(ref e) => write!(f, "UrlParseError:  {}", e),
            Error::Unexpected => write!(f, "UnexpectedError"),
            Error::IOError(ref e) => write!(f, "InputOutputError: {}", e),
        }
    }
}

impl From<url::ParseError> for Error {
    fn from(err: url::ParseError) -> Error {
        Error::UrlParseError(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IOError(err)
    }
}

// #[cfg(feature = "reqwest")]


impl error::Error for Error {}
