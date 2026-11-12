# ESP32-S3 firmware

Back to the [README](../README.md).

`firmware/` runs the same driver on an ESP32-S3 (ESP32-S3-DevKitC-1) with ESP-IDF (Rust std). It polls the adapter over the board's UART with the same polling loop as the CLI (`session::run`) and writes the readings to the ESP-IDF log. No real ELM327 is wired to it yet: a USB-UART adapter stands in for one, so the board talks UART and the emulator answers from the computer at the other end of the cable.

It needs the Xtensa Rust toolchain and two tools:

```sh
cargo install --locked espup ldproxy cargo-espflash
espup install                      # then load ~/export-esp.sh in your shell profile
```

## Wiring

UART0 is the console and the flashing port, so the firmware uses UART1, at 38400 baud (the ELM327 default):

| ESP32-S3    | USB-UART adapter |
|-------------|------------------|
| GPIO17 (TX) | RX               |
| GPIO18 (RX) | TX               |
| GND         | GND              |

The ESP32-S3 pins are 3.3 V only. Set the adapter to 3.3 V logic (mine, an FT232RL board, has a jumper for it) and leave its VCC pin unconnected.

## Running it

Start the emulator on the adapter's serial port:

```sh
elm -p /dev/cu.usbserial-XXXX -a 38400
```

On macOS use the `/dev/cu.*` device: opening `/dev/tty.*` waits for the carrier-detect line.

Then build, flash and open the monitor:

```sh
cd firmware
cargo espflash flash --monitor --baud 921600
```

```
W (4107) firmware: PID FuelTankLevel: not supported by vehicle
I (4167) firmware: 620 rpm
I (4177) firmware: 4 km/h
I (4177) firmware: 83 °C
I (4177) firmware: 100.0 %
I (4177) firmware: ----------------
```

- The first build downloads ESP-IDF v6.1 and its tools into `firmware/.embuild/`, so it takes a while.
- Flash with `cargo espflash`, not `cargo run`. It writes the bootloader and partition table built by ESP-IDF together with the app. With Wi-Fi the app is bigger than 1 MB, so the firmware uses ESP-IDF's large single-app partition table.

## Examples

`firmware/examples/` has three more programs. Flash one with `--example`, for example `cargo espflash flash --monitor --baud 921600 --example uart_at`.

| Example     | What it does |
|-------------|--------------|
| `uart_echo` | Checks the wiring. It echoes every byte it receives and sends `esp32 alive` when nothing arrives for 1 s. Open the adapter's port with a serial terminal (`screen /dev/cu.usbserial-XXXX 38400`) and type: the text shows up in the board's log and comes back to the terminal. |
| `uart_at`   | Checks the link to the emulator or an adapter. It sends `ATI` every 2 s and logs the answer. |
| `wifi_tcp`  | Polls the emulator over Wi-Fi and TCP instead, with obdient's `TcpTransport`. |

`wifi_tcp` reads the Wi-Fi credentials and the emulator's address at compile time from the environment. Put them in `firmware/.env`, which git ignores:

```sh
WIFI_SSID=...
WIFI_PASS=...
OBD_TCP_ADDRESS=192.168.1.20:35000   # the computer running the emulator
```

Start the emulator with `elm -n 35000`, then:

```sh
cd firmware
set -a; source .env; set +a
cargo espflash flash --monitor --baud 921600 --example wifi_tcp
```

The ESP32-S3 only does 2.4 GHz Wi-Fi, and the emulator has to accept connections from the network, not only from `localhost`.

## UART transport

`UartTransport` (`firmware/src/transport/uart.rs`) implements obdient's `Transport` trait over the ESP-IDF UART driver, so the protocol, poller and session code didn't change. It lives in the firmware because that driver only exists on the ESP32. A read that gets nothing within the timeout (`ESP_ERR_TIMEOUT`) becomes `TransportError::Timeout`. There's no connection to reopen, so `reconnect` only clears the receive buffer.

## What I checked on the board

- Over UART it logs the same readings as `wifi_tcp` against the same emulator, cycle by cycle, and reports when the vehicle stops responding (`engineoff`).
- Over Wi-Fi it also reads the same values as the CLI, reconnects when the emulator is stopped and started again, and gives up on a connect after 5 s.
- A UART has no connection, so it can't tell that the other side is gone. Unplugging the USB-UART adapter only makes the line go silent: every PID fails with `response timeout: ""`, the firmware never prints `connection lost`, and it keeps polling, with each cycle taking about 52 s (see Retries in [design](design.md#decisions)). Once the adapter answers again, the readings come back on their own, without a reset. Detecting a dead adapter sooner is still an open question.
- Stopping the emulator for a few seconds doesn't show up in the log at all: the retries absorb it, and one cycle takes about 9 s instead of 1 s.
- The emulator doesn't exit when its serial device disappears: it keeps looping on read errors. After plugging the adapter back in, stop it and start it again.
