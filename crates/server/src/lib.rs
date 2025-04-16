#![feature(ip)]

use percas_core::FoyerEngine;

pub mod scheduled;
pub mod server;
pub mod telemetry;

pub struct PercasContext {
    pub engine: FoyerEngine,
}
