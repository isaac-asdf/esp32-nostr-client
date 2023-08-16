use embedded_svc::io::{Read, Write};
use embedded_websocket::framer::Stream;
use esp_println::println;
use esp_wifi::wifi_interface::{IoError, Socket};
use smoltcp::wire::Ipv4Address;

pub struct NetworkConnection<'a> {
    pub socket: Socket<'a, 'a>,
}

impl<'a> NetworkConnection<'a> {
    pub fn new(
        mut socket: Socket<'a, 'a>,
        address: Ipv4Address,
        port: u16,
    ) -> Result<Self, IoError> {
        socket.open(smoltcp::wire::IpAddress::Ipv4(address), port)?;
        Ok(NetworkConnection { socket })
    }
}

impl Stream<IoError> for NetworkConnection<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        let len = self.socket.read(buf)?;
        let to_print = unsafe { core::str::from_utf8_unchecked(&buf[..len]) };
        if to_print.len() > 0 {
            println!("Read: {}", to_print);
            println!("break");
        }
        self.socket.flush()?;
        Ok(len)
    }
    fn write_all(&mut self, buf: &[u8]) -> Result<(), IoError> {
        // let to_print = unsafe { core::str::from_utf8_unchecked(&buf[..buf.len()]) };
        // println!("buff len: {}", buf.len());
        // println!("Write: {}", to_print);
        // println!("write0: {:?}", buf[0]);
        // println!("write1: {:?}", buf[1]);
        // println!("write2: {:?}", buf[2]);
        // println!("write3: {:?}", buf[3]);
        self.socket.write_all(buf)?;
        self.socket.flush()?;
        Ok(())
    }
}
