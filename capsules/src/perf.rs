//! Provides a performance counter interface for userspace.
//!
//! Usage
//! -----
//!
//! TBD

/// Syscall driver number.
use crate::driver;
pub const DRIVER_NUM: usize = driver::NUM::Perf as usize;

use kernel::{AppId, Driver, ReturnCode};
use riscv::csr;

pub struct Perf;

impl Driver for Perf {
    /// Control the Perf system.
    ///
    /// ### `command_num`
    ///
    /// - `0`: Driver check.
    /// - `1`: Get perf counter.
    /// - `2`: Get number of instructions executed
    fn command(&self, command_num: usize, _: usize, _: usize, _: AppId) -> ReturnCode {
        match command_num {
            0 /* check if present */ => ReturnCode::SuccessWithValue { value: 1 },

            1 /* FIXME HACK This needs to be implemented in the HIL somehow */ =>
                ReturnCode::SuccessWithValue { value: csr::CSR.mcycle.get() as usize },
            
            2 /* FIXME HACK This needs to be implemented in the HIL somehow */ =>
                ReturnCode::SuccessWithValue { value: csr::CSR.minstret.get() as usize },

            _ => ReturnCode::ENOSUPPORT,
        }
    }
}
