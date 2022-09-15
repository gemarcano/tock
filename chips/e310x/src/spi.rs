//! SPI instantiation.

use kernel::utilities::StaticRef;
use sifive::spi::SpiRegisters;

pub const QSPI0_BASE: StaticRef<SpiRegisters> =
    unsafe { StaticRef::new(0x10014000 as *const SpiRegisters) };
pub const SPI1_BASE: StaticRef<SpiRegisters> =
    unsafe { StaticRef::new(0x10024000 as *const SpiRegisters) };
pub const SPI2_BASE: StaticRef<SpiRegisters> =
    unsafe { StaticRef::new(0x10034000 as *const SpiRegisters) };
