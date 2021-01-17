use rtt_target::rprintln;
use stm32f1xx_hal::usb;
use usb_device::prelude::*;

const BUF_SIZE: usize = 64;
const TERMINATOR: u8 = 13;

type StmUsbDevice = UsbDevice<'static, usb::UsbBusType>;
type StmSerialPort = usbd_serial::SerialPort<'static, usb::UsbBusType>;

pub struct Serial {
    pub usb_dev: UsbDevice<'static, usb::UsbBusType>,
    pub port: usbd_serial::SerialPort<'static, usb::UsbBusType>,
    pub buf: [u8; BUF_SIZE],
    pub buf_i: usize,
}

impl Serial {
    pub const fn new(usb_dev: StmUsbDevice, port: StmSerialPort) -> Serial {
        Serial {
            usb_dev,
            port,
            buf: [0u8; BUF_SIZE],
            buf_i: 0,
        }
    }

    /// Polls the USB serial port, reading bytes into `Serial.buf`.
    pub fn poll(&mut self) -> usize {
        let Serial {
            usb_dev,
            port,
            buf,
            buf_i,
        } = self;

        if !usb_dev.poll(&mut [port]) {
            return 0;
        }

        match port.read(&mut buf[*buf_i..]) {
            Ok(count) => {
                *buf_i += count;
                rprintln!("got {} bytes", count);
                count
            }
            Err(_) => 0,
        }
    }

    pub fn read_packet(&mut self, packet_buf: &mut [u8]) -> Result<usize, &str> {
        if self.buf_i == 0 {
            return Ok(0);
        }

        for i in 0..self.buf_i {
            if self.buf[i] == TERMINATOR {
                if i + 1 != self.buf_i {
                    // TODO shift buffer to eliminate read bytes.
                    panic!(
                        "TERMINATOR at {} is not at end of buffer ({}) - 1",
                        i, self.buf_i
                    );
                }

                if i > packet_buf.len() {
                    return Err("provided packet buffer too small");
                }

                // Reset Serial buffer write index, copy packet to provided buffer.
                self.buf_i = 0;

                &packet_buf[..i].copy_from_slice(&self.buf[..i]);
                return Ok(i);
            }
        }

        // No terminator found, packet currently incomplete.
        Ok(0)
    }
}
