use rocket::catch;
use rocket::http::Status;
use rocket::request::Request;
use rocket::response::Responder;
use rocket_dyn_templates::Template;
use std::fmt;

/// Error wrapper that implements [`Responder`]
pub struct Error(anyhow::Error);

/// Error details that can be forwarded to the catcher
struct ErrorDetails(String);

pub type Result<T> = std::result::Result<T, Error>;

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Error {
        Error(e)
    }
}

impl<'req> Responder<'req, 'static> for Error {
    fn respond_to(self, request: &'req Request<'_>) -> rocket::response::Result<'static> {
        let message = format!("{:?}", self.0);
        for line in message.lines() {
            log::error!("{}", line);
        }
        // save the message into request-local cache
        request.local_cache(move || Some(ErrorDetails(message)));
        Err(Status::InternalServerError)
    }
}

#[catch(default)]
pub fn default_catcher(status: Status, request: &Request) -> Template {
    // if the caught error was caused by our `Error` type we have the message stored in cache
    let details = request.local_cache(|| Option::<ErrorDetails>::None)
        .as_ref()
        .map(|details| details.0.to_owned());

    Template::render("error", ctx!{
        code = status.code,
        details,
        reason = status.reason().unwrap_or("Unknown"),
        view = "error",
    })
}


// transparent Error wrapper impls

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
