use crate::repo::Repo;
use crate::repo_path::RepoPath;
use crate::web;
use anyhow::Context as _;
use rocket::http::uri::Origin;
use rocket::http::Status;
use rocket::outcome::Outcome;
use rocket::request::{FromRequest, Request};
use rocket::uri;
use serde::Serialize;
use std::path::{Path, PathBuf};


#[derive(Serialize)]
pub struct Nav {
    path: PathNav,
    refs: Option<RefNav>,
}

#[derive(Serialize)]
struct PathNav {
    segments: Vec<Segment>,
}

#[derive(Serialize)]
struct Segment {
    name: String,
    href: Origin<'static>,
}

#[derive(Serialize)]
struct RefNav {
    current: String,
    href: Origin<'static>,
}


#[rocket::async_trait]
impl<'req> FromRequest<'req> for Nav {
    type Error = crate::error::Error;

    async fn from_request(request: &'req Request<'_>) -> rocket::request::Outcome<Self, Self::Error> {
        // here we're assuming the path is in the form of
        // "/<repo_name>/view/<refs>/<path..>"
        //
        // this is an invariant that all the routes using Nav guard must ensure, otherwise Nav will
        // behave weirdly

        // repo will be None if request path doesn't contain the <repo_nav> segment
        let res = request.guard::<Repo>().await;
        let repo = match res {
            Outcome::Failure(err) => return Outcome::Failure(err),
            Outcome::Forward(_) => None,
            Outcome::Success(repo) => Some(repo),
        };

        let refs = request.param::<&str>(2)
            .transpose().unwrap(); // &str from &str never fails

        let res = request.segments::<RepoPath>(3..)
            .context("bad path format");
        let path = match res {
            Ok(path) => path,
            Err(err) => return Outcome::Failure((Status::BadRequest, err.into())),
        };

        let path_nav = {
            let refs = refs.unwrap_or("");

            let mut segments = repo.as_ref()
                .map(|repo| {
                    path.ancestors()
                        .map(|path| {
                            let name = path.file_name()
                                .map(|fname| fname.to_string_lossy().to_string())
                                // repository root has an empty path -> file_name() returns None
                                .unwrap_or_else(|| repo.name.clone());
                            let href = uri!(web::tree(Path::new(&repo.name), refs, path));
                            Segment { name, href }
                        })
                    .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            // `Path::ancestors` yields paths from longest to shortest and doesn't allow reversing,
            // so we reverse the order here
            segments.reverse();

            PathNav { segments }
        };

        let ref_nav = refs.map(|current| {
            // since here we know path contains <refs> it must also have contained <repo_name>
            // before it and repo is safe to unwrap
            let repo = repo.as_ref().unwrap();

            RefNav {
                current: current.to_string(),
                href: uri!(web::refs(&repo.name, current, &path)),
            }
        });

        Outcome::Success(Nav {
            path: path_nav,
            refs: ref_nav,
        })
    }
}
