use crate::error::Result;
use crate::repo::{File, FileMode, Repo};
use crate::Config;
use anyhow::Context as _;
use futures::stream::{StreamExt, TryStreamExt};
use rocket::fs::NamedFile;
use rocket::http::uri::Origin;
use rocket::response::Redirect;
use rocket::{get, routes, uri, Route, State};
use rocket_dyn_templates::Template;
use serde::Serialize;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio_stream::wrappers::ReadDirStream;


#[get("/favicon.ico")]
async fn favicon() -> Option<()> {
    None
}

#[get("/static/<path..>")]
async fn statics(path: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("./static").join(path)).await.ok()
}

#[get("/")]
async fn index(config: &State<Config>) -> Result<Template> {
    let git_root = &config.git_root;

    let read_dir = fs::read_dir(git_root).await
        .with_context(|| format!("reading directory git_root={:?}", git_root))?;

    let repos = ReadDirStream::new(read_dir)
        .map(|res| Ok(res.context("reading dir entry")?))
        .and_then({
            async fn open_repo(entry: fs::DirEntry) -> Result<Repo> {
                let path = entry.path();
                let name = PathBuf::from(entry.file_name());
                let repo = Repo::open(&path, &name).await
                    .with_context(|| format!("reading repo {:?}", &path))?;
                Ok(repo)
            }
            open_repo
        })
        .try_collect::<Vec<_>>().await?;

    #[derive(Serialize)]
    struct Ctx {
        repos: Vec<Repo>,
    }

    Ok(Template::render("index", Ctx { repos }))
}

#[get("/<repo_name>", rank = 2)]
pub async fn home(repo_name: PathBuf, config: &State<Config>) -> Result<Redirect> {
    let repo_path = config.git_root.join(&repo_name);
    let repo = Repo::open(&repo_path, &repo_name).await
        .with_context(|| format!("reading repo {:?}", &repo_path))?;

    Ok(Redirect::to(uri!(tree(repo_name, &repo.default_branch, PathBuf::from("/")))))
}

#[get("/<repo_name>/tree/<refs>/<path..>", rank = 2)]
pub async fn tree(repo_name: PathBuf, refs: String, path: PathBuf, config: &State<Config>) -> Result<Template> {
    let repo_path = config.git_root.join(&repo_name);
    let repo = Repo::open(&repo_path, &repo_name).await
        .with_context(|| format!("reading repo {:?}", &repo_path))?;

    let git_repo = git2::Repository::open_bare(&repo_path)
        .with_context(|| format!("reading git repo {:?}", &repo_path))?;

    let object = find_subtree_object_by_path(&git_repo, &refs, &path)
        .with_context(|| format!("finding path {:?} in repo {:?}", &path, &repo_path))?
        .context("404")?;

    let up = path.parent()
        .map(|parent| uri!(tree(&repo_name, &refs, parent)));

    match object {
        Object::Tree(tree) => {
            let mut files = list_files(&repo_name, &refs, &path, &tree);

            files.sort_unstable_by(|a, b| a.name.cmp(&b.name));
            files.sort_by(|a, b| match (a.mode, b.mode) {
                (FileMode::Dir, FileMode::Dir) => Ordering::Equal,
                (FileMode::Dir, _) => Ordering::Less,
                (_, FileMode::Dir) => Ordering::Greater,
                _ => Ordering::Equal,
            });

            #[derive(Serialize)]
            struct Ctx {
                repo: Repo,
                up: Option<Origin<'static>>,
                files: Vec<File>,
            }

            Ok(Template::render("tree", Ctx { repo, up, files }))
        }
        Object::Blob(blob) => {
            let name = path.file_name().unwrap()
                .to_string_lossy()
                .to_string();

            let contents = if blob.is_binary() {
                fn concat(sep: &'static str) -> impl Fn(String, String) -> String {
                    move |mut acc, elm| {
                        acc.push_str(sep);
                        acc.push_str(&elm);
                        acc
                    }
                }

                blob.content()
                    .chunks(16)
                    .enumerate()
                    .map(|(offset, chunk)| {
                        let hex = chunk.chunks(2)
                            .map(|word| match word {
                                [a, b] => format!("{:02x}{:02x}", a, b),
                                [a] => format!("{:02x}  ", a),
                                _ => unreachable!(),
                            })
                            .fold(String::with_capacity(40), concat(" "));
                        let ascii = chunk.iter()
                            .map(|chr| if chr.is_ascii_graphic() { *chr as char } else { '.' })
                            .collect::<String>();

                        format!("{:08x}: {: <39}  {}", offset, hex, ascii)
                    })
                    .fold(String::with_capacity(blob.size() / 16 * 68), concat("\n"))
            } else {
                String::from_utf8_lossy(blob.content())
                    .to_string()
            };

            #[derive(Serialize)]
            struct Blob {
                name: String,
                contents: String,
                binary: bool,
            }

            #[derive(Serialize)]
            struct Ctx {
                repo: Repo,
                up: Origin<'static>,
                blob: Blob,
            }

            Ok(Template::render("file", Ctx {
                repo,
                up: up.unwrap(),
                blob: Blob {
                    name,
                    contents,
                    binary: blob.is_binary(),
                },
            }))
        }
    }
}

