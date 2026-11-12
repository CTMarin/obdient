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

    let uart_transport: UartTransport = UartTransport::new(uart, READ_TIMEOUT);
    let session_config: SessionConfig = SessionConfig::default();
    if let Err(error) = session::run(uart_transport, &session_config, log_event) {
        log::error!("session ended: {error}");
        return Err(error.into());
    };

    log::warn!("no PIDs left to poll");

    Ok(())
}
