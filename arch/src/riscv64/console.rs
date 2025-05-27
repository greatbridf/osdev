use sbi::legacy::{console_putchar, console_getchar};

pub fn getchar() -> Option<u8> {
    let c = console_getchar();
    if c == Some(!0) {
        None
    } else {
        c
    }
}

fn write_str(s: &str) {
    for c in s.chars() {
        console_putchar(c as u8);
    }
}
