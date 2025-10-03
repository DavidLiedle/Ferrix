pub mod statusbar;
pub mod copymode;
pub mod commandmode;
pub mod mouse;
pub mod search;
// #[cfg(test)]
// mod tests;
// #[cfg(test)]
// mod gpu_tests;

#[cfg(feature = "gpu")]
pub mod gpu_renderer;

pub use statusbar::StatusBar;