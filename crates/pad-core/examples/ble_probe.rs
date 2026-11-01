//! Prints BLE capability and a battery read for the paired pad address (pass hex address, default the dev pad).

fn main() {
    let addr = std::env::args().nth(1).and_then(|a| u64::from_str_radix(&a, 16).ok()).unwrap_or(0xCA6FFE0A918E);
    println!("low_latency_supported = {}", pad_core::win::ble::low_latency_supported());
    println!("battery({addr:012X}) = {:?}", pad_core::win::ble::battery(addr));
    println!("firmware({addr:012X}) = {:?}", pad_core::win::ble::firmware_revision(addr));
    match pad_core::win::ble::LowLatency::request(addr) {
        Ok(_) => println!("low-latency request: Active"),
        Err(s) => println!("low-latency request: {s:?}"),
    }
    println!("rate override driver = {}", pad_core::win::rate::override_driver_present());
}
