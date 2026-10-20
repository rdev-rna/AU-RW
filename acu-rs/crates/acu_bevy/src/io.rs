//! Cooperative direct-package loading. Startup still waits for the result;
//! yielding makes the I/O executor available to its other queued tasks.
use acu_assets::{
    Limits, Result,
    package::{self, Package},
};
use bevy::tasks::{IoTaskPool, TaskPool, block_on, futures_lite::future::yield_now};

pub fn decode_package(raw: Vec<u8>, limits: Limits) -> Result<Package> {
    // Tests may load original clips without constructing a Bevy App first.
    block_on(
        IoTaskPool::get_or_init(TaskPool::new)
            .spawn(async move { package::decode_cooperative(&raw, limits, yield_now).await }),
    )
}
