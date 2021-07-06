use crate::error::Result;
use crate::git_repo::{self, GitRepo, Object};
use crate::index::Index;
use crate::repo::{File, FileMode, Repo};
use crate::repo_path::RepoPath;
use anyhow::Context as _;
use rocket::fs::NamedFile;
use rocket::http::uri::Origin;
use rocket::http::Status;
use rocket::response::Redirect;
use rocket::{get, routes, uri, Route};
use rocket_dyn_templates::Template;
use serde::Serialize;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};


#[get("/favicon.ico")]
async fn favicon() -> Status {
    Status::NoContent
}

#[get("/static/<path..>")]
async fn statics(path: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("./static").join(path)).await.ok()
}

#[get("/")]
async fn index(index: Index) -> Result<Template> {
    let Index { mut repos, .. } = index;

    repos.sort_unstable_by_key(|repo| repo.name.to_ascii_lowercase());

    Ok(Template::render("index", ctx!{ repos, view = "index" }))
}

#[get("/<_repo_name>", rank = 2)]
pub async fn home(_repo_name: PathBuf, repo: Repo) -> Result<Redirect> {
    Ok(Redirect::to(uri!(tree(Path::new(&repo.name), &repo.default_branch, Path::new("/")))))
}

#[get("/<_repo_name>/tree/<refs>/<path..>", rank = 2)]
pub async fn tree(_repo_name: PathBuf, refs: String, path: RepoPath, repo: Repo, git_repo: GitRepo) -> Result<Template> {
    let object = git_repo.find_subtree_object_by_path(&refs, &path)
        .with_context(|| format!("finding path {:?} in repo {:?}", &path, &repo.path))?
        .context("404")?;

    match object {
        git_repo::Object::Tree(tree) => render_ls_files(tree, refs, &path, repo, &git_repo),
        git_repo::Object::Blob(blob) => render_blob(blob, refs, &path, repo),
    }
}

#[get("/<_repo_name>/refs/<refs>/<path..>", rank = 2)]
pub async fn refs(_repo_name: PathBuf, refs: String, path: RepoPath, repo: Repo, git_repo: GitRepo) -> Result<Template> {
    let branches = git_repo.branches()?
        .into_iter()
        .map(|branch| ctx! {
            href = uri!(tree(Path::new(&repo.name), &branch.name, &path)),
            name = branch.name,
        })
        .collect::<Vec<_>>();

    let path_nav = make_path_nav(&repo, &refs, &path);
    let ref_nav = make_ref_nav(&repo, &refs, &path);

    Ok(Template::render("refs", ctx!{
        path_nav,
        ref_nav,
        branches,
        view = "refs",
    }))
}

pub fn routes() -> Vec<Route> {
    routes! {
        favicon,
        statics,
        index,
        home,
        tree,
        refs,
    }
}


fn render_ls_files(tree: git2::Tree<'_>, refs: String, path: &Path, repo: Repo, git_repo: &GitRepo) -> Result<Template> {
    let up = path.parent()
        .map(|parent| uri!(tree(Path::new(&repo.name), &refs, parent)));

    let mut files = tree.iter()
        .filter_map(|entry| {
            let name = entry.name()?.to_owned();
            let mode = FileMode::from_mode(entry.filemode())?;
            let path = path.join(&name);
            let href = uri!(tree(Path::new(&repo.name), &refs, &path));
            Some(File { name, path, href, mode })
        })
    .collect::<Vec<_>>();

    files.sort_unstable_by(|a, b| Ord::cmp(
            &a.name.to_ascii_lowercase(),
            &b.name.to_ascii_lowercase(),
    ));
    files.sort_by(|a, b| match (a.mode, b.mode) {
        (FileMode::Dir, FileMode::Dir) => Ordering::Equal,
        (FileMode::Dir, _) => Ordering::Less,
        (_, FileMode::Dir) => Ordering::Greater,
        _ => Ordering::Equal,
    });

    let readme = render_readme(&refs, &files, &repo, &git_repo);

    let path_nav = make_path_nav(&repo, &refs, &path);
    let ref_nav = make_ref_nav(&repo, &refs, &path);

    Ok(Template::render("tree", ctx!{ repo, up, files, readme, path_nav, ref_nav, view = "tree" }))
}

