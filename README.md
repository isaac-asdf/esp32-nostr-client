# IoT Demo for ESP32, Nostr

## Link libs

`. $HOME/export-esp.sh`

## Setup

- Install all esp32 specific stuff [link](https://esp-rs.github.io/book/installation/installation.html)
- Install libsecp, used for note signing [link](https://github.com/bitcoin-core/secp256k1)
    - be sure to use `./configure --enable-module-schnorrsig --host=xtensa-esp32` to be compatible with esp32
