//! Serial Peripheral Interface (SPI) driver.
use crate::gpio;
use core::cell::Cell;
use core::cmp;
use kernel::hil::spi;
use kernel::utilities::cells::OptionalCell;
use kernel::utilities::cells::TakeCell;
use kernel::utilities::registers::interfaces::{ReadWriteable, Readable, Writeable};
use kernel::utilities::registers::{register_bitfields, register_structs, ReadOnly, ReadWrite};
use kernel::utilities::StaticRef;
use kernel::ErrorCode;

register_structs! {
    pub SpiRegisters {
        /// Serial clock divisor
        (0x000 => sckdiv: ReadWrite<u32, sckdiv::Register>),
        /// Serial clock mode
        (0x004 => sckmode: ReadWrite<u32, sckmode::Register>),
        (0x008 => _reserved0),
        /// Chip select ID
        (0x010 => csid: ReadWrite<u32>),
        /// Chip select default
        (0x014 => csdef: ReadWrite<u32>),
        /// Chip select mode
        (0x018 => csmode: ReadWrite<u32, csmode::Register>),
        (0x01C => _reserved1),
        /// Delay control 0
        (0x028 => delay0: ReadWrite<u32, delay0::Register>),
        /// Delay control 1
        (0x02C => delay1: ReadWrite<u32, delay1::Register>),
        (0x030 => _reserved2),
        /// Frame format
        (0x040 => fmt: ReadWrite<u32, fmt::Register>),
        (0x044 => _reserved5),
        /// Tx FIFO Data
        (0x048 => txdata: ReadWrite<u32, txdata::Register>),
        /// Rx FIFO Data, need to split into u8 since reading the bottom 8 bits has a side effect
        (0x04C => rxdata_data: ReadOnly<u8>),
        (0x04D => _reserved6),
        (0x04F => rxdata_empty: ReadOnly<u8, rxdata_empty::Register>),
        /// Tx FIFO watermark
        (0x050 => txmark: ReadWrite<u32, txmark::Register>),
        /// Rx FIFO watermark
        (0x054 => rxmark: ReadWrite<u32, rxmark::Register>),
        (0x058 => _reserved7),
        /// SPI flash interface control
        (0x060 => fctrl: ReadWrite<u32, fctrl::Register>),
        /// SPI flash instruction format
        (0x064 => ffmt: ReadWrite<u32, ffmt::Register>),
        (0x068 => _reserved8),
        /// SPI interrupt enable
        (0x070 => ie: ReadWrite<u32, ie::Register>),
        /// SPI interrupt pending
        (0x074 => ip: ReadOnly<u32, ip::Register>),
        (0x078 => @END),
    }
}

register_bitfields![u32,
    sckdiv [
        div OFFSET(0) NUMBITS(12)
    ],
    sckmode [
        pha 0,
        pol 1
    ],
    csmode [
        mode OFFSET(0) NUMBITS(2) [
            AUTO = 0,
            HOLD = 2,
            OFF = 3
        ]
    ],
    delay0 [
        cssck OFFSET(0) NUMBITS(8) [],
        sckcs OFFSET(16) NUMBITS(8) [],
    ],
    delay1 [
        intercs OFFSET(0) NUMBITS(8) [],
        interxfr OFFSET(16) NUMBITS(8) [],
    ],
    fmt [
        proto OFFSET(0) NUMBITS(2) [
            Single = 0,
            Dual = 1,
            Quad = 2
        ],
        endian OFFSET(2) NUMBITS(1) [
            Big = 0,
            Little = 1
        ],
        dir OFFSET(3) NUMBITS(1) [
            Rx = 0,
            Tx = 1
        ],
        len OFFSET(16) NUMBITS(4) []
    ],
    txdata [
        data OFFSET(0) NUMBITS(8) [],
        full OFFSET(31) NUMBITS(1) [],
    ],
    txmark [
        txmark OFFSET(0) NUMBITS(3) [],
    ],
    rxmark [
        rxmark OFFSET(0) NUMBITS(3) [],
    ],
    ie [
        txwm 0,
        rxwm 1
    ],
    ip [
        txwm 0,
        rxwm 1
    ],
    fctrl [
        en 0
    ],
    ffmt [
        cmd_en OFFSET(0) NUMBITS(1) [],
        addr_len OFFSET(1) NUMBITS(3) [],
        pad_cnt OFFSET(4) NUMBITS(4) [],
        cmd_proto OFFSET(8) NUMBITS(2) [],
        addr_proto OFFSET(10) NUMBITS(2) [],
        data_proto OFFSET(12) NUMBITS(2) [],
        cmd_code OFFSET(16) NUMBITS(8) [],
        pad_code OFFSET(24) NUMBITS(8) [],
    ]
];