pub fn routes() -> Vec<Route> {
    routes! {
        favicon,
        statics,
        index,
        home,
        tree,
    }
}


fn list_files<'repo>(
    repo_name: &Path,
    branch_tag_commit: &str,
    path: &Path,
    tree: &git2::Tree<'repo>,
) -> Vec<File> {
    tree.iter()
        .filter_map(|entry| {
            let name = entry.name()?.to_owned();
            let mode = FileMode::from_mode(entry.filemode())?;
            let href = uri!(crate::web::tree(PathBuf::from(repo_name), branch_tag_commit, path.join(&name)));
            Some(File { name, href, mode })
        })
        .collect()
}

enum Object<'repo> {
    Tree(git2::Tree<'repo>),
    Blob(git2::Blob<'repo>),
}

fn find_subtree_object_by_path<'repo>(
    git_repo: &'repo git2::Repository,
    branch_tag_commit: &str,
    path: &Path,
) -> Result<Option<Object<'repo>>> {

    let tree = match find_ref_root_tree(&git_repo, branch_tag_commit)? {
        Some(tree) => tree,
        None => return Ok(None),
    };

    if path == Path::new("") {
        return Ok(Some(Object::Tree(tree)));
    }

    let object = match tree.get_path(path) {
        Ok(entry) => {
            let object = entry.to_object(&git_repo)
                .context("finding path object")?;
            object
        },
        Err(err) if err.code() == git2::ErrorCode::NotFound => return Ok(None),
        Err(err) => Err(err).context("finding tree path")?,
    };

    match object.kind() {
        Some(git2::ObjectType::Tree) => Ok(Some(Object::Tree(object.into_tree().unwrap()))),
        Some(git2::ObjectType::Blob) => Ok(Some(Object::Blob(object.into_blob().unwrap()))),
        _ => Ok(None),
    }
}

fn find_ref_root_tree<'repo>(
    git_repo: &'repo git2::Repository,
    branch_tag_commit: &str,
) -> Result<Option<git2::Tree<'repo>>> {
    match git_repo.find_branch(branch_tag_commit, git2::BranchType::Local) {
        Ok(branch) => {
            let reference = branch.into_reference();
            match reference.peel_to_tree() {
                Ok(tree) => return Ok(Some(tree)),
                Err(err) => Err(err).with_context(|| format!("finding tree for branch {:?}", branch_tag_commit))?,
            }
        }
        Err(err) if err.code() == git2::ErrorCode::NotFound => {}
        Err(err) => Err(err).with_context(|| format!("finding branch {:?}", branch_tag_commit))?,
    };

    let mut tag = None;
    git_repo.tag_foreach(|oid, name| {
        if name == branch_tag_commit.as_bytes() {
            tag = Some(oid);
            true
        } else {
            false
        }
    }).context("iterating tags")?;

    if let Some(tag_oid) = tag {
        let tree = git_repo.find_tree(tag_oid)
            .with_context(|| format!("finding tree for tag {:?} ({:?})", branch_tag_commit, tag_oid))?;
        return Ok(Some(tree));
    }

    let commit_oid = match git2::Oid::from_str(branch_tag_commit) {
        Ok(oid) => oid,
        Err(_) => {
            // The refs string is not a valid commit id, it was probably a branch or tag name so
            // let's return NotFound instead of an error about invalid format.
            return Ok(None);
        }
    };

    match git_repo.find_commit(commit_oid) {
        Ok(commit) => {
            let tree = commit.tree()
                .with_context(|| format!("finding tree from commit {:?}", branch_tag_commit))?;
            return Ok(Some(tree));
        }
        Err(err) if err.code() == git2::ErrorCode::NotFound => {}
        Err(err) => Err(err).with_context(|| format!("finding commit {:?}", branch_tag_commit))?,
    };

    Ok(None)
}
