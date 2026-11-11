use std::time::Duration;

use esp_idf_svc::hal::{
    delay::TickType,
    gpio::{self, Gpio17, Gpio18},
    peripherals::Peripherals,
    uart::{UartDriver, config},
    units::Hertz,
};

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals: Peripherals = Peripherals::take()?;
    let tx: Gpio17 = peripherals.pins.gpio17;
    let rx: Gpio18 = peripherals.pins.gpio18;

    println!("Starting UART AT test");
    let config = config::Config::new().baudrate(Hertz(38_400));
    let uart: UartDriver = UartDriver::new(
        peripherals.uart1,
        tx,
        rx,
        Option::<gpio::Gpio0>::None,
        Option::<gpio::Gpio1>::None,
        &config,
    )?;

    loop {
        let mut buf = [0_u8; 64];
        let mut response: Vec<u8> = vec![];
        uart.write(b"ATI\r")?;

        while !response.contains(&b'>') {
            match uart.read(&mut buf, TickType::new_millis(1000).ticks()) {
                Ok(len) => {
                    response.extend_from_slice(&buf[..len]);
                }
                Err(error) if error.code() == esp_idf_svc::sys::ESP_ERR_TIMEOUT => {
                    log::warn!("no answer");
                    break;
                }
                Err(error) => return Err(error.into()),
            };
        }

        log::info!("READ {:?}", String::from_utf8_lossy(&response));

        std::thread::sleep(Duration::from_secs(2));
    }
}
