#![no_std]
#![no_main]

use embedded_svc::ipv4::Interface;
use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};
use embedded_websocket::framer::Framer;
use embedded_websocket::{
    EmptyRng, WebSocketClient, WebSocketOptions, WebSocketSendMessageType, WebSocketState,
};
use esp32_hal::clock::{ClockControl, CpuClock};
use esp32_hal::i2c::I2C;
use esp32_hal::timer::TimerGroup;
use esp32_hal::{peripherals::Peripherals, prelude::*, Rtc};
use esp32_hal::{Delay, Rng, IO};

use esp_println::logger::init_logger;
use esp_println::println;
use esp_wifi::wifi::utils::create_network_interface;
use esp_wifi::wifi::WifiMode;
use esp_wifi::wifi_interface::WifiStack;
use esp_wifi::{current_millis, initialize, EspWifiInitFor};
use log::info;

use nostr::relay_responses::ResponseTypes;
use smoltcp::iface::SocketStorage;
use smoltcp::wire::Ipv4Address;

use esp_backtrace as _;

use nostr_nostd as nostr;
mod network;
mod time;

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PWD");
const PRIVKEY: &str = env!("PRIVKEY");
const BUFFER_SIZE: usize = 4000;

static mut RTC_OFFSET: u64 = 0;
static mut UTC_TIME: u32 = 0;

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
    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

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

    // // Set GPIO2 as an output, and set its state high initially.
    // let mut led = io.pins.gpio32.into_push_pull_output();
    // led.set_high().unwrap();
    // let mut led = io.pins.gpio33.into_push_pull_output();
    // led.set_high().unwrap();

    // Create a new peripheral object with the described wiring
    // and standard I2C clock speed
    // The following wiring is assumed:
    // - SDA => GPIO32
    // - SCL => GPIO33
    let mut i2c = I2C::new(
        peripherals.I2C0,
        io.pins.gpio32,
        io.pins.gpio33,
        100u32.kHz(),
        &mut system.peripheral_clock_control,
        &clocks,
    );
    info!("Starting up");
    let mut delay = Delay::new(&clocks);
    loop {
        delay.delay_ms(1000u32);
        let mut data = [0u8; 22];
        // i2c.read(0xaa, &mut data).ok();
        i2c.write_read(0x77, &[0xaa], &mut data).ok();

        println!("Cal data: {:02x?}", data);
    }

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
        // using ip from https://tf.nist.gov/tf-cgi/servers.cgi (time-a-g.nist.gov)
        .send(Ipv4Address::new(129, 6, 15, 28).into(), 123, &req_data)
        .unwrap();
    let mut count = 0;

    loop {
        count += 1;
        let rcvd = udp_socket.receive(&mut rcvd_data);
        if rcvd.is_ok() {
            unsafe {
                // set global static offset variable
                RTC_OFFSET = rtc.get_time_ms();
            }
            break;
        }

        // delay to wait for data to show up to port
        delay.delay_ms(500_u32);

        if count > 10 {
            udp_socket
                // retry with another server
                // using ip from https://tf.nist.gov/tf-cgi/servers.cgi (time-b-g.nist.gov)
                .send(Ipv4Address::new(129, 6, 15, 29).into(), 123, &req_data)
                .unwrap();
            info!("reset ntp count...");
            count = 0;
        }
    }
    let response = ntp_nostd::NtpServerResponse::from(rcvd_data.as_ref());
    if response.headers.tx_time_seconds == 0 {
        panic!("No timestamp received");
    }
    unsafe {
        UTC_TIME = response.headers.get_unix_timestamp();
    }

    // initiate a websocket opening handshake
    let mut websocket = WebSocketClient::new_client(EmptyRng::new());
    let websocket_options = WebSocketOptions {
        path: "/",
        host: "192.168.0.24:7000",
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
        network::NetworkConnection::new(socket, Ipv4Address::new(192, 168, 0, 24), 7000).unwrap();
    framer
        .connect(&mut stream, &websocket_options)
        .expect("connection error");

    loop {
        println!("starting a new message");
        let now_as_unix = unsafe { get_utc_timestamp(&rtc) };
        let msg = nostr::Note::new_builder(PRIVKEY)
            .unwrap()
            .content("hello world".into())
            .add_tag("g,geohash".into())
            .build(now_as_unix, [0; 32])
            .unwrap()
            .serialize_to_relay(nostr::ClientMsgKinds::Event);

        framer
            .write(&mut stream, WebSocketSendMessageType::Text, true, &msg)
            .expect("framer write fail");

        while framer.state() == WebSocketState::Open {
            match framer.read(&mut stream, &mut frame_buf) {
                Ok(response) => {
                    match response {
                        embedded_websocket::framer::ReadResult::Text(res) => {
                            match nostr::relay_responses::ResponseTypes::try_from(res).unwrap() {
                                ResponseTypes::Ok => {
                                    /* message parsed, could be accepted or not accepted */
                                    // display message for debugging purposes
                                    println!("{res}");
                                }
                                ResponseTypes::Notice => {
                                    /* message formatted improperly or unreadable by relay */
                                    // display message for debugging purposes
                                    println!("{res}");
                                }
                                // none of the below should be seen at this point
                                ResponseTypes::Auth => {}
                                ResponseTypes::Eose => {}
                                ResponseTypes::Count => {}
                                ResponseTypes::Event => {}
                            };
                        }
                        embedded_websocket::framer::ReadResult::Closed => (),
                        _ => {
                            // binary or pong, which we don't expect
                        }
                    };
                    break;
                }
                Err(e) => {
                    println!("{:?}", e);
                }
            }
        }
        delay.delay_ms(10_000_u32);
    }
}

unsafe fn get_utc_timestamp(rtc: &Rtc) -> u32 {
    let time_now = rtc.get_time_ms() / 1000;
    let rtc_offset_s = RTC_OFFSET / 1000;
    UTC_TIME + u32::try_from(time_now - rtc_offset_s).unwrap()
}
