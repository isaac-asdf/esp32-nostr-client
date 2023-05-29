#![no_std]
#![no_main]
use embedded_svc::ipv4::Interface;
use embedded_svc::wifi::{AccessPointInfo, ClientConfiguration, Configuration, Wifi};
use embedded_websocket::framer::{Framer, ReadResult};
use embedded_websocket::{
    EmptyRng, WebSocketClient, WebSocketCloseStatusCode, WebSocketOptions, WebSocketSendMessageType,
};
use esp32_hal::clock::{ClockControl, CpuClock};
use esp32_hal::Rng;
use esp32_hal::{peripherals::Peripherals, prelude::*, Rtc};

use esp_hal_common::sha::{Sha, ShaMode};
use esp_hal_common::timer::TimerGroup;
use esp_println::logger::init_logger;
use esp_println::println;
use esp_wifi::current_millis;
use esp_wifi::wifi::utils::create_network_interface;
use esp_wifi::wifi::WifiMode;
use esp_wifi::wifi_interface::WifiStack;
use smoltcp::iface::SocketStorage;

use esp_backtrace as _;
use smoltcp::wire::Ipv4Address;

mod network;
mod nostr;

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PWD");
const PRIVKEY: &str = env!("PRIVKEY");
// const SSID: &str = "";
// const PASSWORD: &str = "";
// const PRIVKEY: &str = "";

#[entry]
fn main() -> ! {
    init_logger(log::LevelFilter::Info);
    let peripherals = Peripherals::take();
    let mut system = peripherals.DPORT.split();
    // let clocks = ClockControl::boot_defaults(system.clock_control).freeze();
    let clocks = ClockControl::configure(system.clock_control, CpuClock::Clock240MHz).freeze();

    let timer_group0 = TimerGroup::new(
        peripherals.TIMG0,
        &clocks,
        &mut system.peripheral_clock_control,
    );
    let mut wdt = timer_group0.wdt;
    let mut rtc = Rtc::new(peripherals.RTC_CNTL);

    // Disable MWDT and RWDT (Watchdog) flash boot protection
    wdt.disable();
    rtc.rwdt.disable();

    let mut hasher = Sha::new(
        peripherals.SHA,
        ShaMode::SHA512,
        &mut system.peripheral_clock_control,
    );

    println!("Starting up");
    let (wifi, _) = peripherals.RADIO.split();
    let mut socket_set_entries: [SocketStorage; 3] = Default::default();
    let (iface, device, mut controller, sockets) =
        create_network_interface(wifi, WifiMode::Sta, &mut socket_set_entries);
    let wifi_stack = WifiStack::new(iface, device, sockets, current_millis);

    // create a note
    let sha = peripherals.SHA;
    let mut hasher = Sha::new(sha, ShaMode::SHA256, &mut system.peripheral_clock_control);
    let mut note = nostr::Note::new(PRIVKEY, "esptest", hasher);
    println!("note created");

    let timer = TimerGroup::new(
        peripherals.TIMG1,
        &clocks,
        &mut system.peripheral_clock_control,
    )
    .timer0;
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
    let res = controller.set_configuration(&client_config);
    println!("wifi_set_configuration returned {:?}", res);
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

    // Main working loop
    println!("Post to Nostr relay");
    let mut websocket = WebSocketClient::new_client(EmptyRng::new());
    // initiate a websocket opening handshake
    let websocket_options = WebSocketOptions {
        path: "/",
        host: "192.168.0.5",
        origin: "http://192.168.5.0:7000",
        sub_protocols: None,
        additional_headers: None,
    };

    let mut read_buf = [0; 4000];
    let mut read_cursor = 0;
    let mut write_buf = [0; 4000];
    let mut frame_buf = [0; 4000];
    let mut framer = Framer::new(
        &mut read_buf,
        &mut read_cursor,
        &mut write_buf,
        &mut websocket,
    );

    // set up connection
    let mut stream =
        network::NetworkConnection::new(socket, Ipv4Address::new(192, 168, 0, 5), 7000).unwrap();
    framer
        .connect(&mut stream, &websocket_options)
        .expect("connection error");

    println!("connected");
    framer
        .write(
            &mut stream,
            WebSocketSendMessageType::Text,
            true,
            &note.to_relay(),
        )
        .expect("framer write fail");
    println!("written?");

    while let ReadResult::Text(s) = framer.read(&mut stream, &mut frame_buf).unwrap() {
        println!("Received: {}", s);

        // close the websocket after receiving the first reply
        framer
            .close(&mut stream, WebSocketCloseStatusCode::NormalClosure, None)
            .unwrap();
        println!("Sent close handshake");
    }

    println!("Connection closed");
    loop {}
}
