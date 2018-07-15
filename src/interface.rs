use nb;

pub trait DisplayInterface {
    fn send_command(&mut self, cmd: u8) -> Result<(), ()>;
    fn send_data(&mut self, buf: &[u8]) -> Result<(), ()>;
    fn send_data_async(&mut self, word: u8) -> nb::Result<(), ()>;
}

pub mod spi {
    //! The SPI interface supports the "4-wire" interface of the driver, such that each word on the
    //! SPI bus is 8 bits. The "3-wire" mode replaces the D/C GPIO with a 9th bit on each word,
    //! which seems really awkward to implement with embedded_hal SPI.

    use hal;

    use super::DisplayInterface;
    use nb;

    pub struct SpiInterface<SPI, DC> {
        /// The SPI master device connected to the SSD1322.
        spi: SPI,
        /// A GPIO output pin connected to the D/C (data/command) pin of the SSD1322 (the fourth
        /// "wire" of "4-wire" mode).
        dc: DC,
    }

    impl<SPI, DC> SpiInterface<SPI, DC>
    where
        SPI: hal::spi::FullDuplex<u8> + hal::blocking::spi::Write<u8>,
        DC: hal::digital::OutputPin,
    {
        /// Create a new SPI interface to communicate with the display driver. `spi` is the SPI
        /// master device, and `dc` is the GPIO output pin connected to the D/C pin of the SSD1322.
        pub fn new(spi: SPI, dc: DC) -> Self {
            Self { spi: spi, dc: dc }
        }
    }

    impl<SPI, DC> DisplayInterface for SpiInterface<SPI, DC>
    where
        SPI: hal::spi::FullDuplex<u8> + hal::blocking::spi::Write<u8>,
        DC: hal::digital::OutputPin,
    {
        /// Send a command word to the display's command register. Synchronous.
        fn send_command(&mut self, cmd: u8) -> Result<(), ()> {
            self.dc.set_low();
            self.spi.write(&[cmd]).map_err(|_| ())?;
            self.dc.set_high();
            Ok(())
        }

        /// Send a sequence of data words to the display from a buffer. Synchronous.
        fn send_data(&mut self, buf: &[u8]) -> Result<(), ()> {
            self.dc.set_high();
            self.spi.write(&buf).map_err(|_| ())?;
            Ok(())
        }

        /// Send a data word to the display asynchronously, using `nb` style non-blocking send. If
        /// the hardware FIFO is full, returns `WouldBlock` which means the word was not accepted
        /// and should be retried later.
        fn send_data_async(&mut self, word: u8) -> nb::Result<(), ()> {
            self.dc.set_high();
            match self.spi.send(word) {
                Ok(()) => {
                    let _ = self.spi.read();
                    Ok(())
                }
                Err(nb::Error::Other(_)) => Err(nb::Error::Other(())),
                Err(nb::Error::WouldBlock) => Err(nb::Error::WouldBlock),
            }
        }
    }
}

#[cfg(test)]
pub mod test_spy {
    //! An interface for use in unit tests to spy on whatever was sent to it.

    use super::DisplayInterface;
    use nb;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Debug, PartialEq)]
    pub enum Sent {
        Cmd(u8),
        Data(Vec<u8>),
    }

    pub struct TestSpyInterface {
        sent: Rc<RefCell<Vec<Sent>>>,
    }

    impl TestSpyInterface {
        pub fn new() -> Self {
            TestSpyInterface {
                sent: Rc::new(RefCell::new(Vec::new())),
            }
        }
        pub fn split(&self) -> Self {
            Self {
                sent: self.sent.clone(),
            }
        }
        pub fn check(&self, cmd: u8, data: &[u8]) {
            let sent = self.sent.borrow();
            if data.len() == 0 {
                assert_eq!(sent.len(), 1);
            } else {
                assert_eq!(sent.len(), 2);
                assert_eq!(sent[1], Sent::Data(data.to_vec()));
            }
            assert_eq!(sent[0], Sent::Cmd(cmd));
        }
        pub fn check_multi(&self, expect: &[Sent]) {
            assert_eq!(*self.sent.borrow(), expect);
        }
        pub fn clear(&mut self) {
            self.sent.borrow_mut().clear()
        }
    }

    impl DisplayInterface for TestSpyInterface {
        fn send_command(&mut self, cmd: u8) -> Result<(), ()> {
            self.sent.borrow_mut().push(Sent::Cmd(cmd));
            Ok(())
        }
        fn send_data(&mut self, data: &[u8]) -> Result<(), ()> {
            self.sent.borrow_mut().push(Sent::Data(data.to_vec()));
            Ok(())
        }
        fn send_data_async(&mut self, word: u8) -> nb::Result<(), ()> {
            let mut sent = self.sent.borrow_mut();
            {
                let last_idx = sent.len() - 1;
                match &mut sent[last_idx] {
                    Sent::Cmd(_) => {}
                    Sent::Data(ref mut d) => {
                        d.push(word);
                        return Ok(());
                    }
                };
            }
            sent.push(Sent::Data(vec![word]));
            Ok(())
        }
    }
}
