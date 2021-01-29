use stm32f1xx_hal::usb;
use usb_device::prelude::*;

pub const BUF_BYTES: usize = 64;
const TERMINATOR: u8 = 0;

type StmUsbDevice = UsbDevice<'static, usb::UsbBusType>;
type StmSerialPort = usbd_serial::SerialPort<'static, usb::UsbBusType>;

pub struct Serial {
    pub usb_dev: UsbDevice<'static, usb::UsbBusType>,
    pub port: usbd_serial::SerialPort<'static, usb::UsbBusType>,
    pub buf: [u8; BUF_BYTES],
    pub buf_next: usize, // Next index to write in buf.
}

impl Serial {
    pub const fn new(usb_dev: StmUsbDevice, port: StmSerialPort) -> Serial {
        Serial {
            usb_dev,
            port,
            buf: [0u8; BUF_BYTES],
            buf_next: 0,
        }
    }

    /// Attempts to read a packet from the USB serial port, buffering incomplete packets
    /// for a future attempt.  Returned packets include the terminating byte.
    pub fn read_packet(&mut self, packet_buf: &mut [u8]) -> Result<usize, UsbError> {
        if self.poll()? == 0 {
            // No new serial data to process.
            return Ok(0);
        }

        for i in 0..self.buf_next {
            if self.buf[i] == TERMINATOR {
                if i > packet_buf.len() {
                    return Err(UsbError::BufferOverflow);
                }

                // Copy a complete packet to provided buffer.
                &packet_buf[..i + 1].copy_from_slice(&self.buf[..i + 1]);

                if i + 1 == self.buf_next {
                    // Buffer is now empty, reset index.
                    self.buf_next = 0;
                } else {
                    // Move trailing data to start of buffer, skipping terminator.
                    let start = i + 1;
                    self.buf.copy_within(start..self.buf_next, 0);
                    self.buf_next -= start;
                }

                return Ok(i + 1);
            }
        }

        // No terminator found; packet is not yet complete.
        Ok(0)
    }

    /// Polls the USB serial port, reading bytes into `Serial.buf`.
    fn poll(&mut self) -> Result<usize, UsbError> {
        let Serial {
            usb_dev,
            port,
            buf,
            buf_next,
        } = self;

        if !usb_dev.poll(&mut [port]) {
            return Ok(0);
        }

        match port.read(&mut buf[*buf_next..]) {
            Ok(count) => {
                *buf_next += count;
                Ok(count)
            }
            Err(UsbError::WouldBlock) => Ok(0),
            Err(error) => Err(error),
        }
    }
}
