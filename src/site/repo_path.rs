use rocket::http::uri::fmt::FromUriParam;
use rocket::request::FromSegments;
use std::fmt;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub struct RepoPath {
    path: PathBuf,
}

#[derive(Error, Debug)]
pub enum RepoPathError {
    #[error("segment started with an invalid character {0:?}")]
    BadStart(char),

    #[error("segment contains an invalid character {0:?}")]
    BadChar(char),

    #[error("forbidden segment {0:?}")]
    BadSegment(&'static str),

    #[error("segment ended with an invalid character {0:?}")]
    BadEnd(char),
}

impl<'uri> FromSegments<'uri> for RepoPath {
    type Error = RepoPathError;

    fn from_segments(segments: rocket::http::uri::Segments<'uri, rocket::http::uri::fmt::Path>) -> Result<Self, Self::Error> {
        // Parsing is adopted from `rocket_http::uri::Segments::to_path_buf` for using to address
        // repo paths.
        //
        // The difference is we allow dotfiles and forbid `..` instead of interpreting it.
        let mut buf = PathBuf::new();

        for segment in segments {
            if segment == ".." {
                return Err(RepoPathError::BadSegment(".."));
            } else if segment.starts_with('*') {
                return Err(RepoPathError::BadStart('*'))
            } else if segment.ends_with(':') {
                return Err(RepoPathError::BadEnd(':'))
            } else if segment.ends_with('>') {
                return Err(RepoPathError::BadEnd('>'))
            } else if segment.ends_with('<') {
                return Err(RepoPathError::BadEnd('<'))
            } else if segment.contains('/') {
                return Err(RepoPathError::BadChar('/'))
            } else {
                buf.push(&*segment);
            }
        }

        Ok(RepoPath { path: buf })
    }
}

impl<'a> FromUriParam<rocket::http::uri::fmt::Path, &'a Path> for RepoPath {
    type Target = &'a Path;

    fn from_uri_param(param: &'a Path) -> &'a Path {
        param
    }
}

impl Deref for RepoPath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path.deref()
    }
}

impl fmt::Debug for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.path.fmt(f)
    }
}
