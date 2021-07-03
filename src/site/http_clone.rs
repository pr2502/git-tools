use crate::Config;
use rocket::fs::NamedFile;
use rocket::{get, routes, Route, State};
use std::path::{Path, PathBuf};

async fn serve_file(root: &Path, repo_name: &Path, file: &Path) -> Option<NamedFile> {
    let path = root.join(repo_name).join(file);
    NamedFile::open(path).await.ok()
}

#[get("/<repo_name>/HEAD", rank = 1)]
async fn head(repo_name: PathBuf, config: &State<Config>) -> Option<NamedFile> {
    serve_file(&config.git_root, &repo_name, Path::new("HEAD")).await
}

#[get("/<repo_name>/info/refs", rank = 1)]
async fn info_refs(repo_name: PathBuf, config: &State<Config>) -> Option<NamedFile> {
    serve_file(&config.git_root, &repo_name, Path::new("info/refs")).await
}

#[get("/<repo_name>/objects/<object..>", rank = 1)]
async fn objects(repo_name: PathBuf, object: PathBuf, config: &State<Config>) -> Option<NamedFile> {
    serve_file(&config.git_root, &repo_name, &Path::new("objects").join(object)).await
}

pub fn routes() -> Vec<Route> {
    routes! {
        head,
        info_refs,
        objects,
    }
}