register_bitfields![u8,
    rxdata_empty [
        empty OFFSET(7) NUMBITS(1) [],
    ],
];

pub struct Spi {
    registers: StaticRef<SpiRegisters>,
    client: OptionalCell<&'static dyn spi::SpiMasterClient>,
    busy: Cell<bool>,
    tx_buf: TakeCell<'static, [u8]>,
    rx_buf: TakeCell<'static, [u8]>,
    io_len: Cell<usize>,
    tx_offset: Cell<usize>,
    rx_offset: Cell<usize>,
}

impl spi::SpiMaster for Spi {
    type ChipSelect = u8;

    fn init(&self) -> Result<(), ErrorCode> {
        // Set up SPI interface
        // Explicitly set defaults per datasheet, in case interface had been set up previously
        self.registers.sckmode.write(sckmode::pha.val(0));
        self.registers.sckmode.write(sckmode::pol.val(0));
        self.registers
            .fmt
            .write(fmt::proto::Single + fmt::endian::Big + fmt::dir::Rx + fmt::len.val(8));
        self.registers.ie.modify(ie::txwm::CLEAR + ie::rxwm::CLEAR);
        self.registers.csdef.set(0xFFFFFFFF);
        self.registers.csid.set(0);
        self.registers
            .sckmode
            .modify(sckmode::pha.val(0) + sckmode::pol.val(0));
        self.registers.txmark.write(txmark::txmark.val(0));

        // csmode register AUTO does not quite do what we want. We want to hold CS low for the
        // duration of the transfer, otherwise some external devices will malfunction as they
        // expect multiple bytes per transfer while CS is held active. Thus, set to HOLD, and
        // change to AUTO briefly to release CS.
        self.registers.csmode.modify(csmode::mode::HOLD);

        // Set up internal state
        self.io_len.set(0);
        self.tx_offset.set(0);
        self.rx_offset.set(0);
        self.busy.set(false);
        Ok(())
    }

    fn set_client(&self, client: &'static dyn spi::SpiMasterClient) {
        // Apparently this callback should be called by us when read_write_bytes finishes
        self.client.set(client);
    }

    fn is_busy(&self) -> bool {
        self.busy.get()
    }

    fn read_write_bytes(
        &self,
        write_buffer: &'static mut [u8],
        read_buffer: Option<&'static mut [u8]>,
        len: usize,
    ) -> Result<(), (ErrorCode, &'static mut [u8], Option<&'static mut [u8]>)> {
        let tx_len = cmp::min(len, write_buffer.len());
        // Bail if there's nothing to write
        if tx_len == 0 {
            return Err((ErrorCode::INVAL, write_buffer, read_buffer));
        }
        // ... and if we're already busy with a prior transaction
        if self.busy.get() == true {
            return Err((ErrorCode::BUSY, write_buffer, read_buffer));
        }

        self.busy.set(true);

        let rx_len = match &read_buffer {
            None => 0,
            Some(rx_buf) => rx_buf.len(),
        };
        let io_len = cmp::min(tx_len, rx_len);

        // TX FIFO has a max depth of 7 bytes, per the 3 bits in the watermark register, write up
        // to 4 bytes to start TX
        let tx_len = cmp::min(7, io_len);
        let to_write = &write_buffer[0..tx_len];
        for val in to_write {
            let val: u32 = (*val).into();
            self.registers.txdata.modify(txdata::data.val(val));
        }

        self.io_len.set(io_len);
        self.tx_offset.set(tx_len);
        self.tx_buf.replace(write_buffer);

        read_buffer.map(|rx_buf| {
            self.rx_offset.set(0);
            self.rx_buf.replace(rx_buf);
        });

        // Enable rxmark interrupt, wait until we receive data
        // Configure rxmark to fire when an amount equal to tx_len is enqueued
        self.registers
            .rxmark
            .write(rxmark::rxmark.val((tx_len - 1) as u32));
        // Configure txmark to fire when FIFO is empty if not done
        if self.rx_offset.get() != self.io_len.get() {
            self.registers.txmark.write(txmark::txmark.val(1));
        }
        // Enable rxmark interrupt so we can get data
        self.registers.ie.modify(ie::txwm::CLEAR + ie::rxwm::SET);
        Ok(())
    }

