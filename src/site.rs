use rocket_dyn_templates::Template;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod site {
    pub mod http_clone;
    pub mod web;
    pub mod error;
    pub mod repo;
}
pub use site::*;


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
        .mount("/", site::http_clone::routes())
        .mount("/", site::web::routes())
}
