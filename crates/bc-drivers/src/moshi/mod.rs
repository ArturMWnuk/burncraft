//! Sterownik Moshi MS10105 V4.x przez CH341 (ADR 0014). Szkielet API (M3.3, testy red).

pub mod emu;
pub mod proto;

use std::time::Duration;

use crate::ch341::Ch341;
use crate::driver::*;
use crate::lihuiyu::K40Settings;
use crate::{Context, MachineFile};

pub const BED: [f64; 2] = [330.0, 210.0];

#[derive(Debug, Clone)]
pub struct MoshiOptions {
    pub settings: K40Settings,
    pub bed: [f64; 2],
}

impl Default for MoshiOptions {
    fn default() -> Self {
        MoshiOptions {
            settings: K40Settings::default(),
            bed: BED,
        }
    }
}

fn todo() -> DriverError {
    DriverError::Unsupported("moshi: niezaimplementowane (M3.3)".into())
}

/// Sterownik Moshi.
pub struct MoshiDriver {
    name: String,
}

impl MoshiDriver {
    pub fn connect(
        _name: &str,
        _dev: Box<dyn Ch341>,
        _opts: MoshiOptions,
    ) -> Result<Self, DriverError> {
        Err(todo())
    }
    pub fn wait_job(&self, _timeout: Duration) -> JobState {
        JobState::default()
    }
}

crate::k40_driver_stub!(MoshiDriver);
