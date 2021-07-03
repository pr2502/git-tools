use rocket::http::Status;
use rocket::request::Request;
use rocket::response::Responder;
use std::fmt;

pub struct Error(anyhow::Error);
pub type Result<T> = std::result::Result<T, Error>;

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Error {
        Error(e)
    }
}

impl<'r> Responder<'r, 'static> for Error {
    fn respond_to(self, _: &'r Request<'_>) -> rocket::response::Result<'static> {
        let message = format!("{:?}", self.0);
        for line in message.lines() {
            log::error!("{}", line);
        }
        Err(Status::InternalServerError)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}
