fn main() {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let raw: &str = "41 0C 1A F8\r\r";

    let pid: obdient::pid::Pid = obdient::pid::Pid::EngineSpeed;
    match obdient::pid::parse_response(raw, pid).and_then(|data| obdient::pid::decode(pid, &data)) {
        Ok(reading) => log::info!("{}", reading),
        Err(error) => log::error!("{}", error),
    }
}