    fn write_byte(&self, val: u8) -> Result<(), ErrorCode> {
        self.read_write_byte(val)?;
        Ok(())
    }

    fn read_byte(&self) -> Result<u8, ErrorCode> {
        self.read_write_byte(0)
    }

    fn read_write_byte(&self, val: u8) -> Result<u8, ErrorCode> {
        if self.busy.get() {
            return Err(ErrorCode::BUSY);
        }
        self.registers.txdata.write(txdata::data.val(val.into()));
        while self.registers.rxdata_empty.read(rxdata_empty::empty) == 1 {
            // Do nothing, just wait until we get data
        }
        let data = self.registers.rxdata_data.get();
        Ok(data)
    }

    fn specify_chip_select(&self, cs: Self::ChipSelect) -> Result<(), ErrorCode> {
        self.registers.csid.set(cs.into());
        Ok(())
    }

    fn set_rate(&self, rate: u32) -> Result<u32, ErrorCode> {
        // (f_in / f_sck )/ 2 - 1 = div
        // FIXME right now, f_in is hardcoded to be 16MHz
        // Min rate is is 8000000/4096 = 1954
        // max is 8000000
        if rate < 1954 || rate > 8_000_000 {
            return Err(ErrorCode::INVAL);
        }

        if self.busy.get() {
            return Err(ErrorCode::BUSY);
        }

        let real_rate = rate;

        let div = 8_000_000 / real_rate - 1;
        self.registers.sckdiv.write(sckdiv::div.val(div));
        Ok(real_rate)
    }

    fn get_rate(&self) -> u32 {
        // FIXME right now, f_in is hardcoded to be 16MHz
        // f_sck = f_in / (2 (div + 1))
        let div = self.registers.sckdiv.read(sckdiv::div);
        8000000 / (div + 1)
    }

    fn set_polarity(&self, polarity: spi::ClockPolarity) -> Result<(), ErrorCode> {
        if self.busy.get() {
            return Err(ErrorCode::BUSY);
        }

        let val = match polarity {
            spi::ClockPolarity::IdleLow => 0,
            spi::ClockPolarity::IdleHigh => 1,
        };
        self.registers.sckmode.write(sckmode::pol.val(val));
        Ok(())
    }

    fn get_polarity(&self) -> spi::ClockPolarity {
        match self.registers.sckmode.read(sckmode::pol) {
            0 => spi::ClockPolarity::IdleLow,
            1 => spi::ClockPolarity::IdleHigh,
            _ => unreachable!(),
        }
    }

    fn set_phase(&self, phase: spi::ClockPhase) -> Result<(), ErrorCode> {
        if self.busy.get() {
            return Err(ErrorCode::BUSY);
        }

        let val = match phase {
            spi::ClockPhase::SampleLeading => 0,
            spi::ClockPhase::SampleTrailing => 1,
        };
        self.registers.sckmode.write(sckmode::pha.val(val));
        Ok(())
    }

    fn get_phase(&self) -> spi::ClockPhase {
        match self.registers.sckmode.read(sckmode::pha) {
            0 => spi::ClockPhase::SampleLeading,
            1 => spi::ClockPhase::SampleTrailing,
            _ => unreachable!(),
        }
    }

    fn hold_low(&self) {
        self.registers.csmode.modify(csmode::mode::HOLD);
    }

    fn release_low(&self) {
        self.registers.csmode.modify(csmode::mode::AUTO);
    }
}

impl Spi {
    pub fn new(base: StaticRef<SpiRegisters>) -> Self {
        Spi {
            registers: base,
            client: OptionalCell::empty(),
            busy: Cell::new(false),
            io_len: Cell::new(0),
            tx_offset: Cell::new(0),
            rx_offset: Cell::new(0),
            tx_buf: TakeCell::empty(),
            rx_buf: TakeCell::empty(),
        }
    }

