#![no_std]
#![no_main]
use embedded_svc::ipv4::Interface;
use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};
use embedded_websocket::framer::Framer;
use embedded_websocket::{
    EmptyRng, WebSocketClient, WebSocketOptions, WebSocketSendMessageType, WebSocketState,
};
use esp32_hal::clock::{ClockControl, CpuClock};
use esp32_hal::Rng;
use esp32_hal::{peripherals::Peripherals, prelude::*, Rtc};

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
    let clocks = ClockControl::configure(system.clock_control, CpuClock::Clock240MHz).freeze();

    // Disable MWDT and RWDT (Watchdog) flash boot protection
    let mut rtc = Rtc::new(peripherals.RTC_CNTL);
    let timer_group0 = TimerGroup::new(
        peripherals.TIMG0,
        &clocks,
        &mut system.peripheral_clock_control,
    );
    let mut wdt0 = timer_group0.wdt;
    let timer = TimerGroup::new(
        peripherals.TIMG1,
        &clocks,
        &mut system.peripheral_clock_control,
    )
    .timer0;
    rtc.rwdt.disable();
    wdt0.disable();

    println!("Starting up");
    let (wifi, _) = peripherals.RADIO.split();
    let mut socket_set_entries: [SocketStorage; 3] = Default::default();
    let (iface, device, mut controller, sockets) =
        create_network_interface(wifi, WifiMode::Sta, &mut socket_set_entries);
    let wifi_stack = WifiStack::new(iface, device, sockets, current_millis);

    // Iniitalize wifi
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
        host: "192.168.0.4:7000",
        origin: "",
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
        network::NetworkConnection::new(socket, Ipv4Address::new(192, 168, 0, 4), 7000).unwrap();
    framer
        .connect(&mut stream, &websocket_options)
        .expect("connection error");

    let state = framer.state();
    println!("state: {:?}", state);

    // create a note
    let msg = br#"["EVENT",{"content":"hello","created_at":1687035119,"id":"7648eb0b7aa54e7fc6673fd8c02f818ad135bd9d0fd346a2cd27c3adc885117c","kind":1,"pubkey":"098ef66bce60dd4cf10b4ae5949d1ec6dd777ddeb4bc49b47f97275a127a63cf","sig":"898374a5a18087e304efc07c454d8ef50afa9bfb1514ad5507d59ab76e5c1ed8e7a0ecef888057e05724ccb8d718ca81b409dd6ce6cdbeda9c54e8eb07aab4e3","tags":[]}]"#;
    let msg1 = br#"["EVENT",{"content":"esptest","created_at":1686880020,"id":"1a892186182fc21b33dab71c62b9aeab2df926b905db7e10e671b65d78e6a019","kind":1,"pubkey":"098ef66bce60dd4cf10b4ae5949d1ec6dd777ddeb4bc49b47f97275a127a63cf","sig":"eca27038afc8b1946acfcb3ace9ef4885b15b008507c0e84ea782b3dc222b8f9f1ebfd10c67a57d750315afaef8a77e93cc00836e29d6f662482fb43a93c14b4","tags":[]}]"#;
    let note = nostr::Note::new(PRIVKEY, "esptest");
    let msg = note.to_relay();
    let msg = msg[0..360].as_ref();
    for i in 0..359 {
        if msg[i] != msg1[i] {
            println!("{} {} {}", i, msg[i], msg1[i]);
        }
    }
    framer
        .write(&mut stream, WebSocketSendMessageType::Text, true, &msg)
        .expect("framer write fail");

    while framer.state() == WebSocketState::Open {
        match framer.read(&mut stream, &mut frame_buf) {
            Ok(s) => {
                // framer
                //     .close(&mut stream, WebSocketCloseStatusCode::NormalClosure, None)
                //     .map_err(|e| {
                //         println!("{:?}", e);
                //     });
                // println!("Sent close handshake");
            }
            Err(e) => {
                println!("{:?}", e);
            }
        }
    }

    println!("Connection closed");
    loop {}
}
