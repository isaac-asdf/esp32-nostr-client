#![no_std]
#![no_main]

use embedded_svc::ipv4::Interface;
use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};
use embedded_websocket::framer::Framer;
use embedded_websocket::{EmptyRng, WebSocketClient, WebSocketOptions};
use esp32_hal::clock::{ClockControl, CpuClock};
use esp32_hal::timer::TimerGroup;
use esp32_hal::{peripherals::Peripherals, prelude::*, Rtc};
use esp32_hal::{Delay, Rng};

use esp_println::logger::init_logger;
use esp_println::println;
use esp_wifi::wifi::utils::create_network_interface;
use esp_wifi::wifi::WifiMode;
use esp_wifi::wifi_interface::WifiStack;
use esp_wifi::{current_millis, initialize, EspWifiInitFor};
use log::info;

use smoltcp::iface::SocketStorage;
use smoltcp::wire::Ipv4Address;

use esp_backtrace as _;

// use nostr_nostd as nostr;
mod network;
mod time;

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PWD");
const _PRIVKEY: &str = env!("PRIVKEY");
const BUFFER_SIZE: usize = 4000;

#[entry]
fn main() -> ! {
    init_logger(log::LevelFilter::Info);

    // get peripherals
    let peripherals = Peripherals::take();
    let mut system = peripherals.DPORT.split();
    let mut rtc = Rtc::new(peripherals.RTC_CNTL);
    let tg0 = peripherals.TIMG0;
    let tg1 = peripherals.TIMG1;
    let rng = Rng::new(peripherals.RNG);
    let (wifi, _) = peripherals.RADIO.split();

    // Disable MWDT and RWDT (Watchdog) flash boot protection
    let clocks = ClockControl::configure(system.clock_control, CpuClock::Clock240MHz).freeze();
    let timer_group0 = TimerGroup::new(tg0, &clocks, &mut system.peripheral_clock_control);
    let mut wdt0 = timer_group0.wdt;
    let timer = TimerGroup::new(tg1, &clocks, &mut system.peripheral_clock_control).timer0;
    rtc.rwdt.disable();
    wdt0.disable();

    let init = initialize(
        EspWifiInitFor::Wifi,
        timer,
        rng,
        system.radio_clock_control,
        &clocks,
    )
    .unwrap();

    info!("Starting up");
    let mut socket_set_entries: [SocketStorage; 3] = Default::default();
    let (iface, device, mut controller, sockets) =
        create_network_interface(&init, wifi, WifiMode::Sta, &mut socket_set_entries).unwrap();
    let wifi_stack = WifiStack::new(iface, device, sockets, current_millis);

    let client_config = Configuration::Client(ClientConfiguration {
        ssid: SSID.into(),
        password: PASSWORD.into(),
        ..Default::default()
    });
    let res = controller.set_configuration(&client_config);
    info!("wifi_set_configuration returned {:?}", res);

    controller.start().unwrap();
    info!("is wifi started: {:?}", controller.is_started());

    info!("{:?}", controller.get_capabilities());
    info!("wifi_connect {:?}", controller.connect());

    loop {
        let res = controller.is_connected();
        match res {
            Ok(connected) => {
                if connected {
                    break;
                }
            }
            Err(err) => {
                println!("{:?}", err);
                loop {}
            }
        }
    }
    info!("connected: {:?}", controller.is_connected());

    // wait for getting an ip address
    info!("Wait to get an ip address");
    loop {
        wifi_stack.work();

        if wifi_stack.is_iface_up() {
            info!("got ip {:?}", wifi_stack.get_ip_info());
            break;
        }
    }

    // get tcp socket for nostr comms
    let mut rx_buffer = [0u8; 1536];
    let mut tx_buffer = [0u8; 1536];
    let socket = wifi_stack.get_socket(&mut rx_buffer, &mut tx_buffer);

    // get udp socket for ntp time stamps
    println!("Get NTP time");
    let mut rx_meta1 = [smoltcp::socket::udp::PacketMetadata::EMPTY; 10];
    let mut rx_buffer1 = [0u8; 1536];
    let mut tx_meta1 = [smoltcp::socket::udp::PacketMetadata::EMPTY; 10];
    let mut tx_buffer1 = [0u8; 1536];
    let mut udp_socket = wifi_stack.get_udp_socket(
        &mut rx_meta1,
        &mut rx_buffer1,
        &mut tx_meta1,
        &mut tx_buffer1,
    );
    udp_socket.bind(50123).unwrap();

    let req_data = ntp_nostd::get_client_request();
    let mut rcvd_data = [0_u8; 1536];

    udp_socket
        .send(Ipv4Address::new(18, 119, 130, 247).into(), 123, &req_data)
        .unwrap();
    let mut count = 0;

    let mut delay = Delay::new(&clocks);
    loop {
        count += 1;
        let rcvd = udp_socket.receive(&mut rcvd_data);
        if rcvd.is_ok() {
            break;
        }

        // delay to wait for data to show up to port
        delay.delay_ms(500_u32);

        if count > 10 {
            udp_socket
                .send(Ipv4Address::new(18, 119, 130, 247).into(), 123, &req_data)
                .unwrap();
            println!("reset count again...");
            count = 0;
        }
    }
    let response = ntp_nostd::NtpServerResponse::from(rcvd_data.as_ref());
    if response.headers.tx_time_seconds == 0 {
        panic!("No timestamp received");
    }

    println!("Current unix: {:?}", response.headers.get_unix_timestamp());

    // initiate a websocket opening handshake
    let mut websocket = WebSocketClient::new_client(EmptyRng::new());
    let websocket_options = WebSocketOptions {
        path: "/",
        host: "192.168.1.3:7000",
        origin: "",
        sub_protocols: None,
        additional_headers: None,
    };

    let mut read_buf = [0; BUFFER_SIZE];
    let mut read_cursor = 0;
    let mut write_buf = [0; BUFFER_SIZE];
    let mut frame_buf = [0; BUFFER_SIZE];
    let mut framer = Framer::new(
        &mut read_buf,
        &mut read_cursor,
        &mut write_buf,
        &mut websocket,
    );

    println!("Connect to Nostr relay");
    // set up connection
    let mut stream =
        network::NetworkConnection::new(socket, Ipv4Address::new(192, 168, 1, 3), 7000).unwrap();
    framer
        .connect(&mut stream, &websocket_options)
        .expect("connection error");

    let state = framer.state();
    println!("state: {:?}", state);

    // framer
    //     .write(&mut stream, WebSocketSendMessageType::Text, true, &msg)
    //     .expect("framer write fail");

    // while framer.state() == WebSocketState::Open {
    //     match framer.read(&mut stream, &mut frame_buf) {
    //         Ok(_) => {
    //             //
    //         }
    //         Err(e) => {
    //             println!("{:?}", e);
    //         }
    //     }
    // }

    println!("Connection closed");
    loop {}
}