    pub fn handle_interrupt(&self) {
        let ip_reg = &self.registers.ip;

        let rx_watermark = ip_reg.read(ip::rxwm) == 1;
        if rx_watermark {
            // We're in the read phase. We should have an amount equal to tx offset - rx offset to
            // read
            let rxdata = &self.registers.rxdata_data;
            let rxempty = &self.registers.rxdata_empty;
            // Helper, returns true when we've caught up to TX
            let is_rx_done = || self.rx_offset.get() == self.tx_offset.get();

            // FIXME is there a way to reduce code duplication here?
            // We need FIFO read always, but only need to save if we have a read buffer
            self.rx_buf.take().map_or_else(
                || {
                    while !is_rx_done() {
                        // Due to how we've configured interrupts, we never end up with an empty FIFO before rx
                        // and tx offsets match
                        assert!(rxempty.read(rxdata_empty::empty) == 0);
                        rxdata.get();
                        self.rx_offset.set(self.rx_offset.get() + 1);
                    }
                },
                |rx_buf| {
                    while !is_rx_done() {
                        // Due to how we've configured interrupts, we never end up with an empty FIFO before rx
                        // and tx offsets match
                        assert!(rxempty.read(rxdata_empty::empty) == 0);
                        let val = rxdata.get();
                        rx_buf[self.rx_offset.get()] = val;
                        self.rx_offset.set(self.rx_offset.get() + 1);
                    }
                    assert!(rxempty.read(rxdata_empty::empty) == 1);
                    self.rx_buf.replace(rx_buf);
                },
            );

            // At this point, rx and tx should be caught up. Disable RX interrupt, and enable TX if
            // not done
            if self.tx_offset.get() != self.io_len.get() {
                self.registers.ie.modify(ie::txwm::SET + ie::rxwm::CLEAR);
            }
        }

        // Now handle sending new data, if we have any left, max of 7 bytes
        let tx_watermark = ip_reg.read(ip::txwm) == 1;
        if tx_watermark {
            let txdata_reg = &self.registers.txdata;
            let tx_offset = self.tx_offset.get();
            let end_offset = cmp::min(tx_offset + 7, self.io_len.get());

            self.tx_buf.take().map(|tx_buf| {
                let to_write = &tx_buf[tx_offset..end_offset];
                for val in to_write {
                    // Should never be full, as FIFO should be empty when we start
                    assert!(txdata_reg.read(txdata::full) != 1);
                    txdata_reg.modify(txdata::data.val((*val).into()));
                }
                self.tx_offset.set(end_offset);

                self.tx_buf.replace(tx_buf);
            });

            // At this point, tx should be ahead of rx by 7 or whatever is left. Disable TX, enable
            // RX. Set RX watermark to fire when RX FIFO has equal number of bytes as were TX
            self.registers
                .rxmark
                .write(rxmark::rxmark.val((end_offset - tx_offset - 1) as u32));
            self.registers.ie.modify(ie::txwm::CLEAR + ie::rxwm::SET);
        }

        // If we're completely done, callback
        if (self.tx_offset.get() == self.io_len.get())
            && (self.rx_offset.get() == self.io_len.get())
        {
            // Toggle to AUTO to cause hardware to release CS now that transfer is done
            self.registers.csmode.modify(csmode::mode::AUTO);
            // ... and switch back to HOLD mode for the next transfer
            self.registers.csmode.modify(csmode::mode::HOLD);

            // Disable interrupts
            self.registers.ie.modify(ie::txwm::CLEAR + ie::rxwm::CLEAR);
            // call callback if it's registered, then signal we're done after cleaning up state
            self.client.map(|client| {
                client.read_write_done(
                    self.tx_buf.take().unwrap(),
                    self.rx_buf.take(),
                    self.io_len.get(),
                    Ok(()),
                );
            });

            // Now, clean up internal state
            self.io_len.set(0);
            self.rx_offset.set(0);
            self.tx_offset.set(0);

            self.busy.set(false);
        }
    }

    pub fn initialize_gpio_pins(
        &self,
        cs: &gpio::GpioPin,
        mosi: &gpio::GpioPin,
        miso: &gpio::GpioPin,
        sck: &gpio::GpioPin,
    ) {
        cs.iof0();
        mosi.iof0();
        miso.iof0();
        sck.iof0();
    }
}
