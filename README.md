# obdient

A small Rust driver for ELM327 OBD-II adapters. It connects to the adapter, runs the usual AT setup and reads live engine data in a loop.

I started it as a way to learn how to talk to a real device from Rust. Most of it is the boring-but-important part: framing responses, timeouts, and not getting confused when the adapter misbehaves.

## Status

Early, but it works end to end over TCP against [ELM327-emulator](https://github.com/Ircama/ELM327-emulator). There is now a serial transport too. I tested it against pseudo-terminals on Linux, but I haven't run it against the emulator or a physical adapter yet. On macOS `serialport` can't open a pseudo-terminal (see [Usage](#usage)). See the [roadmap](#roadmap).

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

If the vehicle doesn't answer at startup, the program prints `waiting for vehicle: ...` and tries again every 2 s, so you can start it before the ignition is on. If a PID fails while polling, the error is printed and the cycle moves on to the next one. If the vehicle stops responding, the driver reports it and tries again on the next cycle. You can try both by typing `engineoff` in the emulator console. The emulator's default scenario always reports the throttle at 100 %. Run `elm -n 35000 -s car` for values that change.

## How it works

The code is split into layers, each of which only knows about the one below:

```
main.rs     CLI and printing
poller.rs   startup wait and one polling cycle: retries and what to do with each error
elm327.rs   ELM327 protocol: AT setup, send a command, read the answer
pid.rs      parse the text response and decode the value
transport   moves bytes (TCP or serial)
```

- **Transport** is a trait with `read`, `write_all` and `reconnect`. It doesn't know anything about OBD, so adding serial didn't touch the protocol code.
- **Serial reconnect** closes the old port before opening the new one. Ports are opened in exclusive mode, so opening again while the old one is still open fails with EBUSY. A test against a real pty caught this.
- **Framing.** TCP and serial are byte streams, so an answer can arrive in pieces. `Elm327` keeps reading until the `>` prompt, with an overall deadline and a size limit so a stuck or noisy adapter can't block it forever.
- **Search timeout.** After a command, the adapter can spend several seconds looking for the vehicle's protocol. It prints `SEARCHING...` first, and a new byte would abort the search. While that marker is in the answer, the deadline grows from 2 s to 15 s. The 15 s is an estimate I haven't checked on a real adapter.
- **Resync.** If a command fails, a late answer might still be on its way. Before sending the next command, the driver reads until the pending `>` arrives, so that late answer isn't read as the response to the new one. If the line stays silent until the deadline, it assumes the prompt was lost and carries on. After a corrupt answer the prompt has already arrived, so it only clears what is left on the line.
- **Parsing** handles responses with and without spaces, several ECUs answering, `SEARCHING...`, and the adapter's error messages (`NO DATA`, `?`, `UNABLE TO CONNECT`, bus errors).
- **Errors** are typed (`ObdError`, `TransportError`). I/O errors keep the OS message. Each error also has a kind (transient, PID unavailable, vehicle unavailable, connection lost), and the poller decides what to do from that kind.
- **Retries.** Transient errors are retried up to 3 attempts in total, with exponential backoff between them (100 ms, then 200 ms, capped at 1 s). If they run out, the failure is reported and the cycle moves on to the next PID. In the worst case a cycle takes about 30 s plus backoff: 5 PIDs × 3 attempts × 2 s response timeout. A protocol search can make it longer.
- **Supported PIDs.** At startup the driver asks `0100`, then each later range the previous one announces, up to `01E0`. `NO DATA` on a later range keeps the ranges already read. On the first range it counts as the vehicle not answering.
- **`?` and `NO DATA`.** After startup, a PID is only dropped from polling when the adapter answers `?`. `NO DATA` doesn't drop it, because a slow or busy ECU can answer that too.
- **Vehicle not responding.** `NO DATA` alone can't tell an unsupported PID from a car with the ignition off. At startup the driver keeps asking for the supported PIDs until the vehicle answers. While polling, a cycle with no readings and at least one `NO DATA` counts as the vehicle not responding: nothing is dropped and the next cycle tries again.
- **Connection lost.** The program exits with the error for now. Reconnecting isn't written yet.

Retries and `?` are tested with the mock transport only. Runs against the emulator cover normal polling, the vehicle not responding (`engineoff`), waiting at startup until it answers, recovery when it answers again, `NO DATA` for some PIDs in one cycle, and the exit when the connection drops. The resync bug with late answers depended on read timing and I never reproduced it on demand, so a unit test with the mock transport covers that fix.

## Tests

```sh
cd obdient
cargo test
```

This runs the unit tests (using a mock transport, so no hardware or network is needed) and some integration tests for the TCP transport against a local listener.

The serial integration tests use pseudo-terminal pairs. They need Linux, so on macOS only two of them run. There is a script that runs the whole suite in a Docker container:

```sh
cd obdient
scripts/run-linux-tests.sh
```

There are also tests against the emulator. They are ignored by default because they need it running:

```sh
elm -n 35000
cargo test --test emulator_tcp -- --ignored
```

## Project layout

```
obdient/
├── src/
│   ├── main.rs          CLI
│   ├── poller.rs        polling cycle
│   ├── poller/          retry policy
│   ├── elm327.rs        ELM327 protocol layer
│   ├── pid.rs           PIDs, response parsing and decoding
│   ├── error/           ObdError and TransportError
│   ├── transport.rs     Transport trait
│   └── transport/       TCP and serial transports, and a mock used in tests
├── scripts/             Linux test runner (Docker)
└── tests/               integration tests (TCP, serial, emulator)
```

## Roadmap

- [x] Serial transport, for USB adapters and the emulator's pseudo-terminal
- [x] Retry policy: bounded retries with backoff, driven by the error kind
- [x] Read the supported PIDs from the ECU (`0100`) at startup
- [ ] Reconnect when the adapter goes away
- [ ] Connection timeout for Wi-Fi adapters
- [ ] Try it with a real ELM327 adapter
- [ ] Run it on my ESP32-S3 talking to the adapter over UART

## License

MIT, see [LICENSE](LICENSE).
