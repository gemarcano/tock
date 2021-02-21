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
<<<<<<< HEAD
    fn command(&self, command_num: usize, data: usize, _: usize, _: AppId) -> ReturnCode {
=======
    fn command(&self, command_num: usize, _: usize, _: usize, _: AppId) -> ReturnCode {
>>>>>>> cd1bb8bc39036b5ed6c7ad0ab71dd5f5e2a1cf98
        match command_num {
            0 /* check if present */ => ReturnCode::SuccessWithValue { value: 1 },

            1 /* FIXME HACK This needs to be implemented in the HIL somehow */ =>
                ReturnCode::SuccessWithValue { value: csr::CSR.mcycle.get() as usize },

            _ => ReturnCode::ENOSUPPORT,
        }
    }
}
