# ESP32-S3 firmware

Back to the [README](../README.md).

`firmware/` runs the same driver on an ESP32-S3 (ESP32-S3-DevKitC-1) with ESP-IDF (Rust std). It polls the adapter over the board's UART with the same polling loop as the CLI (`session::run`) and writes the readings to the ESP-IDF log. On the bench a USB-UART adapter stands in for a UART ELM327, so the board talks UART and the emulator answers from the computer at the other end of the cable. In the car, the laptop bridges that same adapter to a USB ELM327 (see [Real car through the laptop](#real-car-through-the-laptop)).

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

The ESP32-S3 pins are 3.3 V only. Set the adapter to 3.3 V logic (mine, an FT232RL board, has a jumper for it) and leave its VCC pin unconnected. On the bench the board and the adapter also share ground through the laptop's USB ports, so a missing GND wire goes unnoticed there, and wiring the board's GND to the adapter's VCC by mistake shorts the adapter's 3.3 V output, so it stops transmitting. In the car the GND wire is the only common ground.

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
- If a session ends, because the AT setup fails, the reconnect attempts run out or no PID is left to poll, the firmware logs it and starts a new session 5 s later. The board never stops polling on its own.

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

## Real car through the laptop

A USB ELM327 can't be wired to the board, since both are USB devices. So in the car the laptop joins the two serial ports with `socat`: the board talks UART to the FT232RL at 38400 baud, and `socat` forwards every byte to the vLinker at 115200.

```sh
socat -d -d -v /dev/cu.usbserial-<FT232RL>,ispeed=38400,ospeed=38400,rawer,clocal=1 /dev/cu.usbserial-<vLinker>,ispeed=115200,ospeed=115200,rawer,clocal=1
```

- Each side keeps its own baud rate.
- socat 1.8 on macOS has no `b38400` option, so use `ispeed` and `ospeed`.
- `-v` prints the bytes in each direction (`>` board to adapter, `<` adapter to board). `-d -d` only prints notices.
- Start `socat` before the board, because the AT setup runs right after boot.
- `socat` exits if one of the devices disappears.

## UART transport

`UartTransport` (`firmware/src/transport/uart.rs`) implements obdient's `Transport` trait over the ESP-IDF UART driver, so the protocol, poller and session code didn't change. It lives in the firmware because that driver only exists on the ESP32. A read that gets nothing within the timeout (`ESP_ERR_TIMEOUT`) becomes `TransportError::Timeout`. There's no connection to reopen, so `reconnect` only clears the receive buffer.

## What I checked on the board

- Over UART it logs the same readings as `wifi_tcp` against the same emulator, cycle by cycle, and reports when the vehicle stops responding (`engineoff`).
- Over Wi-Fi it also reads the same values as the CLI, reconnects when the emulator is stopped and started again, and gives up on a connect after 5 s.
- A UART has no connection, so a gone adapter only makes the line go silent. The driver counts a command that gets no byte at all within 2 s as a lost connection (see Silent adapter in [design](design.md#decisions)). With the emulator stopped:
  - `connection lost: adapter sent nothing before the response timeout` 2.1 s after the first unanswered command.
  - Four failed reconnect attempts, each a silent `ATI` (2.1 s) plus the backoff, and `session ended: ..., restarting in 5s` 25.5 s after the loss. There's no "attempt 5/5" line: the last attempt's error is the one `session ended` shows.
  - Then a new session every 7.1 s (the 5 s wait plus the silent `ATI`), without a reset.
  - With the emulator back, the next session read again 3.8 s after it started.
- If the adapter answers again during the reconnect attempts, the session carries on and logs `reconnected`. A unit test covers that; on the board I only let the attempts run out.
- The emulator doesn't exit when its serial device disappears: it keeps looping on read errors. After plugging the adapter back in, stop it and start it again.
- On the 2012 Volkswagen, through the laptop bridge, it reads the same values as the CLI: idle around 780 rpm, revs up to 2432 rpm, one cycle about every 1.27 s. Unplugging the adapter from the car gives `CAN ERROR` on every PID for two cycles, then the readings come back without a reset. Since then a cycle like that is reported as the vehicle not responding; I haven't re-checked it in the car. With the ignition off it reports that the vehicle is not responding, and it picks up again once the engine starts.
- Every board reset puts one stray byte on the TX line. The emulator ignores it, but a real ELM327 keeps it until the next CR, so it read `<byte>ATZ` and answered `?`, and the session ended. The driver now sends a throwaway `ATI` before `ATZ` to absorb it (see [design](design.md#decisions)).
