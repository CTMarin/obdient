use std::time::Duration;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripherals::Peripherals,
    ipv4::IpInfo,
    nvs::EspDefaultNvsPartition,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use obdient::{
    poller::CycleReport,
    session::{self, SessionConfig, SessionEvent},
    transport::tcp::TcpTransport,
};

const SSID: &str = env!("WIFI_SSID");
const PASSWORD: &str = env!("WIFI_PASS");
const OBD_TCP_ADDRESS: &str = env!("OBD_TCP_ADDRESS");

const READ_TIMEOUT: Duration = Duration::from_millis(100);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals: Peripherals = Peripherals::take()?;
    let sys_loop: EspSystemEventLoop = EspSystemEventLoop::take()?;
    let nvs: EspDefaultNvsPartition = EspDefaultNvsPartition::take()?;

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;
    connect_wifi(&mut wifi)?;
    let ip_info: IpInfo = wifi.wifi().sta_netif().get_ip_info()?;
    log::info!("Wifi DHCP info: {ip_info:?}");

    let tcp_transport: TcpTransport =
        TcpTransport::connect(OBD_TCP_ADDRESS, READ_TIMEOUT, CONNECT_TIMEOUT)?;
    let config: SessionConfig = SessionConfig::default();
    if let Err(error) = session::run(tcp_transport, &config, |event| match event {
        SessionEvent::Unsupported(pid) => log::warn!("PID {pid:?}: not supported by vehicle"),
        SessionEvent::Report(report) => print_report(report),
        SessionEvent::VehicleUnavailable(error) => log::warn!("{error}"),
        SessionEvent::ConnectionLost(error) => {
            log::warn!("connection lost: {error}, reconnecting")
        }
        SessionEvent::Reconnected => log::info!("reconnected"),
        SessionEvent::WaitingForVehicle(error) => log::warn!("waiting for vehicle: {error}"),
        SessionEvent::ReconnectAttemptFailed {
            attempt,
            max_attempts,
            error,
        } => log::warn!("reconnect attempt {attempt}/{max_attempts} failed: {error}"),
    }) {
        log::error!("session ended: {error}");
        return Err(error.into());
    };

    log::warn!("no PIDs left to poll");

    Ok(())
}

fn print_report(report: &CycleReport) {
    for reading in &report.readings {
        log::info!("{reading}");
    }
    for (pid, error) in &report.failures {
        log::warn!("PID {pid:?}: {error}");
    }
    for pid in &report.newly_unavailable {
        log::warn!("PID {pid:?}: marked unavailable")
    }
    log::info!("----------------")
}

fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<()> {
    let wifi_configuration: Configuration = Configuration::Client(ClientConfiguration {
        ssid: SSID
            .try_into()
            .map_err(|_| anyhow::anyhow!("SSID longer than 32 bytes"))?,
        bssid: None,
        auth_method: AuthMethod::WPA2Personal,
        password: PASSWORD
            .try_into()
            .map_err(|_| anyhow::anyhow!("PASSWORD longer than 64 bytes"))?,
        channel: None,
        ..Default::default()
    });

    wifi.set_configuration(&wifi_configuration)?;

    wifi.start()?;
    log::info!("Wifi started");
    wifi.connect()?;
    log::info!("Wifi connected");
    wifi.wait_netif_up()?;
    log::info!("Wifi netif up");

    Ok(())
}
