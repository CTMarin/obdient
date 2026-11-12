# Tests

Back to the [README](../README.md).

```sh
cd obdient
cargo test
```

This runs the unit tests (using a mock transport, so no hardware or network is needed) and some integration tests for the TCP transport against a local listener.

CI also runs clippy and the tests without the default features, so code that needs `serialport` or `clap` can't slip into the core library:

```sh
cargo test --no-default-features
```

The emulator also offers a pseudo-terminal when you start it without `-n`. On macOS, `serialport` can't open pseudo-terminals (the open fails with ENOTTY, "Not a typewriter"), so use TCP there.

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

## What the tests cover

Retries and `?` are tested with the mock transport only. Runs against the emulator cover normal polling, the vehicle not responding (`engineoff`), waiting at startup until it answers, recovery when it answers again, `NO DATA` for some PIDs in one cycle, and the connection dropping: stopping the emulator and starting it again resumes the readings, and leaving it stopped exits after 5 failed attempts. The reconnect itself (number of attempts, the last error, running the AT setup again) is also covered with the mock transport, and so are the session events and their order. I checked the connect timeout by hand against 192.0.2.1, an address reserved for documentation that nobody answers: the program gives up after 5 s. The resync bug with late answers depended on read timing and I never reproduced it on demand, so a unit test with the mock transport covers that fix.
