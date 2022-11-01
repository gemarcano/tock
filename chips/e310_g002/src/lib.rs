//! Chip support for the E310-G002 from SiFive.

#![no_std]
#![crate_name = "e310_g002"]
#![crate_type = "rlib"]

pub use e310x::{chip, clint, deferred_call_tasks, gpio, plic, prci, pwm, rtc, spi, uart, watchdog};

pub mod interrupt_service;
mod interrupts;
