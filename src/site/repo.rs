use crate::error::Result;
use anyhow::Context;
use glob::Pattern;
use rocket::http::uri::Origin;
use rocket::http::Status;
use rocket::outcome::Outcome;
use rocket::request::{FromRequest, Request};
use rocket::{uri, State};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Serialize, Clone)]
pub struct Repo {
    pub name: String,
    pub path: PathBuf,
    pub href: Origin<'static>,
    pub description: Option<String>,
    pub default_branch: String,
    pub readme_path: Option<PathBuf>,

    #[serde(skip)]
    pub lang_override: Vec<(Pattern, String)>,
}

#[derive(Serialize)]
pub struct File {
    pub name: String,
    pub path: PathBuf,
    pub href: Origin<'static>,
    pub mode: FileMode,
}

#[derive(Serialize, Clone, Copy)]
pub enum FileMode {
    #[serde(rename = "file")]
    File,
    #[serde(rename = "dir")]
    Dir,
    #[serde(rename = "exe")]
    Exe,
}

impl FileMode {
    pub fn from_mode(mode: i32) -> Option<FileMode> {
        match mode {
            0o100_644 => Some(FileMode::File),
            0o100_755 => Some(FileMode::Exe),
            0o040_000 => Some(FileMode::Dir),
            _ => {
                log::warn!("unknown file mode {:#o}", mode);
                None
            }
        }
    }
}

mod config {
    use serde::Deserialize;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[derive(Deserialize)]
    pub struct Config {
        /// Repository metadata
        pub repo: Repo,

        #[serde(default)]
        pub access: Access,

        /// Associations of `(language-code, glob-pattern)` which override the default javascript
        /// language detection
        #[serde(default)]
        pub lang_override: HashMap<String, String>,
    }

    #[derive(Deserialize)]
    pub struct Repo {
        pub default_branch: String,
        pub description: Option<String>,
        pub readme: Option<PathBuf>,
    }

    #[derive(Deserialize, Default)]
    pub struct Access {
    }
}

impl Repo {
    pub async fn open(repo_path: &Path, repo_name: &str) -> Result<Option<Repo>> {
        let config_path = repo_path.join("site.toml");
        if !config_path.exists() {
            return Ok(None);
        }
        let config = {
            let data = fs::read(&config_path).await
                .with_context(|| format!("reading repo config {:?}", config_path))?;

            toml::de::from_slice::<config::Config>(&data)
                .with_context(|| format!("parsing repo config {:?}", config_path))?
        };

        let lang_override = config.lang_override
            .into_iter()
            .filter_map(|(k, v)| match Pattern::new(&v) {
                Ok(patt) => Some((patt, k)),
                Err(err) => {
                    log::warn!("ignoring invalid language pattern {:?}: {}", k, err);
                    None
                }
            })
            .collect();

        // we allow both absolute and relative paths,
        // both are interpreted relatively to the repository root
        let readme_path = config.repo.readme.map(|path| {
            if path.starts_with("/") {
                path.strip_prefix("/")
                    .unwrap()
                    .to_owned()
            } else {
                path
            }
        });

        Ok(Some(Repo {
            name: repo_name.to_string(),
            path: repo_path.to_owned(),
            href: uri!(crate::web::home(&repo_name)),
            description: config.repo.description,
            default_branch: config.repo.default_branch,
            lang_override,
            readme_path,
        }))
    }
}

#[rocket::async_trait]
impl<'req> FromRequest<'req> for Repo {
    type Error = crate::error::Error;

    async fn from_request(request: &'req Request<'_>) -> rocket::request::Outcome<Self, Self::Error> {
        let config = request.guard::<&State<crate::Config>>().await.unwrap();

        let repo_name = match request.param::<&str>(0) {
            Some(Ok(repo_name)) => repo_name,
            Some(Err(_)) => unreachable!(), // parsing &str never fails
            None => return Outcome::Forward(()),
        };

        let repo_path = config.git_root.join(&repo_name);

        let res = Repo::open(&repo_path, &repo_name).await;
        match res {
            Ok(Some(repo)) => Outcome::Success(repo),
            Ok(None) => Outcome::Forward(()),
            Err(err) => Outcome::Failure((Status::InternalServerError, err.into())),
        }
    }
}
