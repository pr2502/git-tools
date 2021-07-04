use crate::error::Result;
use crate::repo::Repo;
use anyhow::Context as _;
use futures::stream::{StreamExt, TryStreamExt};
use rocket::http::Status;
use rocket::outcome::Outcome;
use rocket::request::FromRequest;
use rocket::{Request, State};
use std::path::PathBuf;
use tokio::fs;
use tokio_stream::wrappers::ReadDirStream;


pub struct Index {
    pub repos: Vec<Repo>,
}

#[rocket::async_trait]
impl<'req> FromRequest<'req> for Index {
    type Error = crate::error::Error;

    async fn from_request(request: &'req Request<'_>) -> rocket::request::Outcome<Self, Self::Error> {
        let config = request.guard::<&State<crate::Config>>().await.unwrap();

        let res = fs::read_dir(&config.git_root).await
            .with_context(|| format!("reading directory git_root={:?}", &config.git_root));
        let read_dir = match res {
            Ok(read_dir) => read_dir,
            Err(err) => return Outcome::Failure((Status::InternalServerError, err.into())),
        };

        let res = ReadDirStream::new(read_dir)
            .map(|res| Ok(res.context("reading direntry")?))
            .try_filter_map(async move |entry| {
                let metadata = entry.metadata().await
                    .context("reading direntry")?;
                Ok(metadata.is_dir().then(|| entry))
            })
            .try_filter_map(async move |entry| -> Result<_> {
                let path = entry.path();
                let name = PathBuf::from(entry.file_name());
                let res = Repo::open(&path, &name).await;
                match res {
                    Ok(repo) => Ok(repo),
                    Err(err) => {
                        let err = Result::<!>::Err(err)
                            .with_context(|| format!("reading repo {:?}", &path));
                        log::warn!("{:?}", err.unwrap_err());
                        Ok(None)
                    }
                }
            })
            .try_collect::<Vec<_>>().await;

        let repos = match res {
            Ok(repos) => repos,
            Err(err) => return Outcome::Failure((Status::InternalServerError, err.into())),
        };

        Outcome::Success(Index { repos })
    }
}
