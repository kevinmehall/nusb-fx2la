use std::time::Duration;

use futures_lite::{FutureExt, StreamExt};
use nusb::{
    hotplug::HotplugEvent,
    transfer::{ControlOut, ControlType, Recipient},
};

use crate::Error;

/// Poke data into FX2 RAM
async fn fx2_write(intf: &nusb::Interface, addr: u16, data: &[u8]) -> Result<(), Error> {
    intf.control_out(
        ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Device,
            request: 0xa0,
            value: addr,
            index: 0x0000,
            data,
        },
        Duration::from_millis(100),
    )
    .await?;
    Ok(())
}

const CPUCS: u16 = 0xe600;

/// Assert or deassert the FX2 CPU reset
async fn fx2_reset_cpu(intf: &nusb::Interface, reset: bool) -> Result<(), Error> {
    fx2_write(intf, CPUCS, &[reset as u8]).await
}

/// Load firmware to a FX2 device and wait for it to reconnect
pub async fn load_firmware(
    di: nusb::DeviceInfo,
    firmware: &[u8],
) -> Result<nusb::DeviceInfo, Error> {
    log::info!("Loading firmware to device");
    let mut events = nusb::watch_devices()?;
    let dev = di.open().await?;
    let intf = dev.claim_interface(0).await?;

    fx2_reset_cpu(&intf, true).await?;
    let mut addr = 0;
    for chunk in firmware.chunks(4096) {
        fx2_write(&intf, addr, chunk).await?;
        addr += chunk.len() as u16;
    }
    fx2_reset_cpu(&intf, false).await?;

    drop(intf);
    drop(dev);

    async {
        loop {
            match events.next().await.unwrap() {
                HotplugEvent::Disconnected(disconnected) if di.id() == disconnected => {
                    log::info!("Device disconnected after firmware load");
                }
                HotplugEvent::Connected(connected)
                    if di.bus_id() == connected.bus_id()
                        && di.port_chain() == connected.port_chain()
                        && di.vendor_id() == connected.vendor_id()
                        && connected.product_id() == connected.product_id() =>
                {
                    log::info!("Device reconnected after firmware load");
                    break Ok(connected);
                }
                _ => {}
            }
        }
    }
    .race(async {
        async_io::Timer::after(Duration::from_secs(5)).await;
        Err(Error::Other(
            "Timeout waiting for device to reconnect after firmware load".into(),
        ))
    })
    .await
}
