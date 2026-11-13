# obdient

A small Rust driver for ELM327 OBD-II adapters. It connects to the adapter, runs the usual AT setup and reads live engine data in a loop.

I started it as a way to learn how to talk to a real device from Rust. Most of it is the boring-but-important part: framing responses, timeouts, and not getting confused when the adapter misbehaves.

## Status

Early, but it works end to end:

- **Emulator:** polls [ELM327-emulator](https://github.com/Ircama/ELM327-emulator) over TCP.
- **Real car:** reads live data from a 2012 Volkswagen (diesel) through a USB ELM327 adapter and the serial transport. See [real hardware](docs/real-hardware.md).
- **ESP32-S3:** the same driver runs on the board. It polls the emulator over UART through a USB-UART adapter, and the Volkswagen through that same adapter, bridged on the laptop to the USB ELM327. A Wi-Fi example polls the emulator over TCP. See [firmware](docs/firmware.md).
- **Not yet:** a UART ELM327 wired straight to the board, and a second car.

## What it reads

| PID    | Value               | Unit |
|--------|---------------------|------|
| `010C` | Engine speed        | rpm  |
| `010D` | Vehicle speed       | km/h |
| `0105` | Coolant temperature | °C   |
| `0111` | Throttle position   | %    |
| `012F` | Fuel tank level     | %    |

At startup the driver reads which PIDs the vehicle supports and only polls those. A PID it doesn't announce is reported once and skipped. The emulator doesn't announce the fuel level, so it's missing from the output below.

## Quick start

You need Rust 1.85 or newer (the crate uses edition 2024). The easiest way to try it is the emulator, which speaks the same protocol as a real adapter:

```sh
uv tool install ELM327-emulator    # or: pip install ELM327-emulator
elm -n 35000                       # listens on TCP port 35000
```

Then, in another terminal:

```sh
cd obdient
cargo run -- --tcp 127.0.0.1:35000
```

```
620 rpm
4 km/h
83 °C
100.0 %
```

For a serial adapter, pass the device path instead, for example `cargo run -- --serial /dev/ttyUSB0`.

| Option              | Default  | Description                                  |
|---------------------|----------|----------------------------------------------|
| `--tcp <ADDRESS>`   | —        | Adapter address, `host:port`                 |
| `--serial <PATH>`   | —        | Serial device path                           |
| `--baud <N>`        | `38400`  | Serial baud rate                             |
| `--interval-ms <N>` | `1000`   | Pause between polling cycles (ms)            |

Pass exactly one of `--tcp` or `--serial`. On macOS `serialport` can't open the emulator's pseudo-terminal, so use TCP there. Run `elm -n 35000 -s car` for values that change.

## ESP32-S3 firmware

`firmware/` runs the same driver on an ESP32-S3 with ESP-IDF (Rust std) and writes the readings to the ESP-IDF log. It talks to the adapter over UART1, with 3.3 V logic at 38400 baud:

| ESP32-S3    | Adapter |
|-------------|---------|
| GPIO17 (TX) | RX      |
| GPIO18 (RX) | TX      |
| GND         | GND     |

```sh
elm -p /dev/cu.usbserial-XXXX -a 38400   # the emulator on the USB-UART adapter's port
cd firmware
cargo espflash flash --monitor --baud 921600
```

Toolchain setup, the examples (wiring check, AT probe, Wi-Fi) and what I checked on the board are in [docs/firmware.md](docs/firmware.md).

## How it works

The code is split into layers, each of which only knows about the one below:

```
main.rs     CLI and printing
session.rs  the polling loop shared by the CLI and the firmware: setup, poll, reconnect
poller.rs   startup wait, one polling cycle (retries, what to do with each error) and reconnection
elm327.rs   ELM327 protocol: AT setup, send a command, read the answer
pid.rs      parse the text response and decode the value
transport   moves bytes (TCP or serial)
```

- **Transport** is a trait (`read`, `write_all`, `reconnect`), so TCP, serial and the firmware's UART plug in without touching the protocol code.
- **The library never prints.** `session::run` reports what happens as events, and the CLI and the firmware each decide how to show them.
- **Errors have a kind** (transient, PID unavailable, vehicle unavailable, connection lost), and the poller acts on it: retry with backoff, skip the PID, wait for the vehicle or reconnect.
- **Framing and resync.** It reads until the `>` prompt, with a deadline, and drains a late answer before the next command so it isn't taken as the answer to the new one.

Details and the reasoning behind each decision: [docs/design.md](docs/design.md).

## Tests

```sh
cd obdient
cargo test
```

The unit tests use a mock transport, so they need no hardware. The serial tests (Linux pseudo-terminals, with a Docker runner) and the tests against the emulator are in [docs/testing.md](docs/testing.md).

## Roadmap

- [x] Serial transport, for USB adapters and the emulator's pseudo-terminal
- [x] Retry policy: bounded retries with backoff, driven by the error kind
- [x] Read the supported PIDs from the ECU (`0100`) at startup
- [x] Reconnect when the adapter goes away
- [x] Connection timeout for Wi-Fi adapters
- [x] Try it with a real ELM327 adapter on a car (USB, 2012 Volkswagen)
- [ ] Try it on a second car (2004 Ford)
- [x] Run it on my ESP32-S3 over Wi-Fi against the emulator
- [x] Run it on my ESP32-S3 over UART against the emulator, through a USB-UART adapter
- [x] Run it on my ESP32-S3 talking to a real ELM327 over UART, bridged through the laptop
- [ ] Wire a UART ELM327 straight to the ESP32-S3

## License

MIT, see [LICENSE](LICENSE).
