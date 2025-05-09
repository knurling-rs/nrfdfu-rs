pub struct TargetDevice<'l> {
    pub name: &'l str,
    pub vendor_id: u16,
    pub product_id: u16
}

const TEST_DEVICE: TargetDevice = TargetDevice { name: "Nordic Semiconductor ASA OpenSK Serial: v1.0", vendor_id: 0x1915, product_id: 0x521f };
const BLUEFRUIT: TargetDevice = TargetDevice { name: "Adafruit Feather 32u4 Bluefruit", vendor_id: 0x10c4, product_id: 0xea60 };
pub const KNOWN_USB_DEVICE_TARGETS: &'static [TargetDevice] = &[TEST_DEVICE, BLUEFRUIT];
