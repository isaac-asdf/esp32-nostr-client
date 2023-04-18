#![no_std]
#![no_main]
use embedded_io::blocking::*;
use embedded_svc::ipv4::Interface;
use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};

use esp32_hal::clock::{ClockControl, CpuClock};
use esp32_hal::Rng;
use esp32_hal::{peripherals::Peripherals, prelude::*, Rtc};

use esp_println::logger::init_logger;
use esp_println::{print, println};
use esp_wifi::current_millis;
use esp_wifi::wifi::utils::create_network_interface;
use esp_wifi::wifi::WifiMode;
use esp_wifi::wifi_interface::WifiStack;
use smoltcp::iface::SocketStorage;
use smoltcp::wire::Ipv4Address;

use esp_backtrace as _;

mod nostr;

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PWD");
const PRIVKEY: &str = env!("PRIVKEY");

#[entry]
fn main() -> ! {
    // Send note
    let mut note = nostr::Note::new("hello world");
    let output = &note.to_signed(PRIVKEY);
    let to_print = unsafe { core::str::from_utf8_unchecked(&output[..1536]) };
    print!("{}", to_print);

    init_logger(log::LevelFilter::Info);

    let peripherals = Peripherals::take();

    let system = peripherals.DPORT.split();
    let clocks = ClockControl::configure(system.clock_control, CpuClock::Clock240MHz).freeze();
    let mut rtc = Rtc::new(peripherals.RTC_CNTL);
    rtc.rwdt.disable();

    let (wifi, _) = peripherals.RADIO.split();
    let mut socket_set_entries: [SocketStorage; 3] = Default::default();
    let (iface, device, mut controller, sockets) =
        create_network_interface(wifi, WifiMode::Sta, &mut socket_set_entries);
    let wifi_stack = WifiStack::new(iface, device, sockets, current_millis);

    use esp32_hal::timer::TimerGroup;
    let timer = TimerGroup::new(peripherals.TIMG1, &clocks).timer0;
    esp_wifi::initialize(
        timer,
        Rng::new(peripherals.RNG),
        system.radio_clock_control,
        &clocks,
    )
    .unwrap();

    let client_config = Configuration::Client(ClientConfiguration {
        ssid: SSID.into(),
        password: PASSWORD.into(),
        ..Default::default()
    });
    let _res = controller.set_configuration(&client_config);
    controller.start().unwrap();
    println!("wifi_connect {:?}", controller.connect());

    loop {
        let res = controller.is_connected();
        match res {
            Ok(connected) => {
                if connected {
                    break;
                }
            }
            Err(err) => {
                println!("Err: {:?}", err);
                loop {}
            }
        }
    }
    println!("Wait to get an ip address");
    loop {
        wifi_stack.work();

        if wifi_stack.is_iface_up() {
            println!("got ip {:?}", wifi_stack.get_ip_info());
            break;
        }
    }

    let mut rx_buffer = [0u8; 1536];
    let mut tx_buffer = [0u8; 1536];
    let mut socket = wifi_stack.get_socket(&mut rx_buffer, &mut tx_buffer);

    loop {
        println!("Post to Nostr relay");
        socket.work();
        socket.open(Ipv4Address::new(192, 168, 0, 5), 7000).unwrap();

        // establish web socket connection
        socket
            .write(b"GET / HTTP/1.1\r\nAccept: text/html\r\nHost: 192.168.0.5:7000\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n")
            .expect("write err");

        socket.flush().expect("flush err");
        println!();

        let wait_end = current_millis() + 20 * 1000;
        loop {
            let mut buffer = [0u8; 512];
            if let Ok(len) = socket.read(&mut buffer) {
                let to_print = unsafe { core::str::from_utf8_unchecked(&buffer[..len]) };
                print!("{}", to_print);
            } else {
                break;
            }

            if current_millis() > wait_end {
                println!("Timeout");
                break;
            }
        }
        println!();
        println!("Connection");

        // Send note
        let mut note = nostr::Note::new("hello world");
        let output = &note.to_signed(PRIVKEY);
        let to_print = unsafe { core::str::from_utf8_unchecked(&output[..1536]) };
        print!("{}", to_print);

        socket.write(&note.to_signed(PRIVKEY)).expect("write err");

        socket.flush().expect("flush err");
        println!();

        let wait_end = current_millis() + 20 * 1000;
        loop {
            let mut buffer = [0u8; 512];
            if let Ok(len) = socket.read(&mut buffer) {
                let to_print = unsafe { core::str::from_utf8_unchecked(&buffer[..len]) };
                print!("{}", to_print);
            } else {
                break;
            }

            if current_millis() > wait_end {
                println!("Timeout");
                break;
            }
        }
        println!();

        socket.disconnect();

        println!("Break quick");
        let wait_end = current_millis() + 20 * 1000;
        while current_millis() < wait_end {
            socket.work();
        }
    }
}
