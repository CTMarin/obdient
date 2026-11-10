# obdient

A small Rust driver for ELM327 OBD-II adapters. It connects to the adapter, runs the usual AT setup and reads live engine data in a loop.

I started it as a way to learn how to talk to a real device from Rust. Most of it is the boring-but-important part: framing responses, timeouts, and not getting confused when the adapter misbehaves.

## Status

Early, but it works end to end over TCP against [ELM327-emulator](https://github.com/Ircama/ELM327-emulator). There is now a serial transport too. I tested it against pseudo-terminals on Linux, but I haven't run it against the emulator or a physical adapter yet. On macOS `serialport` can't open a pseudo-terminal (see [Usage](#usage)). The same driver also runs on an ESP32-S3: over Wi-Fi it polls the emulator, reconnects when the emulator restarts and respects the connect timeout (see [ESP32-S3 firmware](#esp32-s3-firmware)). It doesn't talk to an adapter over UART yet. See the [roadmap](#roadmap).

## What it reads

| PID    | Value               | Unit |
|--------|---------------------|------|
| `010C` | Engine speed        | rpm  |
| `010D` | Vehicle speed       | km/h |
| `0105` | Coolant temperature | °C   |
| `0111` | Throttle position   | %    |
| `012F` | Fuel tank level     | %    |

At startup the driver reads which PIDs the vehicle supports and only polls those. A PID it doesn't announce is reported once and skipped. The emulator doesn't announce the fuel level, so it's missing from the output below.

## Usage

You need Rust 1.85 or newer (the crate uses edition 2024).

The easiest way to try it is the emulator, which speaks the same protocol as a real adapter:

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

For a serial adapter, pass the device path instead:

```sh
cargo run -- --serial /dev/ttyUSB0
```

| Option              | Default  | Description                                  |
|---------------------|----------|----------------------------------------------|
| `--tcp <ADDRESS>`   | —        | Adapter address, `host:port`                 |
| `--serial <PATH>`   | —        | Serial device path                           |
| `--baud <N>`        | `38400`  | Serial baud rate                             |
| `--interval-ms <N>` | `1000`   | Pause between polling cycles (ms)            |

Pass exactly one of `--tcp` or `--serial`.

The emulator also offers a pseudo-terminal when you start it without `-n`. On macOS, `serialport` can't open pseudo-terminals (the open fails with ENOTTY, "Not a typewriter"), so use TCP there.

If the vehicle doesn't answer at startup, the program prints `waiting for vehicle: ...` and tries again every 2 s, so you can start it before the ignition is on. If a PID fails while polling, the error is printed and the cycle moves on to the next one. If the vehicle stops responding, the driver reports it and tries again on the next cycle. You can try both by typing `engineoff` in the emulator console. If the connection to the adapter drops while polling, the program prints `connection lost: ...`, reconnects and runs the AT setup again. It makes up to 5 attempts, waiting 1, 2, 4 and 8 s between them, and prints `reconnected` when it's back or exits with the last error. To try it, stop the emulator and start it again within about 15 s. A connection that fails at startup isn't retried, because that usually means a wrong address or port. The emulator's default scenario always reports the throttle at 100 %. Run `elm -n 35000 -s car` for values that change.

### Cargo features

Serial support and the CLI are behind Cargo features. Both are on by default.

| Feature  | Enables                                       | Dependency   |
|----------|-----------------------------------------------|--------------|
| `serial` | `SerialTransport`                             | `serialport` |
| `cli`    | the `obdient` binary (it also needs `serial`) | `clap`       |

To use the library without them, turn the defaults off. The ESP32-S3 firmware does this, because `serialport` doesn't build for ESP-IDF:

```toml
obdient = { path = "../obdient", default-features = false }
```

## ESP32-S3 firmware

`firmware/` runs the same driver on an ESP32-S3 with ESP-IDF (Rust std). For now it joins Wi-Fi and polls the emulator over TCP, using the same `TcpTransport` and polling loop as the CLI, and writes the readings to the ESP-IDF log. On the board I checked that it reads the same values as the CLI, reports when the vehicle stops responding (`engineoff`), reconnects when the emulator is stopped and started again, and gives up on a connect after 5 s.

It needs the Xtensa Rust toolchain and two tools:

```sh
cargo install --locked espup ldproxy cargo-espflash
espup install                      # then load ~/export-esp.sh in your shell profile
```

The Wi-Fi credentials and the emulator's address are read at compile time from the environment. Put them in `firmware/.env`, which git ignores:

```sh
WIFI_SSID=...
WIFI_PASS=...
OBD_TCP_ADDRESS=192.168.1.20:35000   # the computer running the emulator
```

Then build, flash and open the monitor:

```sh
cd firmware
set -a; source .env; set +a
cargo espflash flash --monitor --baud 921600
```

- The first build downloads ESP-IDF v6.1 and its tools into `firmware/.embuild/`, so it takes a while.
- The ESP32-S3 only does 2.4 GHz Wi-Fi, and the emulator has to accept connections from the network, not only from `localhost`.
- Flash with `cargo espflash`, not `cargo run`. It writes the bootloader and partition table built by ESP-IDF together with the app. With Wi-Fi the app is bigger than 1 MB, so the firmware uses ESP-IDF's large single-app partition table.

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

- **Transport** is a trait with `read`, `write_all` and `reconnect`. It doesn't know anything about OBD, so adding serial didn't touch the protocol code.
- **Session events.** The polling loop lives in the library (`session::run`), so the CLI and the firmware share it. The library never prints. It reports what happens (a cycle's readings, waiting for the vehicle, a lost connection, a failed reconnect attempt) as a `SessionEvent` passed to a closure, and each program decides how to show it: the CLI prints to the terminal, the firmware writes to the ESP-IDF log. Clippy's `print_stdout` and `print_stderr` lints keep prints out of the library.
- **Reconnect closes first.** Both transports close the old connection before opening the new one, even if the new one then fails. Serial ports are opened in exclusive mode, so opening again while the old one is still open fails with EBUSY. A test against a real pty caught this. Over TCP, an adapter that serves one client at a time might keep serving the old connection and never take the new one. After a failed reconnect, reads and writes return `Disconnected`.
- **Connect timeout.** A TCP connect gives up after 5 s. Without it, a host that doesn't answer blocks for the OS timeout, which is about 75 s on macOS and a couple of minutes on Linux. An address can resolve to more than one (`localhost` is `::1` and `127.0.0.1`), so each one is tried in turn. A connect timeout is reported as an I/O error, not as the read `Timeout`, because it means the connection couldn't be made. Name resolution isn't covered by the timeout, but adapters are usually reached by IP address.
- **Framing.** TCP and serial are byte streams, so an answer can arrive in pieces. `Elm327` keeps reading until the `>` prompt, with an overall deadline and a size limit so a stuck or noisy adapter can't block it forever.
- **Search timeout.** After a command, the adapter can spend several seconds looking for the vehicle's protocol. It prints `SEARCHING...` first, and a new byte would abort the search. While that marker is in the answer, the deadline grows from 2 s to 15 s. The 15 s is an estimate I haven't checked on a real adapter.
- **Resync.** If a command fails, a late answer might still be on its way. Before sending the next command, the driver reads until the pending `>` arrives, so that late answer isn't read as the response to the new one. If the line stays silent until the deadline, it assumes the prompt was lost and carries on. After a corrupt answer the prompt has already arrived, so it only clears what is left on the line.
- **Parsing** handles responses with and without spaces, several ECUs answering, `SEARCHING...`, and the adapter's error messages (`NO DATA`, `?`, `UNABLE TO CONNECT`, bus errors).
- **Errors** are typed (`ObdError`, `TransportError`). I/O errors keep the OS message. Each error also has a kind (transient, PID unavailable, vehicle unavailable, connection lost), and the poller decides what to do from that kind.
- **Retries.** Transient errors are retried up to 3 attempts in total, with exponential backoff between them (100 ms, then 200 ms, capped at 1 s). If they run out, the failure is reported and the cycle moves on to the next PID. In the worst case a cycle takes about 30 s plus backoff: 5 PIDs × 3 attempts × 2 s response timeout. A protocol search can make it longer.
- **Supported PIDs.** At startup the driver asks `0100`, then each later range the previous one announces, up to `01E0`. `NO DATA` on a later range keeps the ranges already read. The emulator does exactly this: its `0140` answer announces the `0160` range, and then it answers `NO DATA` for it. On the first range it counts as the vehicle not answering.
- **`?` and `NO DATA`.** After startup, a PID is only dropped from polling when the adapter answers `?`. `NO DATA` doesn't drop it, because a slow or busy ECU can answer that too.
- **Vehicle not responding.** `NO DATA` alone can't tell an unsupported PID from a car with the ignition off. At startup the driver keeps asking for the supported PIDs until the vehicle answers. While polling, a cycle with no readings and at least one `NO DATA` counts as the vehicle not responding: nothing is dropped and the next cycle tries again.
- **Reconnect.** If the connection is lost while polling, the poller reconnects the transport and runs the AT setup again. First it clears the resync state of the old connection, so the first command doesn't wait for a `>` that can't arrive on the new one. It keeps the PIDs it was polling and doesn't ask `0100` again, since it's the same adapter and the same vehicle. Each attempt covers both steps, and any error counts as a failed attempt. After 5 attempts it gives up. The limit is needed even when the error looks recoverable: when a USB adapter is unplugged its device path disappears (ENOENT), which maps to `Disconnected`, so without a limit it would retry forever. The cycle that was cut is lost, not resumed.

Retries and `?` are tested with the mock transport only. Runs against the emulator cover normal polling, the vehicle not responding (`engineoff`), waiting at startup until it answers, recovery when it answers again, `NO DATA` for some PIDs in one cycle, and the connection dropping: stopping the emulator and starting it again resumes the readings, and leaving it stopped exits after 5 failed attempts. The reconnect itself (number of attempts, the last error, running the AT setup again) is also covered with the mock transport, and so are the session events and their order. I checked the connect timeout by hand against 192.0.2.1, an address reserved for documentation that nobody answers: the program gives up after 5 s. The resync bug with late answers depended on read timing and I never reproduced it on demand, so a unit test with the mock transport covers that fix.

## Tests

```sh
cd obdient
cargo test
```

This runs the unit tests (using a mock transport, so no hardware or network is needed) and some integration tests for the TCP transport against a local listener.

CI also runs clippy and the tests without the default features, so code that needs `serialport` or `clap` can't slip into the core library:

```sh
cargo test --no-default-features
```

The serial integration tests use pseudo-terminal pairs. They need Linux, so on macOS only two of them run. There is a script that runs the whole suite in a Docker container:

```sh
cd obdient
scripts/run-linux-tests.sh
```

There are also tests against the emulator. They are ignored by default because they need it running. The emulator serves one client at a time, so these tests take a lock and run one after another:

```sh
elm -n 35000
cargo test --test emulator_tcp -- --ignored
```

## Project layout

```
obdient/
├── src/
│   ├── main.rs          CLI
│   ├── session.rs       polling loop shared by the CLI and the firmware
│   ├── poller.rs        polling cycle
│   ├── poller/          retry policy
│   ├── elm327.rs        ELM327 protocol layer
│   ├── pid.rs           PIDs, response parsing and decoding
│   ├── error/           ObdError and TransportError
│   ├── transport.rs     Transport trait
│   └── transport/       TCP and serial transports, and a mock used in tests
├── scripts/             Linux test runner (Docker)
└── tests/               integration tests (TCP, serial, emulator)
firmware/                ESP32-S3 firmware (ESP-IDF): polls the emulator over Wi-Fi
```

## Roadmap

- [x] Serial transport, for USB adapters and the emulator's pseudo-terminal
- [x] Retry policy: bounded retries with backoff, driven by the error kind
- [x] Read the supported PIDs from the ECU (`0100`) at startup
- [x] Reconnect when the adapter goes away
- [x] Connection timeout for Wi-Fi adapters
- [ ] Try it with a real ELM327 adapter
- [x] Run it on my ESP32-S3 over Wi-Fi against the emulator
- [ ] Run it on my ESP32-S3 talking to the adapter over UART

## License

MIT, see [LICENSE](LICENSE).