fn render_blob(blob: git2::Blob, refs: String, path: &Path, repo: Repo) -> Result<Template> {
    let up = path.parent()
        .map(|parent| uri!(tree(Path::new(&repo.name), &refs, parent)));

    let name = path.file_name().unwrap()
        .to_string_lossy()
        .to_string();

    let contents;
    let lang;

    if blob.is_binary() {
        contents = fmt_xxd_hexdump(blob.content());
        lang = Some(String::from("xxd"));
    } else {
        contents = String::from_utf8_lossy(blob.content())
            .to_string();
        lang = repo.lang_override.iter()
            .find(|(patt, _)| patt.matches(&name))
            .map(|(_, lang)| lang.clone());
    }

    let path_nav = make_path_nav(&repo, &refs, &path);
    let ref_nav = make_ref_nav(&repo, &refs, &path);

    Ok(Template::render("file", ctx!{
        repo,
        up = up.unwrap(),
        blob = ctx!{ name, contents, lang },
        path_nav,
        ref_nav,
        view = "file",
    }))
}


#[derive(Serialize)]
struct Readme {
    content: String,
    is_html: bool,
}

fn render_readme(refs: &str, files: &[File], repo: &Repo, git_repo: &GitRepo) -> Option<Readme> {
    let path = repo.readme_path.as_ref()?;
    let file = files.iter().find(|file| &file.path == path)?;

    let res = git_repo.find_subtree_object_by_path(&refs, &path)
        .with_context(|| format!("finding path {:?} in repo {:?}", &path, &repo.path))
        .transpose()?;
    let blob = match res {
        Ok(Object::Blob(blob)) => blob,
        Ok(_) => return None,
        Err(err) => {
            log::warn!("{:?}", err);
            return None;
        }
    };

    // ignore readme if git thinks it's binary
    if blob.is_binary() {
        log::warn!("readme file {:?} is present but is binary", &path);
        return None;
    }

    let text_lossy = String::from_utf8_lossy(blob.content());

    if file.name.ends_with(".md") {
        use pulldown_cmark::{Parser, Options, html};

        let options = Options::ENABLE_TABLES |
                      Options::ENABLE_FOOTNOTES |
                      Options::ENABLE_STRIKETHROUGH |
                      Options::ENABLE_TASKLISTS |
                      Options::ENABLE_SMART_PUNCTUATION;

        let parser = Parser::new_ext(&text_lossy, options);

        let mut html_output = String::new();
        html::push_html(&mut html_output, parser);

        return Some(Readme {
            content: html_output,
            is_html: true,
        });
    }

    Some(Readme {
        content: text_lossy.to_string(),
        is_html: false,
    })
}

fn fmt_xxd_hexdump(data: &[u8]) -> String {
    fn concat(sep: &'static str) -> impl Fn(String, String) -> String {
        move |mut acc, elm| {
            acc.push_str(&elm);
            acc.push_str(sep);
            acc
        }
    }

    data.chunks(16)
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

            format!("{:08x}: {: <39} {}", offset, hex, ascii)
        })
        .fold(String::with_capacity(data.len() / 16 * 68), concat("\n"))
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

fn make_path_nav(repo: &Repo, refs: &str, path: &Path) -> PathNav {
    let mut segments = path.ancestors()
        .map(|path| {
            let name = path.file_name()
                .map(|fname| fname.to_string_lossy().to_string())
                // repository root has an empty path -> file_name() returns None
                .unwrap_or_else(|| repo.name.clone());
            let href = uri!(tree(Path::new(&repo.name), refs, path));
            Segment { name, href }
        })
        .collect::<Vec<_>>();

    // `Path::ancestors` yields paths from longest to shortest and doesn't allow reversing,
    // so we reverse the order here
    segments.reverse();

    PathNav { segments }
}


#[derive(Serialize)]
struct RefNav {
    current: String,
    href: Origin<'static>,
}

fn make_ref_nav(repo: &Repo, current: &str, path: &Path) -> RefNav {
    RefNav {
        current: current.to_string(),
        href: uri!(refs(Path::new(&repo.name), current, path)),
    }
}
