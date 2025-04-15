#![feature(ip)]

use atrium_core::FoyerEngine;

pub mod scheduled;
pub mod server;
pub mod telemetry;

pub struct AtriumContext {
    pub engine: FoyerEngine,
}
