use embedded_io::blocking::{Read, Write};
use embedded_websocket::framer::Stream;
use esp_wifi::wifi_interface::{IoError, Socket};
use smoltcp::wire::Ipv4Address;

pub struct NetworkConnection<'a> {
    pub socket: Socket<'a, 'a>,
}

impl NetworkConnection<'_> {
    pub fn new(socket: Socket) -> Self {
        NetworkConnection { socket: socket }
    }

    pub fn connect(&mut self, address: Ipv4Address, port: u16) -> Result<(), IoError> {
        self.socket.open(address, port)
    }
}

#[derive(Debug)]
pub struct NetworkErr;

impl Stream<IoError> for NetworkConnection<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        self.socket.read(buf)
    }
    fn write_all(&mut self, buf: &[u8]) -> Result<(), IoError> {
        self.socket.write_all(buf)
    }
}
