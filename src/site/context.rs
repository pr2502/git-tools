use crate::repo::Repo;
use rocket::request::{self, FromRequest, Request};
use rocket::State;

pub mod config {
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    #[derive(Debug, Deserialize, Serialize)]
    pub struct Config {
        pub git_root: PathBuf,
    }

    impl Default for Config {
        fn default() -> Config {
            Config {
                git_root: PathBuf::from("/home/git"),
            }
        }
    }
}


pub struct Context<'req> {
    config: &'req config::Config,
}

#[rocket::async_trait]
impl<'req> FromRequest<'req> for Context<'req> {
    type Error = ();

    async fn from_request(request: &'req Request<'_>) -> request::Outcome<Self, Self::Error> {
        request.guard::<&State<config::Config>>().await
            .map(|config| Context { config })
    }
}

// impl<'req> Context<'req> {
//     pub async fn list_repos(&self) -> Result<Vec<Repo>> {
//         let git_root = &self.config.git_root;

//         let read_dir = fs::read_dir(git_root).await
//             .with_context(|| format!("reading directory git_root={:?}", git_root))?;

//         let repos = ReadDirStream::new(read_dir)
//             .map(|res| Ok(res.context("reading direntry")?))
//             .try_filter_map(async move |entry| {
//                 let metadata = entry.metadata().await.context("reading direntry")?;
//                 Ok(metadata.is_dir().then(|| entry))
//             })
//             .try_filter_map(async move |entry| -> Result<_> {
//                 let path = entry.path();
//                 let name = PathBuf::from(entry.file_name());
//                 match Repo::open(&path, &name).await {
//                     Ok(repo) => Ok(repo),
//                     Err(err) => {
//                         let err = Result::<!>::Err(err)
//                             .with_context(|| format!("reading repo {:?}", &path));
//                         log::error!("{:?}", err.unwrap_err());
//                         Ok(None)
//                     }
//                 }
//             })
//             .try_collect::<Vec<_>>().await?;
//     }
// }
