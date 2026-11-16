use std::time::Duration;

use esp_idf_svc::hal::{
    gpio::{Gpio0, Gpio1, Gpio17, Gpio18},
    peripherals::Peripherals,
    uart::{UartConfig, UartDriver},
    units::Hertz,
};
use firmware::{log_event, transport::uart::UartTransport};
use obdient::session::{self, SessionConfig};

const READ_TIMEOUT: Duration = Duration::from_millis(100);
const SESSION_RESTART_DELAY: Duration = Duration::from_secs(5);

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals: Peripherals = Peripherals::take()?;
    let tx: Gpio17 = peripherals.pins.gpio17;
    let rx: Gpio18 = peripherals.pins.gpio18;
    let uart_config: UartConfig = UartConfig::new().baudrate(Hertz(38_400));
    let uart: UartDriver = UartDriver::new(
        peripherals.uart1,
        tx,
        rx,
        Option::<Gpio0>::None,
        Option::<Gpio1>::None,
        &uart_config,
    )?;

    let mut uart_transport: UartTransport = UartTransport::new(uart, READ_TIMEOUT);
    let session_config: SessionConfig = SessionConfig::default();

    loop {
        match session::run(&mut uart_transport, &session_config, log_event) {
            Ok(()) => log::warn!("no PIDs left to poll, restarting in {SESSION_RESTART_DELAY:?}"),
            Err(error) => {
                log::error!("session ended: {error}, restarting in {SESSION_RESTART_DELAY:?}")
            }
        }
        std::thread::sleep(SESSION_RESTART_DELAY);
    }
}
