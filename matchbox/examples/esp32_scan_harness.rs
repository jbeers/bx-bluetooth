use esp_idf_svc::hal::task::block_on;
use esp32_nimble::{BLEDevice, BLEScan};
use log::{info, warn, LevelFilter};

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::set_max_level(LevelFilter::Debug);

    info!("bx-bluetooth ESP32 scan harness starting");

    block_on(async {
        let ble_device = BLEDevice::take();
        let mut ble_scan = BLEScan::new();
        ble_scan.active_scan(true).interval(100).window(99);

        info!("starting scan");
        ble_scan
            .start(ble_device, 5_000, |device, data| {
                let name = data.name().map(|value| value.to_string()).unwrap_or_default();
                info!("device addr={:?} name={}", device.addr(), name);
                None::<()>
            })
            .await?;

        info!("scan completed");
        anyhow::Ok(())
    })?;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(10));
        warn!("scan harness idle");
    }
}
