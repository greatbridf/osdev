use super::Port8;

const COUNT: Port8 = Port8::new(0x40);
const CONTROL: Port8 = Port8::new(0x43);

pub fn init() {
    arch::interrupt::disable();
    // Set interval
    CONTROL.write(0x34);

    // Send interval number
    // 0x2e9a = 11930 = 100Hz
    COUNT.write(0x9a);
    COUNT.write(0x2e);
    arch::interrupt::enable();
}
