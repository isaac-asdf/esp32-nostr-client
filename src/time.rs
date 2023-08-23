// use esp32_hal::{clock::Clocks, prelude::_embedded_hal_blocking_delay_DelayMs, Delay};
// use esp_println::println;
// use esp_wifi::wifi_interface::{UdpSocket, WifiStack};
// use smoltcp::wire::Ipv4Address;

// pub fn get_utc_time(wifi_stack: &WifiStack, clocks: &Clocks) -> u32 {
//     let mut rx_meta1 = [smoltcp::socket::udp::PacketMetadata::EMPTY; 10];
//     let mut rx_buffer1 = [0u8; 1536];
//     let mut tx_meta1 = [smoltcp::socket::udp::PacketMetadata::EMPTY; 10];
//     let mut tx_buffer1 = [0u8; 1536];
//     let mut udp_socket = wifi_stack.get_udp_socket(
//         &mut rx_meta1,
//         &mut rx_buffer1,
//         &mut tx_meta1,
//         &mut tx_buffer1,
//     );
//     udp_socket.bind(50123).unwrap();

//     let req_data = ntp_nostd::get_client_request();
//     let mut rcvd_data = [0_u8; 1536];

//     udp_socket
//         .send(Ipv4Address::new(18, 119, 130, 247).into(), 123, &req_data)
//         .unwrap();
//     let mut count = 0;

//     let mut delay = Delay::new(&clocks);
//     loop {
//         count += 1;
//         let rcvd = udp_socket.receive(&mut rcvd_data);
//         if let Ok(rcvd) = rcvd {
//             println!("received {} from {}", rcvd.0, rcvd.1);
//             break;
//         }

//         delay.delay_ms(500_u32);

//         if count > 10 {
//             udp_socket
//                 .send(Ipv4Address::new(18, 119, 130, 247).into(), 123, &req_data)
//                 .unwrap();
//             println!("reset count again...");
//             count = 0;
//         }
//     }
//     let response = ntp_nostd::NtpServerResponse::from(rcvd_data.as_ref());
//     if response.headers.tx_time_seconds == 0 {
//         panic!("No timestamp received");
//     }

//     let utc = response.headers.get_unix_timestamp();
//     utc
// }
