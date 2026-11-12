# Real hardware

What I checked against real adapters and cars. The ESP32-S3 board checks are in [firmware](firmware.md#what-i-checked-on-the-board). Back to the [README](../README.md).

## 2012 Volkswagen through a USB adapter

I've run the CLI on a 2012 Volkswagen (diesel) through a vLinker USB adapter:

```sh
cargo run -- --serial /dev/cu.usbserial-XXXX --baud 115200
```

- The vLinker talks at 115200 baud, not the usual 38400, and identifies as `ELM327 v2.3`. It's powered from USB, so it answers AT commands on the desk without a car.
- Started before the car answered, the driver printed `waiting for vehicle: adapter could not connect to the vehicle` until it did, then started reading. Finding the car's protocol took about 1 s.
- Readings: cranking at 148 rpm, idle between about 740 and 810 rpm, coolant warming from 20 °C to 33 °C over 3 minutes, speed 0 km/h (parked). One cycle took about 1.2 s.
- The car doesn't announce the fuel level (`012F`), so it's reported once and skipped.
- Throttle position (`0111`) reads about 88–90 % and doesn't move when revving. On a diesel this PID most likely reports the intake throttle flap, which stays almost open, not the accelerator pedal (PIDs `0149` and `014A`, which obdient doesn't read).
- With the ignition off, every cycle reports that the vehicle is not responding, and nothing is dropped.
- The connection dropped once mid-run, most likely the USB cable coming out of the laptop. The CLI printed `connection lost: transport error: disconnected from device`, reconnected on the fourth attempt and kept reading.

Still to check: which protocol the car uses, a second car (a 2004 Ford), and unplugging the adapter from the car.
