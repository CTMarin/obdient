# rustobd

A small Rust driver for ELM327 OBD-II adapters. It connects to the adapter, runs the usual AT setup and reads live engine data in a loop.

I started it as a way to learn how to talk to a real device from Rust. Most of it is the boring-but-important part: framing responses, timeouts, and not getting confused when the adapter misbehaves.

## Status

Early, but it works end to end over TCP against [ELM327-emulator](https://github.com/Ircama/ELM327-emulator). I haven't tried it with a physical adapter yet, and there is no serial support yet (see the [roadmap](#roadmap)).

## What it reads

| PID    | Value               | Unit |
|--------|---------------------|------|
| `010C` | Engine speed        | rpm  |
| `010D` | Vehicle speed       | km/h |
| `0105` | Coolant temperature | °C   |
| `0111` | Throttle position   | %    |

## Usage

You need Rust 1.85 or newer (the crate uses edition 2024).

The easiest way to try it is the emulator, which speaks the same protocol as a real adapter:

```sh
uv tool install ELM327-emulator    # or: pip install ELM327-emulator
elm -n 35000                       # listens on TCP port 35000
```

Then, in another terminal:

```sh
cd rustobd
cargo run -- --tcp 127.0.0.1:35000
```

```
620 rpm
4 km/h
83 °C
100.0 %
----------------
```

| Option             | Default  | Description                        |
|--------------------|----------|------------------------------------|
| `--tcp <ADDRESS>`  | required | Adapter address, `host:port`       |
| `--interval-ms <N>`| `1000`   | Pause between polling cycles (ms)  |

If a PID fails, the error is printed and the loop moves on to the next one. You can see this by typing `engineoff` in the emulator console. The emulator's default scenario always reports the throttle at 100 %. Run `elm -n 35000 -s car` for values that change.

## How it works

The code is split into layers, each of which only knows about the one below:

```
main.rs     CLI and polling loop
elm327.rs   ELM327 protocol: AT setup, send a command, read the answer
pid.rs      parse the text response and decode the value
transport   moves bytes (TCP for now)
```

- **Transport** is a trait with `read`, `write_all` and `reconnect`. It doesn't know anything about OBD, so adding serial should not touch the protocol code.
- **Framing.** TCP and serial are byte streams, so an answer can arrive in pieces. `Elm327` keeps reading until the `>` prompt, with an overall deadline and a size limit so a stuck or noisy adapter can't block it forever.
- **Resync.** If a command fails, a late answer might still be on its way. Before sending the next command, the driver drains whatever is left on the line, so that late answer isn't read as the response to the new one.
- **Parsing** handles responses with and without spaces, several ECUs answering, `SEARCHING...`, and the adapter's error messages (`NO DATA`, `?`, `UNABLE TO CONNECT`, bus errors).
- **Errors** are typed (`ObdError`, `TransportError`). Each error also has a kind (transient, PID unavailable, vehicle unavailable, connection lost) so that a retry policy can decide what to do. That policy isn't written yet.

## Tests

```sh
cd rustobd
cargo test
```

This runs the unit tests (using a mock transport, so no hardware or network is needed) and some integration tests for the TCP transport against a local listener.

There are also tests against the emulator. They are ignored by default because they need it running:

```sh
elm -n 35000
cargo test --test emulator_tcp -- --ignored
```

## Project layout

```
rustobd/
├── src/
│   ├── main.rs          CLI
│   ├── elm327.rs        ELM327 protocol layer
│   ├── pid.rs           PIDs, response parsing and decoding
│   ├── error/           ObdError and TransportError
│   ├── transport.rs     Transport trait
│   └── transport/       TCP transport, and a mock used in tests
└── tests/               integration tests (TCP, emulator)
```

## Roadmap

- [ ] Serial transport, for USB adapters and the emulator's pseudo-terminal
- [ ] Retry policy: bounded retries with backoff, driven by the error kind
- [ ] Reconnect when the adapter goes away
- [ ] Connection timeout for Wi-Fi adapters
- [ ] Try it with a real ELM327 adapter
- [ ] Run it on my ESP32-S3 talking to the adapter over UART
