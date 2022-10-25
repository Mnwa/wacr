use crate::AsrProcessor;
use actix::Addr;
use dashmap::DashMap;
use uuid::Uuid;

pub mod client;
pub mod processor;

pub type AsrProcessorStorage = DashMap<Uuid, Addr<AsrProcessor>>;
