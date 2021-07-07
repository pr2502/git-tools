#![feature(async_closure)]
#![feature(never_type)]

use rocket::catchers;
use rocket_dyn_templates::Template;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;


/// Utility macro for dynamically defining template context
macro_rules! ctx {
    ( $( $field:ident $( = $val:expr )? ),* $(,)? ) => {{
        #[allow(unused_mut)]
        let mut ctx = ::tera::Context::new();
        $( ctx!(@ctx $field $( = $val )* ); )*
        ctx.into_json()
    }};

    ( @$ctx:ident $field:ident ) => {
        $ctx.insert(::std::stringify!($field), &$field); 
    };
    ( @$ctx:ident $field:ident = $val:expr ) => {
        $ctx.insert(::std::stringify!($field), &$val); 
    };
}


mod site {
    pub mod error;
    pub mod git_repo;
    pub mod http_clone;
    pub mod index;
    pub mod nav;
    pub mod repo;
    pub mod repo_path;
    pub mod web;
}
pub use site::*;


#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub git_root: PathBuf,
    pub static_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            git_root: PathBuf::from("/home/git"),
            static_dir: PathBuf::from("./static"),
        }
    }
}


#[rocket::launch]
fn launch() -> _ {
    use figment::providers::{Format, Serialized, Toml};
    use figment::{Figment, Profile};
    use rocket::fairing::AdHoc;

    let figment = Figment::from(rocket::Config::default())
        .merge(Serialized::defaults(Config::default()))
        .merge(Toml::file(std::option_env!("GITSITE_CONFIG").unwrap_or("gitsite.toml")).nested())
        .select({
            #[cfg(debug_assertions)] { Profile::new("debug") }
            #[cfg(not(debug_assertions))] { Profile::new("release") }
        });

    rocket::custom(figment)
        .attach(AdHoc::config::<Config>())
        .attach(Template::fairing())
        .mount("/", http_clone::routes())
        .mount("/", web::routes())
        .register("/", catchers![error::default_catcher])
}
