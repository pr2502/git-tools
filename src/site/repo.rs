use crate::error::Result;
use anyhow::Context;
use rocket::http::uri::Origin;
use rocket::uri;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Serialize)]
pub struct Repo {
    pub name: String,
    pub path: PathBuf,
    pub href: Origin<'static>,
    pub description: Option<String>,
    pub default_branch: String,
}

#[derive(Serialize)]
pub struct File {
    pub name: String,
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

    #[derive(Deserialize)]
    pub struct RepoConfig {
        pub repo: Repo,

        #[serde(default)]
        pub access: Access,
    }

    #[derive(Deserialize)]
    pub struct Repo {
        pub default_branch: String,
        pub description: Option<String>,
    }

    #[derive(Deserialize, Default)]
    pub struct Access {

    }
}

impl Repo {
    pub async fn open(repo_path: &Path, repo_name: &Path) -> Result<Repo> {
        let config_path = repo_path.join("site.toml");
        let repo_config = {
            let data = fs::read(&config_path).await
                .with_context(|| format!("reading repo config {:?}", config_path))?;
            toml::de::from_slice::<config::RepoConfig>(&data)
                .with_context(|| format!("parsing repo config {:?}", config_path))?
        };

        Ok(Repo {
            name: repo_name.to_str().unwrap().to_owned(),
            path: repo_path.to_owned(),
            href: uri!(crate::web::home(repo_name)),
            description: repo_config.repo.description,
            default_branch: repo_config.repo.default_branch,
        })
    }

}
