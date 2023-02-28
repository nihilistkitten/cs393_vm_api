#![feature(int_roundings)]
#![allow(dead_code, unused_variables)]
#![no_std]

pub mod address_space;
mod cacher;
mod data_source;

pub use address_space::{AddressSpace, Flags};
pub use data_source::DataSource;
