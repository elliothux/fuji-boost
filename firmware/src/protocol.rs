//! Debug contract with the host (log ring, mailbox, app slot, scene). The values
//! come from `firmware/protocol.json`, which the build also copies into the
//! image manifest for hosts to read.
include!(concat!(env!("OUT_DIR"), "/protocol.rs"));
