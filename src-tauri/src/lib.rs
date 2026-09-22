pub mod adapters;
#[cfg(feature = "desktop")]
mod desktop;
pub mod engine;
pub mod files;
pub mod import;
pub mod model;
pub mod new_api;
#[cfg(feature = "desktop")]
mod new_api_desktop;
pub mod paths;
pub mod skills;
#[cfg(feature = "desktop")]
mod skills_desktop;
#[cfg(feature = "desktop")]
pub use desktop::run;
