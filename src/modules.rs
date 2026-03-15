#[cfg(feature = "rng")]
mod rng;
#[cfg(feature = "spec")]
mod spec;

#[cfg(feature = "rng")]
pub use rng::rng_module;
#[cfg(feature = "spec")]
pub use spec::spec_module;
