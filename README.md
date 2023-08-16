# IoT Demo for ESP32, Nostr

## Link libs

`. $HOME/export-esp.sh`

## Setup

- Install all esp32 specific stuff [link](https://esp-rs.github.io/book/installation/installation.html)
- When running, export variables for `cc` linker which is used by `rust-bitcoin`
  - export CC=xtensa-esp32-elf-gcc
  - export AR=xtensa-esp32-elf-ar

## Bash script for making the above easier

Create a bash script that looks like the below to make setup easier:

```
. $HOME/export-esp.sh
export CC=xtensa-esp32-elf-gcc
export AR=xtensa-esp32-elf-ar
export SSID="wifi_name"
export PWD="wifi_password"
export PRIVKEY="a5084b35a58e3e1a26f5efb46cb9dbada73191526aa6d11bccb590cbeb2d8fa3"
cargo run --release
```
