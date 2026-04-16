#[allow(
    clippy::all,
    clippy::pedantic,
    dead_code,
    deprecated,
    unreachable_patterns
)]
pub mod sync_pb {
    include!(concat!(env!("OUT_DIR"), "/sync_pb.rs"));
}
