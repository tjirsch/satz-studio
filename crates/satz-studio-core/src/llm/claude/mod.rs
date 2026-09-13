//! Claude natively: the wire types ([`types`]), the SSE decoder and assembler
//! ([`sse`]), the HTTPS client ([`client`]) and the error type ([`error`]).

pub mod client;
pub mod error;
pub mod sse;
pub mod types;
