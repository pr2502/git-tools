use crate::error::{Error, Result};
use crate::repo::Repo;
use anyhow::Context;
use rocket::http::Status;
use rocket::outcome::{try_outcome, Outcome};
use rocket::request::{FromRequest, Request};
use serde::Serialize;
use std::path::Path;


pub struct GitRepo {
    git_repo: git2::Repository,
}

#[derive(Serialize)]
pub struct Branch {
    pub name: String,
}

pub enum Object<'repo> {
    Tree(git2::Tree<'repo>),
    Blob(git2::Blob<'repo>),
}

impl<'repo> Object<'repo> {
    #[track_caller]
    pub fn unwrap_blob(self) -> git2::Blob<'repo> {
        match self {
            Object::Blob(blob) => blob,
            Object::Tree(_) => panic!("called `Object::unwrap_blob` on a `Object::Tree` value`"),
        }
    }
}

#[rocket::async_trait]
impl<'req> FromRequest<'req> for GitRepo {
    type Error = crate::error::Error;

    async fn from_request(request: &'req Request<'_>) -> rocket::request::Outcome<Self, Self::Error> {
        let repo = try_outcome!(request.guard::<Repo>().await);

        let res = git2::Repository::open_bare(&repo.path)
            .with_context(|| format!("reading git repo {:?}", &repo.path));
        match res {
            Ok(git_repo) => Outcome::Success(GitRepo { git_repo }),
            Err(err) => Outcome::Failure((Status::InternalServerError, err.into())),
        }
    }
}

impl GitRepo {
    pub fn find_subtree_object_by_path(&self, branch_tag_commit: &str, path: &Path) -> Result<Option<Object<'_>>> {
        let tree = match self.find_ref_root_tree(branch_tag_commit)? {
            Some(tree) => tree,
            None => return Ok(None),
        };

        if path == Path::new("") {
            return Ok(Some(Object::Tree(tree)));
        }

        let object = match tree.get_path(path) {
            Ok(entry) => {
                let object = entry.to_object(&self.git_repo)
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

    fn find_ref_root_tree(&self, branch_tag_commit: &str) -> Result<Option<git2::Tree<'_>>> {
        match self.git_repo.find_branch(branch_tag_commit, git2::BranchType::Local) {
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
        self.git_repo.tag_foreach(|oid, name| {
            if name == branch_tag_commit.as_bytes() {
                tag = Some(oid);
                true
            } else {
                false
            }
        }).context("iterating tags")?;

        if let Some(tag_oid) = tag {
            let tree = self.git_repo.find_tree(tag_oid)
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

        match self.git_repo.find_commit(commit_oid) {
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

    pub fn branches(&self) -> Result<Vec<Branch>> {
        self.git_repo
            .branches(Some(git2::BranchType::Local))
            .context("iterating branches")?
            .map(|res| {
                res.context("reading branch info")
                    .map_err(Error::from)
                    .and_then(|(branch, _)| -> Result<Branch> {
                        let name = branch.name_bytes()
                            .context("reading branch name")?;
                        let name = String::from_utf8_lossy(name)
                            .to_string();
                        Ok(Branch { name })
                    })
            })
            .collect::<Result<Vec<_>>>()
            .context("collecting branches")
            .map_err(Error::from)
    }
}
