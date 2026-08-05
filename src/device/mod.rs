//! Headphone device engine: protocol state machine over a transport.

pub mod engine;
pub mod state;

pub use engine::{DeviceEvent, Engine, EngineError};
pub use state::{BatteryState, DeviceState, GsCapability, Prop, Properties, SupportFunctions};
