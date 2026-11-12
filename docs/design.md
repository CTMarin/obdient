# Design

How obdient behaves at run time and why it's built this way. Back to the [README](../README.md).

## Behavior

If the vehicle doesn't answer at startup, the program prints `waiting for vehicle: ...` and tries again every 2 s, so you can start it before the ignition is on. If a PID fails while polling, the error is printed and the cycle moves on to the next one. If the vehicle stops responding, the driver reports it and tries again on the next cycle. You can try both by typing `engineoff` in the emulator console. If the connection to the adapter drops while polling, the program prints `connection lost: ...`, reconnects and runs the AT setup again. It makes up to 5 attempts, waiting 1, 2, 4 and 8 s between them, and prints `reconnected` when it's back or exits with the last error. To try it, stop the emulator and start it again within about 15 s. A connection that fails at startup isn't retried, because that usually means a wrong address or port. The emulator's default scenario always reports the throttle at 100 %. Run `elm -n 35000 -s car` for values that change.

## Decisions

- **Transport** is a trait with `read`, `write_all` and `reconnect`. It doesn't know anything about OBD, so adding serial didn't touch the protocol code. The firmware's UART transport plugs in the same way.
- **Session events.** The polling loop lives in the library (`session::run`), so the CLI and the firmware share it. The library never prints. It reports what happens (a cycle's readings, waiting for the vehicle, a lost connection, a failed reconnect attempt) as a `SessionEvent` passed to a closure, and each program decides how to show it: the CLI prints to the terminal, the firmware writes to the ESP-IDF log. Clippy's `print_stdout` and `print_stderr` lints keep prints out of the library.
- **Reconnect closes first.** Both transports close the old connection before opening the new one, even if the new one then fails. Serial ports are opened in exclusive mode, so opening again while the old one is still open fails with EBUSY. A test against a real pty caught this. Over TCP, an adapter that serves one client at a time might keep serving the old connection and never take the new one. After a failed reconnect, reads and writes return `Disconnected`.
- **Connect timeout.** A TCP connect gives up after 5 s. Without it, a host that doesn't answer blocks for the OS timeout, which is about 75 s on macOS and a couple of minutes on Linux. An address can resolve to more than one (`localhost` is `::1` and `127.0.0.1`), so each one is tried in turn. A connect timeout is reported as an I/O error, not as the read `Timeout`, because it means the connection couldn't be made. Name resolution isn't covered by the timeout, but adapters are usually reached by IP address.
- **Framing.** TCP and serial are byte streams, so an answer can arrive in pieces. `Elm327` keeps reading until the `>` prompt, with an overall deadline and a size limit so a stuck or noisy adapter can't block it forever.
- **Search timeout.** After a command, the adapter can spend several seconds looking for the vehicle's protocol. It prints `SEARCHING...` first, and a new byte would abort the search. While that marker is in the answer, the deadline grows from 2 s to 15 s. The 15 s is an estimate. On a real adapter the search took about 1 s with the car connected, and about 7 s without a car before `UNABLE TO CONNECT`, so it leaves room. I've only checked one car.
- **Resync.** If a command fails, a late answer might still be on its way. Before sending the next command, the driver reads until the pending `>` arrives, so that late answer isn't read as the response to the new one. If the line stays silent until the deadline, it assumes the prompt was lost and carries on. After a corrupt answer the prompt has already arrived, so it only clears what is left on the line.
- **Parsing** handles responses with and without spaces, several ECUs answering, `SEARCHING...`, and the adapter's error messages (`NO DATA`, `?`, `UNABLE TO CONNECT`, bus errors).
- **Errors** are typed (`ObdError`, `TransportError`). I/O errors keep the OS message. Each error also has a kind (transient, PID unavailable, vehicle unavailable, connection lost), and the poller decides what to do from that kind.
- **Retries.** Transient errors are retried up to 3 attempts in total, with exponential backoff between them (100 ms, then 200 ms, capped at 1 s). If they run out, the failure is reported and the cycle moves on to the next PID. A silent line is the worst case: each attempt waits 2 s for the answer, and every command after a failed one first waits up to 2 s for a late `>` (see Resync). With 5 PIDs a cycle then takes about 1 min (15 × 2 s + 14 × 2 s, plus backoff). On the ESP32-S3, with 4 PIDs, it took about 52 s. A protocol search can make it longer.
- **Supported PIDs.** At startup the driver asks `0100`, then each later range the previous one announces, up to `01E0`. `NO DATA` on a later range keeps the ranges already read. The emulator does exactly this: its `0140` answer announces the `0160` range, and then it answers `NO DATA` for it. On the first range it counts as the vehicle not answering.
- **`?` and `NO DATA`.** After startup, a PID is only dropped from polling when the adapter answers `?`. `NO DATA` doesn't drop it, because a slow or busy ECU can answer that too.
- **Vehicle not responding.** `NO DATA` alone can't tell an unsupported PID from a car with the ignition off. At startup the driver keeps asking for the supported PIDs until the vehicle answers. While polling, a cycle with no readings and at least one `NO DATA` counts as the vehicle not responding: nothing is dropped and the next cycle tries again.
- **Reconnect.** If the connection is lost while polling, the poller reconnects the transport and runs the AT setup again. First it clears the resync state of the old connection, so the first command doesn't wait for a `>` that can't arrive on the new one. It keeps the PIDs it was polling and doesn't ask `0100` again, since it's the same adapter and the same vehicle. Each attempt covers both steps, and any error counts as a failed attempt. After 5 attempts it gives up. The limit is needed even when the error looks recoverable: when a USB adapter is unplugged its device path disappears (ENOENT), which maps to `Disconnected`, so without a limit it would retry forever. The cycle that was cut is lost, not resumed.

## Cargo features

Serial support and the CLI are behind Cargo features. Both are on by default.

| Feature  | Enables                                       | Dependency   |
|----------|-----------------------------------------------|--------------|
| `serial` | `SerialTransport`                             | `serialport` |
| `cli`    | the `obdient` binary (it also needs `serial`) | `clap`       |

To use the library without them, turn the defaults off. The ESP32-S3 firmware does this, because `serialport` doesn't build for ESP-IDF:

```toml
obdient = { path = "../obdient", default-features = false }
```
